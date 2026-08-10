//! Google OAuth token and OpenID identity HTTP adapter.

#![allow(dead_code)]

use crate::auth::{
    AddAccountError, AddAccountErrorTag, Cancellation, IdentityPort, PortFuture, SensitiveString,
    TokenExchangePort, TokenExchangeRequest, TokenResponse, APPROVED_SCOPES,
};
use reqwest::{redirect::Policy, StatusCode};
use serde::Deserialize;
use std::{collections::HashSet, time::Duration};
use url::Url;
use zeroize::Zeroizing;

const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const IDENTITY_ENDPOINT: &str = "https://openidconnect.googleapis.com/v1/userinfo";
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_TOKEN_RESPONSE_BYTES: usize = 32 * 1024;
const MAX_IDENTITY_RESPONSE_BYTES: usize = 16 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GoogleIdentity {
    pub(crate) email: String,
    pub(crate) display_name: Option<String>,
}

impl std::fmt::Debug for GoogleIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GoogleIdentity")
            .field("email", &"[redacted]")
            .field(
                "display_name",
                &self.display_name.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

pub(crate) struct GoogleHttpClient {
    client: reqwest::Client,
    token_endpoint: Url,
    identity_endpoint: Url,
}

impl GoogleHttpClient {
    pub(crate) fn new() -> Result<Self, AddAccountError> {
        Self::with_endpoints(TOKEN_ENDPOINT, IDENTITY_ENDPOINT)
    }

    fn with_endpoints(token: &str, identity: &str) -> Result<Self, AddAccountError> {
        let client = reqwest::Client::builder()
            .tls_backend_rustls()
            .redirect(Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(HTTP_TIMEOUT)
            .user_agent("Zerochi/0.1")
            .build()
            .map_err(|_| internal())?;
        let token_endpoint = Url::parse(token).map_err(|_| internal())?;
        let identity_endpoint = Url::parse(identity).map_err(|_| internal())?;
        if !cfg!(test)
            && (token_endpoint.scheme() != "https" || identity_endpoint.scheme() != "https")
        {
            return Err(internal());
        }
        Ok(Self {
            client,
            token_endpoint,
            identity_endpoint,
        })
    }
}

impl TokenExchangePort for GoogleHttpClient {
    fn exchange<'a>(
        &'a self,
        request: TokenExchangeRequest<'a>,
        cancellation: &'a Cancellation,
    ) -> PortFuture<'a, Result<TokenResponse, AddAccountError>> {
        Box::pin(async move {
            cancellation.ensure_active()?;
            let body = SensitiveString::new(
                url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("client_id", request.client_id.as_str())
                    .append_pair("code", request.code.expose())
                    .append_pair("code_verifier", request.verifier)
                    .append_pair("grant_type", "authorization_code")
                    .append_pair("redirect_uri", request.redirect_uri)
                    .finish(),
            );
            let response = self
                .client
                .post(self.token_endpoint.clone())
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body.expose().to_owned())
                .send()
                .await
                .map_err(|error| map_transport(error, AddAccountErrorTag::TokenExchangeFailed))?;
            cancellation.ensure_active()?;
            let status = response.status();
            let bytes = read_bounded(
                response,
                MAX_TOKEN_RESPONSE_BYTES,
                AddAccountErrorTag::TokenExchangeFailed,
            )
            .await?;

            if !status.is_success() {
                return Err(classify_token_status(status, &bytes));
            }
            let wire: TokenWire = serde_json::from_slice(&bytes)
                .map_err(|_| AddAccountError::from_tag(AddAccountErrorTag::TokenExchangeFailed))?;
            validate_scopes(&wire.scope)?;
            if wire.access_token.is_empty()
                || wire.expires_in == 0
                || !wire.token_type.eq_ignore_ascii_case("bearer")
                || wire
                    .refresh_token
                    .as_ref()
                    .is_some_and(SensitiveString::is_empty)
            {
                return Err(AddAccountError::from_tag(
                    AddAccountErrorTag::TokenExchangeFailed,
                ));
            }

            Ok(TokenResponse {
                access_token: wire.access_token,
                refresh_token: wire.refresh_token,
                expires_in: Duration::from_secs(wire.expires_in),
            })
        })
    }
}

