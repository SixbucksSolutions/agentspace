//! The real [`TaskStore`]: Task Scheduler's COM API, through `windows = "=0.62.2"` (ITaskService,
//! registering in the root folder with TASK_CREATE_OR_UPDATE). This module and [`crate::sys`] are
//! the only ones that use `unsafe`.
//!
//! Nothing here that makes a COM call runs in a test: the only tests are of the plain mapping from
//! a definition's names to Task Scheduler's enums. A real install and `-Uninstall` reach the rest
//! only from `main`, and the first real registration is v0.06's cutover, with Terry's yes (D41).
//!
//! Every method connects for itself, so no COM call is made before the first one a mode needs:
//! a real install's steps 1 and 2 finish before this module is touched.

use crate::definition::{TASK_NAME, TaskDefinition, TriggerInfo, TriggerKind};
use crate::flow::{TaskState, TaskStore};
use windows::Win32::Foundation::{VARIANT_BOOL, VARIANT_FALSE, VARIANT_TRUE};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::TaskScheduler::{
    IExecAction, ILogonTrigger, IRegisteredTask, ITaskFolder, ITaskService, TASK_ACTION_EXEC,
    TASK_CREATE_OR_UPDATE, TASK_INSTANCES_IGNORE_NEW, TASK_INSTANCES_POLICY,
    TASK_LOGON_INTERACTIVE_TOKEN, TASK_LOGON_TYPE, TASK_RUNLEVEL_LUA, TASK_RUNLEVEL_TYPE,
    TASK_STATE_DISABLED, TASK_STATE_QUEUED, TASK_STATE_READY, TASK_STATE_RUNNING,
    TASK_TRIGGER_LOGON, TASK_TRIGGER_TIME, TaskScheduler,
};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{BSTR, Interface};

/// The real Task Scheduler.
pub struct Scheduler;

/// `HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)` and `HRESULT_FROM_WIN32(ERROR_PATH_NOT_FOUND)`: what
/// Task Scheduler answers for a task that is not registered.
const NOT_FOUND: [u32; 2] = [0x8007_0002, 0x8007_0003];

/// `RPC_E_CHANGED_MODE`: the thread already has COM, in another mode.
const RPC_E_CHANGED_MODE: u32 = 0x8001_0106;

/// COM for this call: initialized on creation, and released on drop. It MUST be declared before
/// any COM object a method holds, so it drops after them.
struct Com {
    uninitialize: bool,
}

impl Com {
    fn new() -> Result<Com, String> {
        // SAFETY: Plain COM initialization for the calling thread, paired with CoUninitialize on
        // drop when it succeeded.
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_ok() {
            Ok(Com { uninitialize: true })
        } else if result.0 as u32 == RPC_E_CHANGED_MODE {
            Ok(Com {
                uninitialize: false,
            })
        } else {
            Err(format!(
                "COM could not be started (0x{:08X}).",
                result.0 as u32
            ))
        }
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.uninitialize {
            // SAFETY: Paired with the successful CoInitializeEx in `new`.
            unsafe { CoUninitialize() };
        }
    }
}

fn describe(what: &str, error: &windows::core::Error) -> String {
    format!(
        "{what}: {} (0x{:08X})",
        error.message().trim(),
        error.code().0 as u32
    )
}

fn is_not_found(error: &windows::core::Error) -> bool {
    NOT_FOUND.contains(&(error.code().0 as u32))
}

fn bstr(text: &str) -> BSTR {
    BSTR::from(text)
}

fn bool_of(value: bool) -> VARIANT_BOOL {
    if value { VARIANT_TRUE } else { VARIANT_FALSE }
}

/// Task Scheduler, connected to this computer as the current user, and its root folder.
fn connect() -> Result<(ITaskService, ITaskFolder), String> {
    // SAFETY: COM calls on objects this function creates and owns; every pointer passed is to a
    // local that outlives the call. The caller holds a `Com` for the thread.
    unsafe {
        let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| describe("Task Scheduler could not be reached", &e))?;
        // Four empty VARIANTs: this computer, the current user's own token.
        let empty = VARIANT::default();
        service
            .Connect(&empty, &empty, &empty, &empty)
            .map_err(|e| describe("Task Scheduler could not be connected to", &e))?;
        let root = service
            .GetFolder(&bstr("\\"))
            .map_err(|e| describe("Task Scheduler's root folder could not be opened", &e))?;
        Ok((service, root))
    }
}

