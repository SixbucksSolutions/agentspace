//! Windows Management Instrumentation's `Win32_Process.Create`, the one call by which agentspace
//! creates a process outside the caller's job and every job above it, with the user's logon
//! environment: the keeper's launcher (docs/keeper.md, The handshake, step 2; D42) and a
//! provisioning run (docs/driver.md, Starting a provisioning run; D48, D55). agentspace-launch and
//! this crate's `WmiDetach` both call `create`.
//!
//! `Win32_ProcessStartup` always hides the process's console window (`ShowWindow` 0, `SW_HIDE`);
//! the console is hidden, never absent. It sets `CreateFlags` only when the caller passes flags,
//! as `WmiDetach` does (`CREATE_NEW_PROCESS_GROUP`), and `EnvironmentVariables` only when the
//! caller passes entries. So the launcher's call, which passes neither, sets `ShowWindow` alone,
//! and its environment is WMI's: the logon one. `request` builds what the call puts, so that it
//! can be checked without calling WMI.
//!
//! The call initializes COM on the calling thread for its own length, so a caller runs it on a
//! thread of its own. That thread owns all the call uses: its copies of the command line, the
//! folder and the entries, its own COM initialization and every interface it holds, so a caller
//! MAY abandon it, as agentspace-launch and `WmiDetach` each do at a 10-second bound: when the
//! call returns at last, the interfaces are released and the thread uninitializes COM itself and
//! ends.

use std::ffi::{OsStr, OsString};
use std::mem::ManuallyDrop;
use std::os::windows::ffi::OsStrExt;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    CoSetProxyBlanket, CoUninitialize, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayPutElement};
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Variant::{
    VARIANT, VT_ARRAY, VT_BSTR, VT_I4, VT_UI4, VT_UNKNOWN, VariantClear,
};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, IWbemServices, WBEM_FLAG_RETURN_WBEM_COMPLETE, WbemLocator,
};
use windows::core::{BSTR, IUnknown, Interface, PCWSTR, w};

/// `SW_HIDE`.
const SW_HIDE: i32 = 0;

/// `CREATE_NEW_PROCESS_GROUP`, the flag `WmiDetach` passes as `CreateFlags`.
pub const CREATE_NEW_PROCESS_GROUP: u32 = 0x200;

/// One property that `create` puts on `Win32_ProcessStartup`. It has no `Debug`, since an
/// environment block holds the logon environment's values.
pub enum Startup {
    /// `ShowWindow`.
    ShowWindow(i32),
    /// `CreateFlags`.
    CreateFlags(u32),
    /// `EnvironmentVariables`, each entry `NAME=VALUE`.
    EnvironmentVariables(Vec<OsString>),
}

impl Startup {
    /// The property's name in `Win32_ProcessStartup`.
    pub fn name(&self) -> &'static str {
        match self {
            Startup::ShowWindow(_) => "ShowWindow",
            Startup::CreateFlags(_) => "CreateFlags",
            Startup::EnvironmentVariables(_) => "EnvironmentVariables",
        }
    }
}

/// What one `Win32_Process.Create` call is given: its `CommandLine`, its `CurrentDirectory` and
/// the properties of its `ProcessStartupInformation`, in the order they are put.
pub struct Request {
    pub command_line: OsString,
    pub directory: OsString,
    pub startup: Vec<Startup>,
}

/// The request `create` sends for these inputs: `ShowWindow` 0 always, `CreateFlags` only when
/// `create_flags` is given, and `EnvironmentVariables` only when `environment` is not empty.
pub fn request(
    command_line: &OsStr,
    directory: &OsStr,
    environment: &[OsString],
    create_flags: Option<u32>,
) -> Request {
    let mut startup = vec![Startup::ShowWindow(SW_HIDE)];
    if let Some(flags) = create_flags {
        startup.push(Startup::CreateFlags(flags));
    }
    if !environment.is_empty() {
        startup.push(Startup::EnvironmentVariables(environment.to_vec()));
    }
    Request {
        command_line: command_line.to_os_string(),
        directory: directory.to_os_string(),
        startup,
    }
}

