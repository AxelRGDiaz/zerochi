use super::{AddAccountError, AddAccountErrorTag};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[derive(Clone)]
pub(crate) struct AddAccountAttemptGuardSet {
    active: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}

impl AddAccountAttemptGuardSet {
    pub(crate) fn new() -> Self {
        Self {
            active: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn begin(&self) -> Result<AddAccountAttemptGuard, AddAccountError> {
        let mut active = self.active.lock().map_err(|_| internal())?;
        if active.is_some() {
            return Err(AddAccountError::from_tag(
                AddAccountErrorTag::AuthInProgress,
            ));
        }

        let cancelled = Arc::new(AtomicBool::new(false));
        *active = Some(Arc::clone(&cancelled));
        Ok(AddAccountAttemptGuard {
            active: Arc::clone(&self.active),
            cancellation: Cancellation(cancelled),
        })
    }

    /// Marks the active attempt cancelled. Repeated calls remain successful.
    pub(crate) fn cancel_active(&self) -> Result<bool, AddAccountError> {
        let active = self.active.lock().map_err(|_| internal())?;
        if let Some(cancelled) = active.as_ref() {
            cancelled.store(true, Ordering::Release);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[derive(Clone)]
pub(crate) struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub(crate) fn ensure_active(&self) -> Result<(), AddAccountError> {
        if self.is_cancelled() {
            Err(AddAccountError::from_tag(AddAccountErrorTag::Cancelled))
        } else {
            Ok(())
        }
    }
}

pub(crate) struct AddAccountAttemptGuard {
    active: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    cancellation: Cancellation,
}

impl AddAccountAttemptGuard {
    pub(crate) fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }
}

impl Drop for AddAccountAttemptGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            if active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &self.cancellation.0))
            {
                *active = None;
            }
        }
    }
}

fn internal() -> AddAccountError {
    AddAccountError::from_tag(AddAccountErrorTag::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_attempt_returns_auth_in_progress_without_sharing_or_cancelling() {
        let attempts = AddAccountAttemptGuardSet::new();
        let first = attempts.begin().expect("first attempt");
        let first_cancellation = first.cancellation();

        let error = match attempts.begin() {
            Ok(_) => panic!("second attempt unexpectedly started"),
            Err(error) => error,
        };

        assert_eq!(error.tag(), AddAccountErrorTag::AuthInProgress);
        assert!(!first_cancellation.is_cancelled());
        first_cancellation.ensure_active().expect("still active");
    }

    #[test]
    fn dropping_the_guard_allows_the_next_attempt() {
        let attempts = AddAccountAttemptGuardSet::new();
        drop(attempts.begin().expect("first attempt"));

        attempts.begin().expect("next attempt after release");
    }

    #[test]
    fn cancellation_is_idempotent_and_observable_by_the_active_attempt_only() {
        let attempts = AddAccountAttemptGuardSet::new();
        let first = attempts.begin().expect("first attempt");
        let cancellation = first.cancellation();

        assert_eq!(attempts.cancel_active(), Ok(true));
        assert_eq!(attempts.cancel_active(), Ok(true));
        assert!(cancellation.is_cancelled());
        assert_eq!(
            cancellation.ensure_active().unwrap_err().tag(),
            AddAccountErrorTag::Cancelled
        );

        drop(first);
        assert_eq!(attempts.cancel_active(), Ok(false));
        let second = attempts.begin().expect("second attempt");
        assert!(!second.cancellation().is_cancelled());
    }

    #[test]
    fn cloned_guard_sets_serialize_concurrent_callers() {
        let attempts = AddAccountAttemptGuardSet::new();
        let active = attempts.begin().expect("active attempt");
        let callers: Vec<_> = (0..16)
            .map(|_| {
                let attempts = attempts.clone();
                std::thread::spawn(move || match attempts.begin() {
                    Ok(_) => panic!("concurrent attempt unexpectedly started"),
                    Err(error) => error.tag(),
                })
            })
            .collect();

        for caller in callers {
            assert_eq!(
                caller.join().expect("caller thread"),
                AddAccountErrorTag::AuthInProgress
            );
        }
        assert!(!active.cancellation().is_cancelled());
    }
}
