//! macOS system sleep/wake projection into the central poll scheduler.

use std::sync::{Arc, Mutex};

/// The only lifecycle requests the native adapter may make of the scheduler.
///
/// `WakeAndCatchUp` deliberately combines resuming with one immediate request so
/// callers cannot accidentally resume scheduled polling without performing the
/// contractually required guarded catch-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchedulerLifecycleRequest {
    Pause,
    WakeAndCatchUp,
}

/// Injected application-owned callback. Implementations should enqueue the
/// request into the existing scheduler and return promptly; they must not poll
/// Gmail directly.
pub(crate) type SleepWakeCallback = Arc<dyn Fn(SchedulerLifecycleRequest) + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SystemPowerState {
    Awake,
    Sleeping,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SystemPowerEvent {
    WillSleep,
    DidWake,
}

struct SleepWakeGate {
    state: Mutex<SystemPowerState>,
    callback: SleepWakeCallback,
}

impl SleepWakeGate {
    fn new(callback: SleepWakeCallback) -> Self {
        Self {
            state: Mutex::new(SystemPowerState::Awake),
            callback,
        }
    }

    fn observe(&self, event: SystemPowerEvent) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let request = match (*state, event) {
            (SystemPowerState::Awake, SystemPowerEvent::WillSleep) => {
                *state = SystemPowerState::Sleeping;
                Some(SchedulerLifecycleRequest::Pause)
            }
            (SystemPowerState::Sleeping, SystemPowerEvent::DidWake) => {
                *state = SystemPowerState::Awake;
                Some(SchedulerLifecycleRequest::WakeAndCatchUp)
            }
            _ => None,
        };

        // Keep transition delivery ordered with state mutation. The production
        // callback is a non-blocking scheduler enqueue and must not re-enter.
        if let Some(request) = request {
            (self.callback)(request);
        }
    }
}

/// Fixed, non-sensitive installation failures for the native observer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SleepWakeObserverError {
    NativeWorkspaceUnavailable,
    AlreadyInstalled,
}

impl std::fmt::Display for SleepWakeObserverError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::NativeWorkspaceUnavailable => "macOS workspace lifecycle is unavailable",
            Self::AlreadyInstalled => "macOS workspace lifecycle is already installed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for SleepWakeObserverError {}

#[cfg(target_os = "macos")]
mod macos {
    use std::{
        cell::RefCell,
        panic::{catch_unwind, AssertUnwindSafe},
        ptr::NonNull,
        sync::Arc,
    };

