//! Native authorization-browser handoff.

// Lead will construct this adapter while wiring the live add-account command.
#![allow(dead_code)]

use std::{
    fmt,
    process::{Command, Stdio},
    sync::Arc,
};

use url::{Host, Url};

use crate::auth::{
    AddAccountError, AddAccountErrorTag, AuthorizationRequest, BrowserPort, Cancellation,
    PortFuture,
};

const GMAIL_ACCOUNT_ENDPOINT: &str = "https://mail.google.com/mail/u/";
const GMAIL_THREAD_ID_MAX_BYTES: usize = 256;
const OUTLOOK_MAIL_ENDPOINT: &str = "https://outlook.live.com/mail/";

trait NativeUrlOpener: Send + Sync {
    fn open(&self, url: &str) -> Result<(), ()>;
}

struct DefaultMacOsBrowser;

impl NativeUrlOpener for DefaultMacOsBrowser {
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
    opener: Arc<dyn NativeUrlOpener>,
}

impl MacOsBrowserPort {
    pub(crate) fn new() -> Self {
        Self {
            opener: Arc::new(DefaultMacOsBrowser),
        }
    }

    #[cfg(test)]
    fn with_opener(opener: Arc<dyn NativeUrlOpener>) -> Self {
        Self { opener }
    }
}

/// Opaque native-navigation failure. Its representations intentionally contain no diagnostics.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct ExternalNavigationError;

impl fmt::Debug for ExternalNavigationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ExternalNavigationError([REDACTED])")
    }
}

impl fmt::Display for ExternalNavigationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("external navigation failed")
    }
}

impl std::error::Error for ExternalNavigationError {}

pub(crate) trait ExternalNavigationService: Send + Sync {
    fn open_account(&self, email: &str) -> Result<(), ExternalNavigationError>;

    /// Opens the Outlook.com inbox for a personal Microsoft account. Outlook on the web has no
    /// stable per-message link derivable from stored metadata, so messages also land here.
    fn open_outlook_account(&self, _email: &str) -> Result<(), ExternalNavigationError> {
        Err(ExternalNavigationError)
    }

    /// Opens one Gmail conversation when supported, otherwise safely falls back to the account.
    fn open_message(&self, email: &str, thread_id: &str) -> Result<(), ExternalNavigationError> {
        validate_thread_id(thread_id)?;
        self.open_account(email)
    }
}

#[derive(Clone)]
pub(crate) struct MacOsExternalNavigationService {
    opener: Arc<dyn NativeUrlOpener>,
}

impl MacOsExternalNavigationService {
    pub(crate) fn new() -> Self {
        Self {
            opener: Arc::new(DefaultMacOsBrowser),
        }
    }

    #[cfg(test)]
    fn with_opener(opener: Arc<dyn NativeUrlOpener>) -> Self {
        Self { opener }
    }
}

impl fmt::Debug for MacOsExternalNavigationService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MacOsExternalNavigationService([REDACTED])")
    }
}

impl ExternalNavigationService for MacOsExternalNavigationService {
    fn open_account(&self, email: &str) -> Result<(), ExternalNavigationError> {
        let url = gmail_account_url(email)?;
        self.opener
            .open(url.as_str())
            .map_err(|_| ExternalNavigationError)
    }

    fn open_message(&self, email: &str, thread_id: &str) -> Result<(), ExternalNavigationError> {
        let contextual_url = gmail_message_url(email, thread_id)?;
        if self.opener.open(contextual_url.as_str()).is_ok() {
            return Ok(());
        }

        self.open_account(email)
    }

    fn open_outlook_account(&self, email: &str) -> Result<(), ExternalNavigationError> {
        let url = outlook_account_url(email)?;
        self.opener
            .open(url.as_str())
            .map_err(|_| ExternalNavigationError)
    }
}

fn outlook_account_url(email: &str) -> Result<Url, ExternalNavigationError> {
    if email.is_empty() || email.chars().any(char::is_control) {
        return Err(ExternalNavigationError);
    }

    let mut url = Url::parse(OUTLOOK_MAIL_ENDPOINT).map_err(|_| ExternalNavigationError)?;
    {
        let mut query = url.query_pairs_mut();
        query.clear().append_pair("login_hint", email);
    }

    let allowlisted = url.scheme() == "https"
        && matches!(url.host(), Some(Host::Domain("outlook.live.com")))
        && url.port().is_none()
        && url.path() == "/mail/"
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none();

    if allowlisted {
        Ok(url)
    } else {
        Err(ExternalNavigationError)
    }
}