/// The definition's logon type, as Task Scheduler names it.
fn logon_type(def: &TaskDefinition) -> Result<TASK_LOGON_TYPE, String> {
    match def.logon_type.as_str() {
        "Interactive" => Ok(TASK_LOGON_INTERACTIVE_TOKEN),
        other => Err(format!(
            "The logon type {other} is not one the installer sets."
        )),
    }
}

/// The definition's run level, as Task Scheduler names it.
fn run_level(def: &TaskDefinition) -> Result<TASK_RUNLEVEL_TYPE, String> {
    match def.run_level.as_str() {
        "Limited" => Ok(TASK_RUNLEVEL_LUA),
        other => Err(format!(
            "The run level {other} is not one the installer sets."
        )),
    }
}

/// The definition's multiple-instances policy, as Task Scheduler names it.
fn multiple_instances(def: &TaskDefinition) -> Result<TASK_INSTANCES_POLICY, String> {
    match def.multiple_instances.as_str() {
        "IgnoreNew" => Ok(TASK_INSTANCES_IGNORE_NEW),
        other => Err(format!(
            "The multiple-instances policy {other} is not one the installer sets."
        )),
    }
}

/// The registered task, `None` when there is none.
fn find_task(root: &ITaskFolder) -> Result<Option<IRegisteredTask>, String> {
    // SAFETY: A COM call on a live folder object, with a BSTR that outlives the call.
    match unsafe { root.GetTask(&bstr(TASK_NAME)) } {
        Ok(task) => Ok(Some(task)),
        Err(error) if is_not_found(&error) => Ok(None),
        Err(error) => Err(describe("The task could not be looked up", &error)),
    }
}

fn state_of(task: &IRegisteredTask) -> Result<TaskState, String> {
    // SAFETY: A COM call on a live task object.
    let state =
        unsafe { task.State() }.map_err(|e| describe("The task's state could not be read", &e))?;
    Ok(if state == TASK_STATE_DISABLED {
        TaskState::Disabled
    } else if state == TASK_STATE_QUEUED {
        TaskState::Queued
    } else if state == TASK_STATE_READY {
        TaskState::Ready
    } else if state == TASK_STATE_RUNNING {
        TaskState::Running
    } else {
        TaskState::Unknown
    })
}

/// The enums a definition names, in Task Scheduler's terms.
struct Kinds {
    logon_type: TASK_LOGON_TYPE,
    run_level: TASK_RUNLEVEL_TYPE,
    multiple_instances: TASK_INSTANCES_POLICY,
}

fn kinds(def: &TaskDefinition) -> Result<Kinds, String> {
    Ok(Kinds {
        logon_type: logon_type(def)?,
        run_level: run_level(def)?,
        multiple_instances: multiple_instances(def)?,
    })
}