const TOO_MANY: &str = "Too many environment variables were given.";

/// A COM error as the pass reports it: Windows' text and the HRESULT.
fn describe(e: &windows::core::Error) -> String {
    let message = e.message();
    let message = message.trim_end().trim_end_matches('.');
    if message.is_empty() {
        format!("HRESULT 0x{:08X}.", e.code().0 as u32)
    } else {
        format!("{message} (HRESULT 0x{:08X}).", e.code().0 as u32)
    }
}

/// COM for this thread, uninitialized when dropped.
struct Apartment;

impl Apartment {
    fn enter() -> Result<Apartment, String> {
        // SAFETY: initializes COM for this thread only; balanced by the drop below.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr.is_err() {
            return Err(describe(&windows::core::Error::from(hr)));
        }
        Ok(Apartment)
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: balances the successful CoInitializeEx of `enter` on this thread.
        unsafe { CoUninitialize() };
    }
}

/// A VARIANT cleared when dropped.
struct Variant(VARIANT);

impl Variant {
    fn bstr(text: &OsStr) -> Variant {
        let mut v = VARIANT::default();
        // SAFETY: a zeroed VARIANT given a type and the value of that type.
        unsafe {
            (*v.Anonymous.Anonymous).vt = VT_BSTR;
            (*v.Anonymous.Anonymous).Anonymous.bstrVal = ManuallyDrop::new(bstr(text));
        }
        Variant(v)
    }

    fn i4(value: i32) -> Variant {
        let mut v = VARIANT::default();
        // SAFETY: as above.
        unsafe {
            (*v.Anonymous.Anonymous).vt = VT_I4;
            (*v.Anonymous.Anonymous).Anonymous.lVal = value;
        }
        Variant(v)
    }

    /// A one-dimensional array of strings, `VT_ARRAY | VT_BSTR`.
    fn strings(items: &[OsString]) -> Result<Variant, String> {
        let count = u32::try_from(items.len()).map_err(|_| TOO_MANY.to_string())?;
        // SAFETY: makes a new array of `count` BSTRs, each null until it is put below.
        let array = unsafe { SafeArrayCreateVector(VT_BSTR, 0, count) };
        if array.is_null() {
            return Err("An array for the variables could not be made.".to_string());
        }
        let mut v = VARIANT::default();
        // SAFETY: as above; from here the VARIANT owns the array, and destroys it on clear.
        unsafe {
            (*v.Anonymous.Anonymous).vt = VT_ARRAY | VT_BSTR;
            (*v.Anonymous.Anonymous).Anonymous.parray = array;
        }
        let v = Variant(v);
        for (index, item) in items.iter().enumerate() {
            let index = i32::try_from(index).map_err(|_| TOO_MANY.to_string())?;
            let raw = bstr(item).into_raw();
            // SAFETY: a valid array of BSTRs and an index inside it. A BSTR element is passed as
            // the BSTR itself, and the array keeps a copy of its own.
            let put = unsafe { SafeArrayPutElement(array, &index, raw.cast()) };
            // SAFETY: `raw` came from `into_raw` above, and is freed once, here.
            drop(unsafe { BSTR::from_raw(raw) });
            put.map_err(|e| describe(&e))?;
        }
        Ok(v)
    }

    fn object(object: &IWbemClassObject) -> Result<Variant, String> {
        let unknown: IUnknown = object.cast().map_err(|e| describe(&e))?;
        let mut v = VARIANT::default();
        // SAFETY: as above; the VARIANT owns the reference it is given, and drops it on clear.
        unsafe {
            (*v.Anonymous.Anonymous).vt = VT_UNKNOWN;
            (*v.Anonymous.Anonymous).Anonymous.punkVal = ManuallyDrop::new(Some(unknown));
        }
        Ok(Variant(v))
    }

    /// The value as a return code: a 32-bit integer of either sign.
    fn code(&self) -> Option<u32> {
        // SAFETY: reads the field the type names.
        unsafe {
            let inner = &self.0.Anonymous.Anonymous;
            if inner.vt == VT_I4 {
                Some(inner.Anonymous.lVal as u32)
            } else if inner.vt == VT_UI4 {
                Some(inner.Anonymous.ulVal)
            } else {
                None
            }
        }
    }
}

