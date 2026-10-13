use super::{AddAccountError, AddAccountErrorTag, ClientId, Pkce, SensitiveString};
use std::fmt;
use url::{Host, Url};

const GOOGLE_AUTHORIZATION_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const CALLBACK_PATH: &str = "/oauth/callback";
pub(crate) const APPROVED_SCOPES: &str = "https://www.googleapis.com/auth/gmail.readonly";
// Personal Microsoft accounts only: the `consumers` authority matches the Outlook.com web
// navigation target. Work/school tenants need their own authority and admin-consent handling.
const MICROSOFT_AUTHORIZATION_ENDPOINT: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
/// Read-only Graph mail plus the identity read and refresh-token grant it depends on.
pub(crate) const MICROSOFT_SCOPES: &str = "offline_access User.Read Mail.Read";

/// Exact `http://localhost:<port>` redirect registered for an Entra public client.
pub(crate) struct LocalhostRedirect(String);

impl LocalhostRedirect {
    pub(crate) fn parse(value: &str) -> Result<Self, AddAccountError> {
        let valid = value.strip_prefix("http://localhost:").is_some_and(|port| {
            !port.is_empty()
                && port.bytes().all(|byte| byte.is_ascii_digit())
                && port.parse::<u16>().is_ok_and(|port| port != 0)
        });
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(callback_invalid())
        }
    }
}

impl fmt::Debug for LocalhostRedirect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LocalhostRedirect([REDACTED])")
    }
}

pub(crate) struct LoopbackRedirect(Url);

impl LoopbackRedirect {
    pub(crate) fn parse(value: &str) -> Result<Self, AddAccountError> {
        let parsed = Url::parse(value).map_err(|_| callback_invalid())?;
        let valid = parsed.scheme() == "http"
            && parsed.host() == Some(Host::Ipv4(std::net::Ipv4Addr::LOCALHOST))
            && parsed.port().is_some()
            && parsed.path() == CALLBACK_PATH
            && parsed.query().is_none()
            && parsed.fragment().is_none()
            && parsed.username().is_empty()
            && parsed.password().is_none();

        if valid {
            Ok(Self(parsed))
        } else {
            Err(callback_invalid())
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for LoopbackRedirect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LoopbackRedirect([REDACTED])")
    }
}

pub(crate) struct AuthorizationRequest {
    url: SensitiveString,
    redirect_uri: SensitiveString,
    verifier: SensitiveString,
    expected_state: SensitiveString,
}

impl AuthorizationRequest {
    pub(crate) fn new(
        client_id: &ClientId,
        redirect: LoopbackRedirect,
        pkce: Pkce,
    ) -> Result<Self, AddAccountError> {
        let (verifier, challenge, expected_state) = pkce.into_parts();
        let mut url = Url::parse(GOOGLE_AUTHORIZATION_ENDPOINT)
            .map_err(|_| AddAccountError::from_tag(AddAccountErrorTag::Internal))?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("client_id", client_id.as_str())
                .append_pair("redirect_uri", redirect.as_str())
                .append_pair("response_type", "code")
                .append_pair("scope", APPROVED_SCOPES)
                .append_pair("access_type", "offline")
                .append_pair("prompt", "consent")
                .append_pair("code_challenge", challenge.expose())
                .append_pair("code_challenge_method", "S256")
                .append_pair("state", expected_state.expose());
        }

        Ok(Self {
            url: SensitiveString::new(url.into()),
            redirect_uri: SensitiveString::new(redirect.0.into()),
            verifier,
            expected_state,
        })
    }

    pub(crate) fn microsoft(
        client_id: &ClientId,
        redirect: LocalhostRedirect,
        pkce: Pkce,
    ) -> Result<Self, AddAccountError> {
        let (verifier, challenge, expected_state) = pkce.into_parts();
        let mut url = Url::parse(MICROSOFT_AUTHORIZATION_ENDPOINT)
            .map_err(|_| AddAccountError::from_tag(AddAccountErrorTag::Internal))?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("client_id", client_id.as_str())
                .append_pair("redirect_uri", &redirect.0)
                .append_pair("response_type", "code")
                .append_pair("response_mode", "query")
                .append_pair("scope", MICROSOFT_SCOPES)
                .append_pair("prompt", "select_account")
                .append_pair("code_challenge", challenge.expose())
                .append_pair("code_challenge_method", "S256")
                .append_pair("state", expected_state.expose());
        }