impl IdentityPort for GoogleHttpClient {
    fn identity<'a>(
        &'a self,
        access_token: &'a SensitiveString,
        cancellation: &'a Cancellation,
    ) -> PortFuture<'a, Result<GoogleIdentity, AddAccountError>> {
        Box::pin(async move {
            cancellation.ensure_active()?;
            let response = self
                .client
                .get(self.identity_endpoint.clone())
                .bearer_auth(access_token.expose())
                .send()
                .await
                .map_err(|error| map_transport(error, AddAccountErrorTag::IdentityInvalid))?;
            cancellation.ensure_active()?;
            let status = response.status();
            let bytes = read_bounded(
                response,
                MAX_IDENTITY_RESPONSE_BYTES,
                AddAccountErrorTag::IdentityInvalid,
            )
            .await?;
            if !status.is_success() {
                let tag = if status == StatusCode::FORBIDDEN {
                    AddAccountErrorTag::WorkspaceRestricted
                } else {
                    AddAccountErrorTag::IdentityInvalid
                };
                return Err(AddAccountError::from_tag(tag));
            }
            let wire: IdentityWire = serde_json::from_slice(&bytes)
                .map_err(|_| AddAccountError::from_tag(AddAccountErrorTag::IdentityInvalid))?;
            validate_identity(wire)
        })
    }
}

#[derive(Deserialize)]
struct TokenWire {
    access_token: SensitiveString,
    refresh_token: Option<SensitiveString>,
    expires_in: u64,
    scope: String,
    token_type: String,
}

#[derive(Deserialize)]
struct IdentityWire {
    email: String,
    email_verified: bool,
    name: Option<String>,
}

#[derive(Deserialize)]
struct ProviderErrorWire {
    error: String,
}

