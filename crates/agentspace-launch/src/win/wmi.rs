//! The launcher is created through Windows Management Instrumentation's
//! `Win32_Process.Create` (docs/keeper.md, The handshake, step 2), which makes it outside the
//! pass's job and every job above it, with the user's logon environment. `Win32_ProcessStartup`
//! gives it no window (`ShowWindow` 0, `SW_HIDE`) and sets nothing else, so its environment is
//! WMI's: the logon one.
//!
//! One call runs on a thread of its own, with COM initialized there, so the pass can abandon it at
//! its 10-second bound (`handshake::bounded`).
//!
//! **An abandoned call's thread is left to run, and is never joined.** It owns all it uses: its
//! copies of the command line and the folder, its own COM initialization and every interface it
//! holds. When the call returns at last, its answer goes to a channel nobody reads, the interfaces
//! are released, and the thread uninitializes COM itself and ends. A call that never returns keeps
//! its thread until the process ends, which ends the thread with it: one thread for each abandoned
//! try, at most 3 for a start. The launcher such a call makes late is the claim's to stop, never
//! this module's (keeper.md, The handshake, steps 2 and 3).

use crate::handshake::Create;
use std::mem::ManuallyDrop;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    CoSetProxyBlanket, CoUninitialize, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Variant::{VARIANT, VT_BSTR, VT_I4, VT_UI4, VT_UNKNOWN, VariantClear};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, IWbemServices, WBEM_FLAG_RETURN_WBEM_COMPLETE, WbemLocator,
};
use windows::core::{BSTR, IUnknown, Interface, PCWSTR, w};

/// `SW_HIDE`.
const SW_HIDE: i32 = 0;

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
    fn bstr(text: &str) -> Variant {
        let mut v = VARIANT::default();
        // SAFETY: a zeroed VARIANT given a type and the value of that type.
        unsafe {
            (*v.Anonymous.Anonymous).vt = VT_BSTR;
            (*v.Anonymous.Anonymous).Anonymous.bstrVal = ManuallyDrop::new(BSTR::from(text));
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

/// `Win32_Process.Create` with this command line and working folder, and no window: the call's
/// return code, or the error of a call that failed.
fn call(command_line: &str, directory: &str) -> Result<u32, String> {
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
    put(&startup, w!("ShowWindow"), &Variant::i4(SW_HIDE))?;
    put(&input, w!("CommandLine"), &Variant::bstr(command_line))?;
    put(&input, w!("CurrentDirectory"), &Variant::bstr(directory))?;
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

/// One create call of the launcher, unbounded: the bound is the caller's.
pub(crate) fn create(command_line: &str, directory: &str) -> Create {
    match call(command_line, directory) {
        Ok(code) => Create::Returned(code),
        Err(error) => Create::Threw(error),
    }
}