fn gmail_account_url(email: &str) -> Result<Url, ExternalNavigationError> {
    if email.is_empty() || email.chars().any(char::is_control) {
        return Err(ExternalNavigationError);
    }

    let mut url = Url::parse(GMAIL_ACCOUNT_ENDPOINT).map_err(|_| ExternalNavigationError)?;
    {
        let mut query = url.query_pairs_mut();
        query.clear().append_pair("authuser", email);
    }

    let allowlisted = url.scheme() == "https"
        && matches!(url.host(), Some(Host::Domain("mail.google.com")))
        && url.port().is_none()
        && url.path() == "/mail/u/"
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none();

    if allowlisted {
        Ok(url)
    } else {
        Err(ExternalNavigationError)
    }
}

fn gmail_message_url(email: &str, thread_id: &str) -> Result<Url, ExternalNavigationError> {
    validate_thread_id(thread_id)?;
    let mut url = gmail_account_url(email)?;
    let expected_fragment = format!("all/{thread_id}");
    url.set_fragment(Some(&expected_fragment));

    let allowlisted = url.scheme() == "https"
        && matches!(url.host(), Some(Host::Domain("mail.google.com")))
        && url.port().is_none()
        && url.path() == "/mail/u/"
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment() == Some(expected_fragment.as_str());

    if allowlisted {
        Ok(url)
    } else {
        Err(ExternalNavigationError)
    }
}