impl Drop for Variant {
    fn drop(&mut self) {
        // SAFETY: the VARIANT is ours and was made valid; clearing frees what it owns.
        let _ = unsafe { VariantClear(&mut self.0) };
    }
}

fn put(object: &IWbemClassObject, name: PCWSTR, value: &Variant) -> Result<(), String> {
    // SAFETY: a valid object, a NUL-ended name and a valid VARIANT; type 0 takes the property's
    // own type.
    unsafe { object.Put(name, 0, &value.0, 0) }.map_err(|e| describe(&e))
}

/// One instance of a WMI class, made from its definition.
fn instance(services: &IWbemServices, class: &str) -> Result<IWbemClassObject, String> {
    let mut definition = None;
    // SAFETY: a connected services proxy and out-parameters of ours.
    unsafe {
        services.GetObject(
            &BSTR::from(class),
            WBEM_FLAG_RETURN_WBEM_COMPLETE,
            None,
            Some(&mut definition),
            None,
        )
    }
    .map_err(|e| describe(&e))?;
    let definition = definition.ok_or_else(|| format!("WMI gave no {class} class."))?;
    // SAFETY: a valid class definition.
    unsafe { definition.SpawnInstance(0) }.map_err(|e| describe(&e))
}

/// A BSTR of exactly these UTF-16 units.
fn bstr(text: &OsStr) -> BSTR {
    BSTR::from_wide(&text.encode_wide().collect::<Vec<u16>>())
}

/// `Win32_Process.Create` with this command line and working folder, and a hidden console window:
/// the method's `ReturnValue`, 0 when the process was created, or the error of a call that failed.
///
/// `environment` is `Win32_ProcessStartup.EnvironmentVariables`, each entry `NAME=VALUE`, which
/// WMI makes the process's whole environment, in place of the logon environment rather than on
/// top of it; an empty slice sets no `EnvironmentVariables`, and the process gets the logon
/// environment. `create_flags` is `Win32_ProcessStartup.CreateFlags`, set only when given. The
/// caller MUST have refused a command line, a folder or an entry that holds a NUL, and an entry
/// whose name is empty or holds `=` (`WmiDetach` does). The call is unbounded: a bound is the
/// caller's.
pub fn create(
    command_line: &OsStr,
    directory: &OsStr,
    environment: &[OsString],
    create_flags: Option<u32>,
) -> Result<u32, String> {
    send(&request(command_line, directory, environment, create_flags))
}