/// Builds the task's definition in Task Scheduler's own objects and registers it in the root
/// folder, replacing any task of that name, then starts it.
fn register(
    service: &ITaskService,
    root: &ITaskFolder,
    def: &TaskDefinition,
    kinds: &Kinds,
) -> Result<(), windows::core::Error> {
    // SAFETY: COM calls on objects created from the live, connected service; every pointer
    // passed is to a local that outlives its call.
    unsafe {
        let task = service.NewTask(0)?;

        task.RegistrationInfo()?
            .SetDescription(&bstr(&def.description))?;

        let principal = task.Principal()?;
        principal.SetUserId(&bstr(&def.principal_user))?;
        principal.SetLogonType(kinds.logon_type)?;
        principal.SetRunLevel(kinds.run_level)?;

        let settings = task.Settings()?;
        settings.SetExecutionTimeLimit(&bstr(&def.execution_time_limit))?;
        settings.SetMultipleInstances(kinds.multiple_instances)?;
        settings.SetStartWhenAvailable(bool_of(def.start_when_available))?;
        settings.SetDisallowStartIfOnBatteries(bool_of(def.disallow_start_if_on_batteries))?;
        settings.SetStopIfGoingOnBatteries(bool_of(def.stop_if_going_on_batteries))?;
        settings.SetRestartCount(def.restart_count as i32)?;
        settings.SetRestartInterval(&bstr(&def.restart_interval))?;
        settings.SetEnabled(bool_of(def.enabled))?;
        settings.SetHidden(bool_of(def.hidden))?;

        let triggers = task.Triggers()?;
        let logon = triggers.Create(TASK_TRIGGER_LOGON)?;
        logon
            .cast::<ILogonTrigger>()?
            .SetUserId(&bstr(&def.logon_trigger_user))?;
        let once = triggers.Create(TASK_TRIGGER_TIME)?;
        // UTC with a `Z` (The task's fields, Trigger 2). The read-back prints the boundary as
        // Task Scheduler returns it (architect, 2026-11-19), which is unobserved until v0.06's
        // cutover: most likely this same string, possibly local time with an offset.
        once.SetStartBoundary(&bstr(&def.start_boundary))?;
        if let Some(end) = &def.end_boundary {
            once.SetEndBoundary(&bstr(end))?;
        }
        let repetition = once.Repetition()?;
        repetition.SetInterval(&bstr(&def.repetition_interval))?;
        if let Some(duration) = &def.repetition_duration {
            repetition.SetDuration(&bstr(duration))?;
        }
        repetition.SetStopAtDurationEnd(bool_of(def.stop_at_duration_end))?;

        let action = task.Actions()?.Create(TASK_ACTION_EXEC)?;
        let exec = action.cast::<IExecAction>()?;
        exec.SetPath(&bstr(&def.execute.display().to_string()))?;
        exec.SetArguments(&bstr(&def.arguments))?;
        exec.SetWorkingDirectory(&bstr(&def.working_directory.display().to_string()))?;

        // The task's name, relative to the root folder; the installing user, no password, the
        // principal's logon type, and no security descriptor.
        let registered = root.RegisterTaskDefinition(
            &bstr(&def.name),
            &task,
            TASK_CREATE_OR_UPDATE.0,
            &VARIANT::from(def.principal_user.as_str()),
            &VARIANT::default(),
            kinds.logon_type,
            &VARIANT::default(),
        )?;
        registered.Run(&VARIANT::default())?;
    }
    Ok(())
}

/// Reads one text property that Task Scheduler gives through an out parameter.
macro_rules! read_text {
    ($object:expr, $getter:ident) => {{
        let mut text = BSTR::default();
        // Used only inside the `unsafe` block of a caller that holds live COM objects: a getter
        // that writes a BSTR into a local.
        $object.$getter(&mut text).map(|()| text.to_string())
    }};
}

/// Reads the triggers of a registered task, in the forms the dry run prints.
fn read_triggers(task: &IRegisteredTask) -> Result<Vec<TriggerInfo>, windows::core::Error> {
    let mut found = Vec::new();
    // SAFETY: COM calls on live objects the task owns. Task Scheduler's collections count from 1.
    unsafe {
        let collection = task.Definition()?.Triggers()?;
        let mut count = 0i32;
        collection.Count(&mut count)?;
        for index in 1..=count {
            let trigger = collection.get_Item(index)?;
            let mut kind = Default::default();
            trigger.Type(&mut kind)?;
            let start_boundary = read_text!(trigger, StartBoundary)?;
            let end_boundary = read_text!(trigger, EndBoundary)?;
            let repetition = trigger.Repetition()?;
            let interval = read_text!(repetition, Interval)?;
            let duration = read_text!(repetition, Duration)?;
            let mut stop = VARIANT_FALSE;
            repetition.StopAtDurationEnd(&mut stop)?;
            let mut info = TriggerInfo {
                kind: TriggerKind::Other(kind_name(kind.0)),
                user: String::new(),
                start_boundary,
                end_boundary: Some(end_boundary),
                repetition_interval: Some(interval),
                repetition_duration: Some(duration),
                stop_at_duration_end: Some(stop == VARIANT_TRUE),
            };
            if kind == TASK_TRIGGER_LOGON {
                info.kind = TriggerKind::AtLogOn;
                let logon = trigger.cast::<ILogonTrigger>()?;
                info.user = read_text!(logon, UserId)?;
                info.stop_at_duration_end = None;
            } else if kind == TASK_TRIGGER_TIME {
                info.kind = TriggerKind::Once;
            } else {
                info.stop_at_duration_end = None;
            }
            found.push(info);
        }
    }
    Ok(found)
}