fn validate_thread_id(thread_id: &str) -> Result<(), ExternalNavigationError> {
    let valid = !thread_id.is_empty()
        && thread_id.len() <= GMAIL_THREAD_ID_MAX_BYTES
        && thread_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(ExternalNavigationError)
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
        fail_first: bool,
    }

    impl FakeOpener {
        fn failing() -> Self {
            Self {
                handed_off: Mutex::new(Vec::new()),
                fail: true,
                fail_first: false,
            }
        }

        fn failing_first() -> Self {
            Self {
                handed_off: Mutex::new(Vec::new()),
                fail: false,
                fail_first: true,
            }
        }
    }

    impl NativeUrlOpener for FakeOpener {
        fn open(&self, url: &str) -> Result<(), ()> {
            let mut handed_off = self.handed_off.lock().map_err(|_| ())?;
            handed_off.push(url.to_owned());
            if self.fail || (self.fail_first && handed_off.len() == 1) {
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

    #[test]
    fn account_navigation_hands_off_exact_allowlisted_url_once() {
        let opener = Arc::new(FakeOpener::default());
        let navigation = MacOsExternalNavigationService::with_opener(opener.clone());

        navigation
            .open_account("person@example.com")
            .expect("account handoff");

        assert_eq!(
            opener.handed_off.lock().expect("handoffs").as_slice(),
            ["https://mail.google.com/mail/u/?authuser=person%40example.com"]
        );
    }

    #[test]
    fn message_navigation_hands_off_exact_allowlisted_conversation_url_once() {
        let opener = Arc::new(FakeOpener::default());
        let navigation = MacOsExternalNavigationService::with_opener(opener.clone());

        navigation
            .open_message("person+alerts@example.com", "thread_ABC-123")
            .expect("message handoff");

        assert_eq!(
            opener.handed_off.lock().expect("handoffs").as_slice(),
            ["https://mail.google.com/mail/u/?authuser=person%2Balerts%40example.com#all/thread_ABC-123"]
        );
    }

    #[test]
    fn contextual_handoff_failure_falls_back_once_to_the_allowlisted_account_url() {
        let opener = Arc::new(FakeOpener::failing_first());
        let navigation = MacOsExternalNavigationService::with_opener(opener.clone());

        navigation
            .open_message("person@example.com", "thread-123")
            .expect("account fallback");

        assert_eq!(
            opener.handed_off.lock().expect("handoffs").as_slice(),
            [
                "https://mail.google.com/mail/u/?authuser=person%40example.com#all/thread-123",
                "https://mail.google.com/mail/u/?authuser=person%40example.com",
            ]
        );
    }

    #[test]
    fn both_contextual_and_account_handoff_failure_returns_only_fixed_error() {
        let opener = Arc::new(FakeOpener::failing());
        let navigation = MacOsExternalNavigationService::with_opener(opener.clone());

        assert_eq!(
            navigation.open_message("person@example.com", "thread-123"),
            Err(ExternalNavigationError)
        );
        assert_eq!(opener.handed_off.lock().expect("handoffs").len(), 2);
    }

    #[test]
    fn invalid_thread_identifier_fails_before_any_native_handoff() {
        let opener = Arc::new(FakeOpener::default());
        let navigation = MacOsExternalNavigationService::with_opener(opener.clone());
        let over_limit = "a".repeat(GMAIL_THREAD_ID_MAX_BYTES + 1);

        for invalid in [
            "",
            "contains/slash",
            "contains space",
            "contains?query",
            "contains#fragment",
            "contains\nnewline",
            "non-ascii-é",
            &over_limit,
        ] {
            assert_eq!(
                navigation.open_message("person@example.com", invalid),
                Err(ExternalNavigationError)
            );
        }
        assert!(opener.handed_off.lock().expect("handoffs").is_empty());
    }

    #[test]
    fn maximum_length_thread_identifier_is_accepted_without_positional_session_path() {
        let thread_id = "a".repeat(GMAIL_THREAD_ID_MAX_BYTES);
        let url = gmail_message_url("person@example.com", &thread_id).expect("message URL");

        assert_eq!(url.path(), "/mail/u/");
        assert_eq!(url.fragment(), Some(format!("all/{thread_id}").as_str()));
        assert!(!url.path().contains("/u/0"));
    }

    #[test]
    fn account_url_percent_encodes_reserved_and_non_ascii_identity_characters() {
        let email = "josé+team&alerts@example.com";
        let url = gmail_account_url(email).expect("account URL");

        assert_eq!(
            url.as_str(),
            "https://mail.google.com/mail/u/?authuser=jos%C3%A9%2Bteam%26alerts%40example.com"
        );
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            [("authuser".into(), email.into())]
        );
    }

    #[test]
    fn account_url_has_fixed_scheme_host_path_and_no_positional_session_index() {
        let url = gmail_account_url("person42@example.com").expect("account URL");

        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("mail.google.com"));
        assert_eq!(url.port(), None);
        assert_eq!(url.path(), "/mail/u/");
        assert_eq!(url.query_pairs().count(), 1);
        assert!(url
            .path_segments()
            .expect("segments")
            .all(|segment| { segment.is_empty() || segment == "mail" || segment == "u" }));
    }

    #[test]
    fn invalid_identity_fails_before_native_handoff() {
        let opener = Arc::new(FakeOpener::default());
        let navigation = MacOsExternalNavigationService::with_opener(opener.clone());

        let error = navigation.open_account("person@example.com\nhttps://evil.test");

        assert_eq!(error, Err(ExternalNavigationError));
        assert!(opener.handed_off.lock().expect("handoffs").is_empty());
    }

    #[test]
    fn account_navigation_failure_redacts_identity_url_and_adapter_diagnostics() {
        let opener = Arc::new(FakeOpener::failing());
        let navigation = MacOsExternalNavigationService::with_opener(opener);
        let email = "sensitive+account@example.com";
        let url = gmail_account_url(email).expect("account URL").to_string();

        let error = navigation.open_account(email).unwrap_err();
        let rendered = format!("{error}\n{error:?}\n{navigation:?}");

        assert!(!rendered.contains(email));
        assert!(!rendered.contains(&url));
        assert!(!rendered.contains("/usr/bin/open"));
        assert!(!rendered.contains("mail.google.com"));
    }

    #[test]
    fn message_navigation_failure_redacts_identity_thread_and_both_urls() {
        let opener = Arc::new(FakeOpener::failing());
        let navigation = MacOsExternalNavigationService::with_opener(opener);
        let email = "sensitive+account@example.com";
        let thread_id = "sensitive-thread-id";
        let contextual_url = gmail_message_url(email, thread_id)
            .expect("message URL")
            .to_string();
        let account_url = gmail_account_url(email).expect("account URL").to_string();

        let error = navigation.open_message(email, thread_id).unwrap_err();
        let rendered = format!("{error}\n{error:?}\n{navigation:?}");

        assert!(!rendered.contains(email));
        assert!(!rendered.contains(thread_id));
        assert!(!rendered.contains(&contextual_url));
        assert!(!rendered.contains(&account_url));
        assert!(!rendered.contains("/usr/bin/open"));
        assert!(!rendered.contains("mail.google.com"));
    }

    #[test]
    fn outlook_account_opens_the_allowlisted_inbox_with_an_encoded_login_hint() {
        let url = outlook_account_url("user+tag@outlook.com").expect("outlook url");
        assert_eq!(
            url.as_str(),
            "https://outlook.live.com/mail/?login_hint=user%2Btag%40outlook.com"
        );
        assert!(outlook_account_url("").is_err());
        assert!(outlook_account_url("user@outlook.com\n").is_err());
    }
}