/// Sends one request to `Win32_Process.Create`.
fn send(request: &Request) -> Result<u32, String> {
    let _apartment = Apartment::enter()?;
    // SAFETY: COM is initialized on this thread for the life of every interface below.
    let locator: IWbemLocator =
        unsafe { CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| describe(&e))?;
    let empty = BSTR::new();
    // SAFETY: a valid locator; empty strings mean the current user, locale and authority.
    let services = unsafe {
        locator.ConnectServer(
            &BSTR::from("ROOT\\CIMV2"),
            &empty,
            &empty,
            &empty,
            0,
            &empty,
            None,
        )
    }
    .map_err(|e| describe(&e))?;
    // The provider creates the process as this user, which it may do only when allowed to
    // impersonate.
    // SAFETY: a valid proxy and constant settings.
    unsafe {
        CoSetProxyBlanket(
            &services,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            PCWSTR::null(),
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )
    }
    .map_err(|e| describe(&e))?;

    let mut class = None;
    // SAFETY: as for `instance`.
    unsafe {
        services.GetObject(
            &BSTR::from("Win32_Process"),
            WBEM_FLAG_RETURN_WBEM_COMPLETE,
            None,
            Some(&mut class),
            None,
        )
    }
    .map_err(|e| describe(&e))?;
    let class = class.ok_or_else(|| "WMI gave no Win32_Process class.".to_string())?;
    let mut signature = None;
    // SAFETY: a valid class and an out-parameter of ours; the output signature is not wanted.
    unsafe { class.GetMethod(w!("Create"), 0, &mut signature, std::ptr::null_mut()) }
        .map_err(|e| describe(&e))?;
    let signature =
        signature.ok_or_else(|| "WMI gave no Win32_Process.Create signature.".to_string())?;
    // SAFETY: a valid method signature.
    let input = unsafe { signature.SpawnInstance(0) }.map_err(|e| describe(&e))?;

    let startup = instance(&services, "Win32_ProcessStartup")?;
    for property in &request.startup {
        match property {
            Startup::ShowWindow(show) => put(&startup, w!("ShowWindow"), &Variant::i4(*show))?,
            // A CIM uint32 is put as a VT_I4, its bits kept.
            Startup::CreateFlags(flags) => put(
                &startup,
                w!("CreateFlags"),
                &Variant::i4(i32::from_ne_bytes(flags.to_ne_bytes())),
            )?,
            Startup::EnvironmentVariables(entries) => put(
                &startup,
                w!("EnvironmentVariables"),
                &Variant::strings(entries)?,
            )?,
        }
    }
    put(
        &input,
        w!("CommandLine"),
        &Variant::bstr(&request.command_line),
    )?;
    put(
        &input,
        w!("CurrentDirectory"),
        &Variant::bstr(&request.directory),
    )?;
    put(
        &input,
        w!("ProcessStartupInformation"),
        &Variant::object(&startup)?,
    )?;

    let mut output = None;
    // SAFETY: a valid proxy, the method's own input instance and an out-parameter of ours.
    unsafe {
        services.ExecMethod(
            &BSTR::from("Win32_Process"),
            &BSTR::from("Create"),
            WBEM_FLAG_RETURN_WBEM_COMPLETE,
            None,
            &input,
            Some(&mut output),
            None,
        )
    }
    .map_err(|e| describe(&e))?;
    let output = output.ok_or_else(|| "Win32_Process.Create gave no answer.".to_string())?;
    let mut value = Variant(VARIANT::default());
    // SAFETY: a valid output instance and a VARIANT of ours, cleared on drop.
    unsafe { output.Get(w!("ReturnValue"), 0, &mut value.0, None, None) }
        .map_err(|e| describe(&e))?;
    value
        .code()
        .ok_or_else(|| "Win32_Process.Create gave no return code.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(request: &Request) -> Vec<&'static str> {
        request.startup.iter().map(Startup::name).collect()
    }

    /// The launcher's call, exactly as agentspace-launch makes it (src/win/wmi.rs): no
    /// environment and no flags. Its request MUST set `ShowWindow` 0 alone, as before
    /// `CreateFlags` was added for `WmiDetach`.
    #[test]
    fn the_launchers_request_sets_show_window_alone_and_no_create_flags() {
        let r = request(OsStr::new("\"l.exe\" a"), OsStr::new(r"C:\w"), &[], None);
        assert_eq!(names(&r), ["ShowWindow"]);
        assert!(matches!(r.startup[0], Startup::ShowWindow(0)));
        assert_eq!(r.command_line, "\"l.exe\" a");
        assert_eq!(r.directory, r"C:\w");
    }

    #[test]
    fn flags_and_entries_are_set_only_when_given() {
        let r = request(
            OsStr::new("c"),
            OsStr::new("d"),
            &[],
            Some(CREATE_NEW_PROCESS_GROUP),
        );
        assert_eq!(names(&r), ["ShowWindow", "CreateFlags"]);
        assert!(matches!(r.startup[1], Startup::CreateFlags(512)));

        let entries = [OsString::from("A=1"), OsString::from("B=2")];
        let r = request(OsStr::new("c"), OsStr::new("d"), &entries, Some(512));
        assert_eq!(
            names(&r),
            ["ShowWindow", "CreateFlags", "EnvironmentVariables"]
        );
        let Startup::EnvironmentVariables(got) = &r.startup[2] else {
            panic!("The third property is the environment.");
        };
        assert_eq!(got.as_slice(), entries.as_slice());
    }
}