    use block2::RcBlock;
    use objc2::{
        msg_send,
        rc::Retained,
        runtime::{AnyClass, AnyObject, NSObjectProtocol, ProtocolObject},
    };
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSString};

    use super::{SleepWakeCallback, SleepWakeGate, SleepWakeObserverError, SystemPowerEvent};

    const WILL_SLEEP_NOTIFICATION: &str = "NSWorkspaceWillSleepNotification";
    const DID_WAKE_NOTIFICATION: &str = "NSWorkspaceDidWakeNotification";

    struct ObserverRegistration {
        _center: Retained<NSNotificationCenter>,
        _will_sleep: Retained<ProtocolObject<dyn NSObjectProtocol>>,
        _did_wake: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    }

    thread_local! {
        // Installation happens once from Tauri setup on the main thread. Keeping
        // the opaque observer tokens here makes their process lifetime explicit
        // without claiming that AppKit objects are Send/Sync Tauri state.
        static REGISTRATION: RefCell<Option<ObserverRegistration>> = const { RefCell::new(None) };
    }

    pub(super) fn install(callback: SleepWakeCallback) -> Result<(), SleepWakeObserverError> {
        REGISTRATION.with(|slot| {
            if slot.borrow().is_some() {
                return Err(SleepWakeObserverError::AlreadyInstalled);
            }

            let workspace_class = AnyClass::get(c"NSWorkspace")
                .ok_or(SleepWakeObserverError::NativeWorkspaceUnavailable)?;

            // SAFETY: NSWorkspace is supplied by AppKit, `sharedWorkspace` returns
            // an NSWorkspace object, and its `notificationCenter` selector returns
            // an NSNotificationCenter. Tauri already links AppKit on macOS.
            let workspace: Retained<AnyObject> =
                unsafe { msg_send![workspace_class, sharedWorkspace] };
            let center: Retained<NSNotificationCenter> =
                unsafe { msg_send![&workspace, notificationCenter] };

            let gate = Arc::new(SleepWakeGate::new(callback));
            let will_sleep = register(
                &center,
                WILL_SLEEP_NOTIFICATION,
                Arc::clone(&gate),
                SystemPowerEvent::WillSleep,
            );
            let did_wake = register(
                &center,
                DID_WAKE_NOTIFICATION,
                gate,
                SystemPowerEvent::DidWake,
            );

            slot.replace(Some(ObserverRegistration {
                _center: center,
                _will_sleep: will_sleep,
                _did_wake: did_wake,
            }));
            Ok(())
        })
    }

    fn register(
        center: &NSNotificationCenter,
        name: &str,
        gate: Arc<SleepWakeGate>,
        event: SystemPowerEvent,
    ) -> Retained<ProtocolObject<dyn NSObjectProtocol>> {
        let name = NSString::from_str(name);
        let handler = RcBlock::new(move |_notification: NonNull<NSNotification>| {
            // A scheduler callback must not unwind through an Objective-C block.
            let _ = catch_unwind(AssertUnwindSafe(|| gate.observe(event)));
        });

        // SAFETY: The notification name is an immutable NSString, no object or
        // queue filter is required, and the sendable block accepts the generated
        // NSNotification pointer signature. NSNotificationCenter copies it.
        unsafe {
            center.addObserverForName_object_queue_usingBlock(Some(&name), None, None, &handler)
        }
    }
}

/// Installs one process-lifetime NSWorkspace observer for sleep/wake events.
///
/// The callback receives only scheduler-control intent; no account, provider,
/// OAuth, cursor, or message data enters this native boundary.
#[cfg(target_os = "macos")]
pub(crate) fn install_macos_sleep_wake_observer(
    callback: SleepWakeCallback,
) -> Result<(), SleepWakeObserverError> {
    macos::install(callback)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn recorder() -> (
        SleepWakeCallback,
        Arc<Mutex<Vec<SchedulerLifecycleRequest>>>,
    ) {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let callback: SleepWakeCallback = Arc::new(move |request| {
            captured.lock().unwrap().push(request);
        });
        (callback, requests)
    }

    #[test]
    fn wake_without_prior_sleep_is_ignored() {
        let (callback, requests) = recorder();
        let gate = SleepWakeGate::new(callback);

        gate.observe(SystemPowerEvent::DidWake);

        assert!(requests.lock().unwrap().is_empty());
    }

    #[test]
    fn duplicate_native_events_emit_one_pause_and_one_catch_up() {
        let (callback, requests) = recorder();
        let gate = SleepWakeGate::new(callback);

        gate.observe(SystemPowerEvent::WillSleep);
        gate.observe(SystemPowerEvent::WillSleep);
        gate.observe(SystemPowerEvent::DidWake);
        gate.observe(SystemPowerEvent::DidWake);

        assert_eq!(
            *requests.lock().unwrap(),
            [
                SchedulerLifecycleRequest::Pause,
                SchedulerLifecycleRequest::WakeAndCatchUp,
            ]
        );
    }

    #[test]
    fn each_complete_sleep_cycle_gets_exactly_one_catch_up() {
        let (callback, requests) = recorder();
        let gate = SleepWakeGate::new(callback);

        for _ in 0..3 {
            gate.observe(SystemPowerEvent::WillSleep);
            gate.observe(SystemPowerEvent::DidWake);
        }

        assert_eq!(
            *requests.lock().unwrap(),
            [
                SchedulerLifecycleRequest::Pause,
                SchedulerLifecycleRequest::WakeAndCatchUp,
                SchedulerLifecycleRequest::Pause,
                SchedulerLifecycleRequest::WakeAndCatchUp,
                SchedulerLifecycleRequest::Pause,
                SchedulerLifecycleRequest::WakeAndCatchUp,
            ]
        );
    }
}