async fn read_bounded(
    mut response: reqwest::Response,
    maximum: usize,
    failure_tag: AddAccountErrorTag,
) -> Result<Zeroizing<Vec<u8>>, AddAccountError> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum as u64)
    {
        return Err(AddAccountError::from_tag(failure_tag));
    }
    let mut bytes = Zeroizing::new(Vec::new());
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| map_transport(error, failure_tag))?
    {
        if bytes.len().saturating_add(chunk.len()) > maximum {
            return Err(AddAccountError::from_tag(failure_tag));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn validate_scopes(scope: &str) -> Result<(), AddAccountError> {
    let actual: Vec<_> = scope.split_ascii_whitespace().collect();
    let unique: HashSet<_> = actual.iter().copied().collect();
    let approved: HashSet<_> = APPROVED_SCOPES.split_ascii_whitespace().collect();
    if actual.len() != unique.len() || unique != approved {
        return Err(AddAccountError::from_tag(
            AddAccountErrorTag::TokenExchangeFailed,
        ));
    }
    Ok(())
}

fn validate_identity(wire: IdentityWire) -> Result<GoogleIdentity, AddAccountError> {
    let email = wire.email.trim();
    let mut segments = email.split('@');
    let valid_email = wire.email_verified
        && email == wire.email
        && email.len() <= 320
        && !email.bytes().any(|byte| byte.is_ascii_control())
        && segments.next().is_some_and(|local| !local.is_empty())
        && segments.next().is_some_and(|domain| !domain.is_empty())
        && segments.next().is_none();
    if !valid_email {
        return Err(AddAccountError::from_tag(
            AddAccountErrorTag::IdentityInvalid,
        ));
    }

    let display_name = wire.name.and_then(|name| {
        let trimmed = name.trim();
        (!trimmed.is_empty()
            && trimmed.len() <= 256
            && !trimmed.bytes().any(|byte| byte.is_ascii_control()))
        .then(|| trimmed.to_owned())
    });
    Ok(GoogleIdentity {
        email: wire.email,
        display_name,
    })
}

fn classify_token_status(status: StatusCode, body: &[u8]) -> AddAccountError {
    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        return AddAccountError::from_tag(AddAccountErrorTag::NetworkUnavailable);
    }
    let tag = serde_json::from_slice::<ProviderErrorWire>(body)
        .ok()
        .map(|wire| match wire.error.as_str() {
            "admin_policy_enforced" | "org_internal" | "restricted_client" => {
                AddAccountErrorTag::WorkspaceRestricted
            }
            "access_denied" => AddAccountErrorTag::ProviderDenied,
            _ => AddAccountErrorTag::TokenExchangeFailed,
        })
        .unwrap_or(AddAccountErrorTag::TokenExchangeFailed);
    AddAccountError::from_tag(tag)
}

fn map_transport(error: reqwest::Error, fallback: AddAccountErrorTag) -> AddAccountError {
    let tag = if error.is_timeout() {
        AddAccountErrorTag::Timeout
    } else if error.is_connect() {
        AddAccountErrorTag::NetworkUnavailable
    } else {
        fallback
    };
    AddAccountError::from_tag(tag)
}

fn internal() -> AddAccountError {
    AddAccountError::from_tag(AddAccountErrorTag::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{ClientId, Environment, SensitiveString};
    use std::{
        ffi::OsString,
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
    };

    const CLIENT_ID: &str =
        "123456789012-abcdefghijklmnopqrstuvwxyz012345.apps.googleusercontent.com";
    const SCOPES_JSON: &str = "openid email profile https://www.googleapis.com/auth/gmail.readonly";

    struct Env;

    impl Environment for Env {
        fn var_os(&self, _name: &str) -> Option<OsString> {
            Some(CLIENT_ID.into())
        }
    }

    fn mock_server(response_body: String, status: &str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let address = listener.local_addr().expect("mock address");
        let (sender, receiver) = mpsc::channel();
        let status = status.to_owned();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept mock request");
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .expect("read timeout");
            let mut bytes = vec![0; 16 * 1024];
            let mut used = 0;
            loop {
                let read = stream.read(&mut bytes[used..]).expect("read mock request");
                used += read;
                let header_end = bytes[..used]
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n");
                if let Some(header_end) = header_end {
                    let header_end = header_end + 4;
                    let headers = String::from_utf8_lossy(&bytes[..header_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|v| v.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    while used < header_end + content_length {
                        let read = stream.read(&mut bytes[used..]).expect("read mock body");
                        used += read;
                    }
                    break;
                }
            }
            let _ = sender.send(String::from_utf8_lossy(&bytes[..used]).into_owned());
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
                response_body.len()
            )
            .expect("write mock response");
        });
        (format!("http://{address}"), receiver)
    }

    fn cancellation() -> (crate::auth::AddAccountAttemptGuard, Cancellation) {
        let attempts = crate::auth::AddAccountAttemptGuardSet::new();
        let guard = attempts.begin().expect("attempt");
        let cancellation = guard.cancellation();
        (guard, cancellation)
    }

    #[test]
    fn exchanges_code_without_client_secret_and_validates_tokens() {
        let body = format!(
            "{{\"access_token\":\"fixture-access\",\"refresh_token\":\"fixture-refresh\",\"expires_in\":3599,\"scope\":\"{SCOPES_JSON}\",\"token_type\":\"Bearer\"}}"
        );
        let (endpoint, captured) = mock_server(body, "200 OK");
        let client = GoogleHttpClient::with_endpoints(&endpoint, &endpoint).expect("client");
        let client_id = ClientId::load_from(&Env).expect("client id");
        let code = SensitiveString::new("fixture-code".into());
        let (_guard, cancellation) = cancellation();
        let response = tauri::async_runtime::block_on(client.exchange(
            TokenExchangeRequest {
                client_id: &client_id,
                redirect_uri: "http://127.0.0.1:49152/oauth/callback",
                code: &code,
                verifier: "fixture-verifier",
            },
            &cancellation,
        ))
        .expect("token exchange");

        assert_eq!(response.expires_in, Duration::from_secs(3599));
        assert_eq!(response.access_token.expose(), "fixture-access");
        assert_eq!(
            response.refresh_token.as_ref().map(SensitiveString::expose),
            Some("fixture-refresh")
        );
        let request = captured.recv().expect("captured request");
        assert!(request.contains("code=fixture-code"));
        assert!(request.contains("code_verifier=fixture-verifier"));
        assert!(!request.contains("client_secret"));
    }

    #[test]
    fn rejects_missing_or_broadened_scopes_invalid_expiry_and_oversized_body() {
        for body in [
            "{\"access_token\":\"a\",\"refresh_token\":\"r\",\"expires_in\":0,\"scope\":\"openid email profile https://www.googleapis.com/auth/gmail.readonly\",\"token_type\":\"Bearer\"}".to_owned(),
            "{\"access_token\":\"a\",\"refresh_token\":\"r\",\"expires_in\":1,\"scope\":\"openid email profile https://www.googleapis.com/auth/gmail.readonly extra\",\"token_type\":\"Bearer\"}".to_owned(),
            "x".repeat(MAX_TOKEN_RESPONSE_BYTES + 1),
        ] {
            let (endpoint, _) = mock_server(body, "200 OK");
            let client = GoogleHttpClient::with_endpoints(&endpoint, &endpoint).expect("client");
            let client_id = ClientId::load_from(&Env).expect("client id");
            let code = SensitiveString::new("fixture-code".into());
            let (_guard, cancellation) = cancellation();
            let error = tauri::async_runtime::block_on(client.exchange(
                TokenExchangeRequest {
                    client_id: &client_id,
                    redirect_uri: "http://127.0.0.1:49152/oauth/callback",
                    code: &code,
                    verifier: "fixture-verifier",
                },
                &cancellation,
            ))
            .unwrap_err();
            assert_eq!(error.tag(), AddAccountErrorTag::TokenExchangeFailed);
        }
    }

    #[test]
    fn fetches_only_a_verified_identity_with_bearer_access() {
        let (endpoint, captured) = mock_server(
            "{\"email\":\"person@example.test\",\"email_verified\":true,\"name\":\"Person\"}"
                .into(),
            "200 OK",
        );
        let client = GoogleHttpClient::with_endpoints(&endpoint, &endpoint).expect("client");
        let access = SensitiveString::new("fixture-access".into());
        let (_guard, cancellation) = cancellation();
        let identity = tauri::async_runtime::block_on(client.identity(&access, &cancellation))
            .expect("identity");

        assert_eq!(identity.email, "person@example.test");
        assert_eq!(identity.display_name.as_deref(), Some("Person"));
        let request = captured.recv().expect("captured request");
        assert!(
            request.contains("authorization: Bearer fixture-access")
                || request.contains("Authorization: Bearer fixture-access")
        );
    }

    #[test]
    fn unverified_identity_and_workspace_forbidden_are_sanitized() {
        let (endpoint, _) = mock_server(
            "{\"email\":\"person@example.test\",\"email_verified\":false}".into(),
            "200 OK",
        );
        let client = GoogleHttpClient::with_endpoints(&endpoint, &endpoint).expect("client");
        let access = SensitiveString::new("fixture-access".into());
        let (_guard, cancellation) = cancellation();
        let error =
            tauri::async_runtime::block_on(client.identity(&access, &cancellation)).unwrap_err();
        assert_eq!(error.tag(), AddAccountErrorTag::IdentityInvalid);

        let (endpoint, _) = mock_server("provider body fixture-access".into(), "403 Forbidden");
        let client = GoogleHttpClient::with_endpoints(&endpoint, &endpoint).expect("client");
        let error =
            tauri::async_runtime::block_on(client.identity(&access, &cancellation)).unwrap_err();
        assert_eq!(error.tag(), AddAccountErrorTag::WorkspaceRestricted);
        assert!(!format!("{error:?}").contains("fixture-access"));
    }
}