/// The name a trigger of another kind prints after `Trigger <n>:`, from TASK_TRIGGER_TYPE2.
fn kind_name(kind: i32) -> String {
    match kind {
        0 => "OnEvent",
        2 => "Daily",
        3 => "Weekly",
        4 => "Monthly",
        5 => "MonthlyDayOfWeek",
        6 => "OnIdle",
        7 => "AtRegistration",
        8 => "AtStartup",
        11 => "OnSessionStateChange",
        12 => "Custom",
        _ => "Unknown",
    }
    .to_string()
}

impl TaskStore for Scheduler {
    fn state(&self) -> Result<Option<TaskState>, String> {
        let _com = Com::new()?;
        let (_service, root) = connect()?;
        match find_task(&root)? {
            Some(task) => state_of(&task).map(Some),
            None => Ok(None),
        }
    }

    fn register_and_start(&self, def: &TaskDefinition) -> Result<(), String> {
        // Checked before COM is started, so a definition this module cannot register reaches
        // nothing.
        let kinds = kinds(def)?;
        let _com = Com::new()?;
        let (service, root) = connect()?;
        register(&service, &root, def, &kinds)
            .map_err(|e| describe("The task could not be registered and started", &e))
    }

    fn read_back(&self) -> Result<Vec<TriggerInfo>, String> {
        let _com = Com::new()?;
        let (_service, root) = connect()?;
        let task = find_task(&root)?.ok_or_else(|| {
            "The task is not registered, so its triggers cannot be read.".to_string()
        })?;
        read_triggers(&task).map_err(|e| describe("The task's triggers could not be read", &e))
    }

    fn unregister(&self) -> Result<bool, String> {
        let _com = Com::new()?;
        let (_service, root) = connect()?;
        let Some(task) = find_task(&root)? else {
            return Ok(false);
        };
        // A failure to end the task is ignored: it may not be running.
        // SAFETY: A COM call on a live task object.
        let _ = unsafe { task.Stop(0) };
        // SAFETY: A COM call on a live folder object, with a BSTR that outlives the call.
        unsafe { root.DeleteTask(&bstr(TASK_NAME), 0) }
            .map_err(|e| describe("The task could not be unregistered", &e))?;
        Ok(true)
    }
}

/// Only the plain mapping is tested here. No test creates a `Com`, calls [`connect`], or uses
/// [`Scheduler`].
#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::task_definition;
    use std::path::Path;

    fn sample() -> TaskDefinition {
        task_definition(
            Path::new("C:\\example\\bin"),
            Path::new("C:\\example"),
            "EXAMPLEDOM\\someone",
            "2026-11-19T10:20:30Z",
        )
    }

    #[test]
    fn the_definitions_names_map_to_the_registered_kinds() {
        let kinds = kinds(&sample()).expect("Kinds");
        assert_eq!(kinds.logon_type, TASK_LOGON_INTERACTIVE_TOKEN);
        assert_eq!(kinds.run_level, TASK_RUNLEVEL_LUA);
        assert_eq!(kinds.multiple_instances, TASK_INSTANCES_IGNORE_NEW);
    }

    #[test]
    fn a_name_the_installer_does_not_set_is_refused() {
        let mut def = sample();
        def.run_level = "Highest".to_string();
        assert!(kinds(&def).is_err());
        let mut def = sample();
        def.logon_type = "Password".to_string();
        assert!(kinds(&def).is_err());
        let mut def = sample();
        def.multiple_instances = "Parallel".to_string();
        assert!(kinds(&def).is_err());
    }
}
