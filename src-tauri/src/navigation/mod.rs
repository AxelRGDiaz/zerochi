//! Native authorization-browser handoff.

// Lead will construct this adapter while wiring the live add-account command.
#![allow(dead_code)]

use std::{
    fmt,
    process::{Command, Stdio},
    sync::Arc,
};

use crate::auth::{
    AddAccountError, AddAccountErrorTag, AuthorizationRequest, BrowserPort, Cancellation,
    PortFuture,
};

trait AuthorizationUrlOpener: Send + Sync {
    fn open(&self, url: &str) -> Result<(), ()>;
}

struct DefaultMacOsBrowser;

impl AuthorizationUrlOpener for DefaultMacOsBrowser {
    fn open(&self, url: &str) -> Result<(), ()> {
        let status = Command::new("/usr/bin/open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| ())?;

        if status.success() {
            Ok(())
        } else {
            Err(())
        }
    }
}

#[derive(Clone)]
pub(crate) struct MacOsBrowserPort {
    opener: Arc<dyn AuthorizationUrlOpener>,
}

impl MacOsBrowserPort {
    pub(crate) fn new() -> Self {
        Self {
            opener: Arc::new(DefaultMacOsBrowser),
        }
    }

    #[cfg(test)]
    fn with_opener(opener: Arc<dyn AuthorizationUrlOpener>) -> Self {
        Self { opener }
    }
}

impl fmt::Debug for MacOsBrowserPort {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MacOsBrowserPort([REDACTED])")
    }
}

impl BrowserPort for MacOsBrowserPort {
    fn open<'a>(
        &'a self,
        request: &'a AuthorizationRequest,
        cancellation: &'a Cancellation,
    ) -> PortFuture<'a, Result<(), AddAccountError>> {
        Box::pin(async move {
            cancellation.ensure_active()?;
            self.opener
                .open(request.url())
                .map_err(|_| AddAccountError::from_tag(AddAccountErrorTag::BrowserOpenFailed))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{
        generate_pkce, AddAccountAttemptGuardSet, ClientId, Environment, LoopbackRedirect,
        RandomSource,
    };
    use std::{
        ffi::OsString,
        future::Future,
        pin::Pin,
        sync::Mutex,
        task::{Context, Poll, Waker},
    };

    const CLIENT_ID: &str =
        "123456789012-abcdefghijklmnopqrstuvwxyz012345.apps.googleusercontent.com";

    struct FixedEnvironment;

    impl Environment for FixedEnvironment {
        fn var_os(&self, name: &str) -> Option<OsString> {
            (name == "ZEROCHI_GOOGLE_CLIENT_ID").then(|| OsString::from(CLIENT_ID))
        }
    }

    struct FixedRandom;

    impl RandomSource for FixedRandom {
        fn fill(&self, destination: &mut [u8]) -> Result<(), AddAccountError> {
            destination.fill(0x5A);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeOpener {
        handed_off: Mutex<Vec<String>>,
        fail: bool,
    }

    impl FakeOpener {
        fn failing() -> Self {
            Self {
                handed_off: Mutex::new(Vec::new()),
                fail: true,
            }
        }
    }

    impl AuthorizationUrlOpener for FakeOpener {
        fn open(&self, url: &str) -> Result<(), ()> {
            self.handed_off.lock().map_err(|_| ())?.push(url.to_owned());
            if self.fail {
                Err(())
            } else {
                Ok(())
            }
        }
    }

    fn authorization_request() -> AuthorizationRequest {
        let client_id = ClientId::load_from(&FixedEnvironment).expect("client id");
        let redirect =
            LoopbackRedirect::parse("http://127.0.0.1:49152/oauth/callback").expect("redirect");
        let pkce = generate_pkce(&FixedRandom).expect("pkce");
        AuthorizationRequest::new(&client_id, redirect, pkce).expect("request")
    }

    fn block_on_ready<T>(future: impl Future<Output = T>) -> T {
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future = Box::pin(future);
        match Pin::new(&mut future).poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("adapter future unexpectedly pending"),
        }
    }

    #[test]
    fn hands_the_exact_authorization_url_to_the_injected_opener_once() {
        let opener = Arc::new(FakeOpener::default());
        let browser = MacOsBrowserPort::with_opener(opener.clone());
        let request = authorization_request();
        let expected = request.url().to_owned();
        let attempts = AddAccountAttemptGuardSet::new();
        let guard = attempts.begin().expect("attempt");
        let cancellation = guard.cancellation();

        block_on_ready(browser.open(&request, &cancellation)).expect("browser handoff");

        assert_eq!(
            opener.handed_off.lock().expect("handoffs").as_slice(),
            [expected]
        );
    }

    #[test]
    fn opener_failure_maps_to_fixed_error_without_url_diagnostics() {
        let opener = Arc::new(FakeOpener::failing());
        let browser = MacOsBrowserPort::with_opener(opener);
        let request = authorization_request();
        let sensitive_url = request.url().to_owned();
        let attempts = AddAccountAttemptGuardSet::new();
        let guard = attempts.begin().expect("attempt");
        let cancellation = guard.cancellation();

        let error = block_on_ready(browser.open(&request, &cancellation)).unwrap_err();
        let rendered = format!("{error}\n{error:?}\n{browser:?}");

        assert_eq!(error.tag(), AddAccountErrorTag::BrowserOpenFailed);
        assert!(!rendered.contains(&sensitive_url));
        assert!(!rendered.contains(CLIENT_ID));
    }

    #[test]
    fn cancellation_prevents_url_handoff() {
        let opener = Arc::new(FakeOpener::default());
        let browser = MacOsBrowserPort::with_opener(opener.clone());
        let request = authorization_request();
        let attempts = AddAccountAttemptGuardSet::new();
        let guard = attempts.begin().expect("attempt");
        attempts.cancel_active().expect("cancel");
        let cancellation = guard.cancellation();

        let error = block_on_ready(browser.open(&request, &cancellation)).unwrap_err();

        assert_eq!(error.tag(), AddAccountErrorTag::Cancelled);
        assert!(opener.handed_off.lock().expect("handoffs").is_empty());
    }
}