        Ok(Self {
            url: SensitiveString::new(url.into()),
            redirect_uri: SensitiveString::new(redirect.0),
            verifier,
            expected_state,
        })
    }

    pub(crate) fn url(&self) -> &str {
        self.url.expose()
    }

    pub(crate) fn redirect_uri(&self) -> &str {
        self.redirect_uri.expose()
    }

    pub(crate) fn verifier(&self) -> &str {
        self.verifier.expose()
    }

    pub(crate) fn expected_state(&self) -> &str {
        self.expected_state.expose()
    }
}

impl fmt::Debug for AuthorizationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthorizationRequest([REDACTED])")
    }
}

fn callback_invalid() -> AddAccountError {
    AddAccountError::from_tag(AddAccountErrorTag::CallbackInvalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{generate_pkce, Environment, RandomSource};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use sha2::{Digest, Sha256};
    use std::{
        collections::HashMap,
        ffi::OsString,
        sync::atomic::{AtomicU8, Ordering},
    };

    const CLIENT_ID: &str =
        "123456789012-abcdefghijklmnopqrstuvwxyz012345.apps.googleusercontent.com";

    struct FixedEnvironment;

    impl Environment for FixedEnvironment {
        fn var_os(&self, name: &str) -> Option<OsString> {
            let values = HashMap::from([("ZEROCHI_GOOGLE_CLIENT_ID", CLIENT_ID)]);
            values.get(name).map(OsString::from)
        }
    }

    struct FixedRandom(AtomicU8);

    impl FixedRandom {
        fn new() -> Self {
            Self(AtomicU8::new(0x5a))
        }
    }

    impl RandomSource for FixedRandom {
        fn fill(&self, destination: &mut [u8]) -> Result<(), AddAccountError> {
            let byte = self.0.swap(0xa5, Ordering::Relaxed);
            destination.fill(byte);
            Ok(())
        }
    }

    fn request() -> AuthorizationRequest {
        let client_id = ClientId::load_from(&FixedEnvironment).expect("client id");
        let redirect = LoopbackRedirect::parse("http://127.0.0.1:49152/oauth/callback")
            .expect("loopback redirect");
        let pkce = generate_pkce(&FixedRandom::new()).expect("pkce");
        AuthorizationRequest::new(&client_id, redirect, pkce).expect("authorization request")
    }

    #[test]
    fn constructs_the_exact_google_desktop_authorization_request() {
        let request = request();
        let parsed = Url::parse(request.url()).expect("authorization URL");
        let pairs: Vec<_> = parsed.query_pairs().into_owned().collect();

        assert_eq!(parsed.scheme(), "https");
        assert_eq!(parsed.host_str(), Some("accounts.google.com"));
        assert_eq!(parsed.path(), "/o/oauth2/v2/auth");
        assert_eq!(pairs.len(), 9);
        assert_eq!(
            pairs
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            [
                "client_id",
                "redirect_uri",
                "response_type",
                "scope",
                "access_type",
                "prompt",
                "code_challenge",
                "code_challenge_method",
                "state"
            ]
        );
        assert_eq!(pairs[0], ("client_id".into(), CLIENT_ID.into()));
        assert_eq!(
            pairs[1],
            (
                "redirect_uri".into(),
                "http://127.0.0.1:49152/oauth/callback".into()
            )
        );
        assert_eq!(pairs[2], ("response_type".into(), "code".into()));
        assert_eq!(pairs[3], ("scope".into(), APPROVED_SCOPES.into()));
        assert_eq!(pairs[4], ("access_type".into(), "offline".into()));
        assert_eq!(pairs[5], ("prompt".into(), "consent".into()));
        assert_eq!(
            pairs[6],
            (
                "code_challenge".into(),
                URL_SAFE_NO_PAD.encode(Sha256::digest(request.verifier().as_bytes()))
            )
        );
        assert_eq!(pairs[7], ("code_challenge_method".into(), "S256".into()));
        assert_eq!(pairs[8], ("state".into(), request.expected_state().into()));
    }

    #[test]
    fn request_contains_only_the_approved_scopes_and_no_secret_or_verifier_parameter() {
        let request = request();
        let parsed = Url::parse(request.url()).expect("authorization URL");
        let pairs: HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        let scopes: Vec<_> = pairs["scope"].split(' ').collect();

        assert_eq!(scopes, ["https://www.googleapis.com/auth/gmail.readonly"]);
        assert!(!pairs.contains_key("client_secret"));
        assert!(!pairs.contains_key("code_verifier"));
        assert!(!request.url().contains(request.verifier()));
        assert!(!request.url().contains("fixture-secret"));
    }

    #[test]
    fn loopback_redirect_is_exact_and_fail_closed() {
        for invalid in [
            "https://127.0.0.1:49152/oauth/callback",
            "http://localhost:49152/oauth/callback",
            "http://0.0.0.0:49152/oauth/callback",
            "http://127.0.0.1/oauth/callback",
            "http://127.0.0.1:49152/other",
            "http://127.0.0.1:49152/oauth/callback?code=value",
            "http://127.0.0.1:49152/oauth/callback#fragment",
            "not a URL",
        ] {
            let error = LoopbackRedirect::parse(invalid).unwrap_err();
            assert_eq!(error.tag(), AddAccountErrorTag::CallbackInvalid);
            assert!(!format!("{error:?}").contains(invalid));
        }
    }

    #[test]
    fn request_debug_redacts_url_client_id_state_and_verifier() {
        let request = request();
        let rendered = format!("{request:?}");

        assert_eq!(rendered, "AuthorizationRequest([REDACTED])");
        assert!(!rendered.contains(CLIENT_ID));
        assert!(!rendered.contains(request.expected_state()));
        assert!(!rendered.contains(request.verifier()));
    }

    const MICROSOFT_CLIENT_ID: &str = "00000000-1111-2222-3333-444444444444";

    #[test]
    fn constructs_the_microsoft_public_client_request_for_the_localhost_redirect() {
        let client_id =
            ClientId::parse_microsoft(MICROSOFT_CLIENT_ID.to_owned()).expect("client id");
        let redirect = LocalhostRedirect::parse("http://localhost:49152").expect("redirect");
        let pkce = generate_pkce(&FixedRandom::new()).expect("pkce");
        let request = AuthorizationRequest::microsoft(&client_id, redirect, pkce).expect("request");
        let parsed = Url::parse(request.url()).expect("authorization URL");
        let pairs: HashMap<_, _> = parsed.query_pairs().into_owned().collect();

        assert_eq!(parsed.scheme(), "https");
        assert_eq!(parsed.host_str(), Some("login.microsoftonline.com"));
        assert_eq!(parsed.path(), "/consumers/oauth2/v2.0/authorize");
        assert_eq!(pairs.len(), 9);
        assert_eq!(pairs["client_id"], MICROSOFT_CLIENT_ID);
        assert_eq!(pairs["redirect_uri"], "http://localhost:49152");
        assert_eq!(pairs["response_type"], "code");
        assert_eq!(pairs["response_mode"], "query");
        assert_eq!(pairs["scope"], "offline_access User.Read Mail.Read");
        assert_eq!(pairs["code_challenge_method"], "S256");
        assert_eq!(pairs["state"], request.expected_state());
        assert_eq!(
            pairs["code_challenge"],
            URL_SAFE_NO_PAD.encode(Sha256::digest(request.verifier().as_bytes()))
        );
        assert!(!pairs.contains_key("client_secret"));
        assert!(!request.url().contains(request.verifier()));
        assert_eq!(request.redirect_uri(), "http://localhost:49152");
    }

    #[test]
    fn microsoft_client_id_must_be_a_hyphenated_guid() {
        for invalid in [
            "",
            "00000000111122223333444444444444",
            "00000000-1111-2222-3333-44444444444",
            "00000000-1111-2222-3333-4444444444445",
            "0000000g-1111-2222-3333-444444444444",
            "123456789012-abcdefghijklmnopqrstuvwxyz012345.apps.googleusercontent.com",
        ] {
            let error = ClientId::parse_microsoft(invalid.to_owned()).unwrap_err();
            assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
        }
    }

    #[test]
    fn localhost_redirect_is_exact_and_fail_closed() {
        for invalid in [
            "http://localhost",
            "http://localhost:",
            "http://localhost:0",
            "http://localhost:99999",
            "http://localhost:49152/",
            "http://localhost:49152/oauth/callback",
            "http://localhost:49152@example.test",
            "http://localhost.example.test:49152",
            "https://localhost:49152",
            "http://127.0.0.1:49152",
        ] {
            let error = LocalhostRedirect::parse(invalid).unwrap_err();
            assert_eq!(error.tag(), AddAccountErrorTag::CallbackInvalid);
            assert!(!format!("{error:?}").contains(invalid));
        }
    }
}
