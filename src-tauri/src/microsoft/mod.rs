//! Microsoft identity platform token and read-only Microsoft Graph mail adapter.
//!
//! It implements the same provider seams as the Gmail adapter, so the sync engine, persistence,
//! notifications, and tray are unchanged. Graph has no history cursor comparable to Gmail's, so
//! the cursor is the newest observed Inbox `receivedDateTime` as epoch seconds plus one: digits
//! only, like a Gmail history id, and never expiring.

use crate::{
    accounts::AccountStatePort,
    auth::{
        AddAccountError, AddAccountErrorTag, Cancellation, ClientId, ClientSecret, IdentityPort,
        PortFuture, RefreshTokenReadError, RefreshTokenReadPort, RefreshTokenStorePort,
        SensitiveString, TokenExchangePort, TokenExchangeRequest, TokenResponse, MICROSOFT_SCOPES,
    },
    gmail::{
        AccessTokenRefreshPort, GmailChanges, GmailFuture, GmailHistoryEvent, GmailMessageMetadata,
        GmailProfile, GmailReadError, GmailReadPort, GoogleIdentity, MAX_UNREAD_MESSAGE_IDS,
    },
};
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::{redirect::Policy, StatusCode};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

const TOKEN_ENDPOINT: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const GRAPH_ME_ENDPOINT: &str = "https://graph.microsoft.com/v1.0/me";
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
// Entra access tokens are larger than Google's; refresh responses carry two of them.
const MAX_TOKEN_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_IDENTITY_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_GRAPH_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_REFRESH_TOKEN_BYTES: usize = 8_192;
const UNREAD_PAGE_SIZE: &str = "1000";
const MAX_UNREAD_PAGES: usize = 30;
const CHANGES_PAGE_SIZE: &str = "50";
const MAX_CHANGES_PAGES: usize = 10;
const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_HEADER_BYTES: usize = 2_048;
// Immutable ids survive moves between folders, so a stored id keeps naming the same message.
const PREFER_IMMUTABLE_ID: &str = "IdType=\"ImmutableId\"";
const ACCESS_TOKEN_EXPIRY_MARGIN: Duration = Duration::from_secs(120);

type TokenDigest = [u8; 32];

/// Receives the replacement refresh token Entra issues on every refresh grant.
pub(crate) trait RefreshTokenRotationPort: Send + Sync {
    fn rotated<'a>(&'a self, previous: &'a [u8], next: &'a [u8]) -> GmailFuture<'a, ()>;
}

struct CachedAccessToken {
    value: Zeroizing<String>,
    expires_at: Instant,
}

pub(crate) struct MicrosoftHttpClient {
    client: reqwest::Client,
    token_endpoint: Url,
    me_endpoint: Url,
    // Entra rotates the refresh token on every grant; caching the access token for its lifetime
    // keeps rotation (and its Keychain write) hourly instead of once per 30-second poll.
    access_tokens: Mutex<HashMap<TokenDigest, CachedAccessToken>>,
    rotation: Option<Arc<dyn RefreshTokenRotationPort>>,
}

impl MicrosoftHttpClient {
    pub(crate) fn new() -> Result<Self, AddAccountError> {
        Self::with_endpoints(TOKEN_ENDPOINT, GRAPH_ME_ENDPOINT)
    }

    pub(crate) fn with_rotation(mut self, rotation: Arc<dyn RefreshTokenRotationPort>) -> Self {
        self.rotation = Some(rotation);
        self
    }

    fn with_endpoints(token: &str, me: &str) -> Result<Self, AddAccountError> {
        let client = reqwest::Client::builder()
            .tls_backend_rustls()
            .redirect(Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(HTTP_TIMEOUT)
            .user_agent("Zerochi/0.1")
            .build()
            .map_err(|_| internal())?;
        let token_endpoint = Url::parse(token).map_err(|_| internal())?;
        let me_endpoint = Url::parse(me).map_err(|_| internal())?;
        if !cfg!(test) && (token_endpoint.scheme() != "https" || me_endpoint.scheme() != "https") {
            return Err(internal());
        }
        Ok(Self {
            client,
            token_endpoint,
            me_endpoint,
            access_tokens: Mutex::new(HashMap::new()),
            rotation: None,
        })
    }

    fn graph_url(&self, segments: &[&str], query: &[(&str, &str)]) -> Result<Url, GmailReadError> {
        let mut url = self.me_endpoint.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| GmailReadError::InvalidResponse)?;
            path.pop_if_empty();
            for segment in segments {
                path.push(segment);
            }
        }
        // OData expects `%20`, not the `+` that form encoding produces for spaces.
        let encoded = query
            .iter()
            .map(|(name, value)| format!("{name}={}", percent_encode(value)))
            .collect::<Vec<_>>()
            .join("&");
        url.set_query((!encoded.is_empty()).then_some(encoded.as_str()));
        Ok(url)
    }

    /// Follows only next links that stay on the configured Graph origin and path prefix.
    fn next_page_url(&self, next_link: Option<String>) -> Result<Option<Url>, GmailReadError> {
        let Some(next_link) = next_link else {
            return Ok(None);
        };
        if next_link.len() > 4_096 {
            return Err(GmailReadError::InvalidResponse);
        }
        let url = Url::parse(&next_link).map_err(|_| GmailReadError::InvalidResponse)?;
        let same_origin = url.scheme() == self.me_endpoint.scheme()
            && url.host() == self.me_endpoint.host()
            && url.port_or_known_default() == self.me_endpoint.port_or_known_default()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path().starts_with(self.me_endpoint.path());
        if same_origin {
            Ok(Some(url))
        } else {
            Err(GmailReadError::InvalidResponse)
        }
    }

    async fn graph_get(
        &self,
        url: Url,
        access_token: &SensitiveString,
    ) -> Result<reqwest::Response, GmailReadError> {
        self.client
            .get(url)
            .bearer_auth(access_token.expose())
            .header("Prefer", PREFER_IMMUTABLE_ID)
            .send()
            .await
            .map_err(|_| GmailReadError::Unavailable)
    }

    async fn graph_page<Wire: for<'de> Deserialize<'de>>(
        &self,
        url: Url,
        access_token: &SensitiveString,
    ) -> Result<Wire, GmailReadError> {
        let response = self.graph_get(url, access_token).await?;
        if !response.status().is_success() {
            return Err(map_graph_status(response.status()));
        }
        let bytes = read_graph_bounded(response, MAX_GRAPH_RESPONSE_BYTES).await?;
        serde_json::from_slice(&bytes).map_err(|_| GmailReadError::InvalidResponse)
    }

    async fn mailbox_email(
        &self,
        access_token: &SensitiveString,
    ) -> Result<String, GmailReadError> {
        let url = self.graph_url(&[], &[("$select", "mail,userPrincipalName")])?;
        let response = self.graph_get(url, access_token).await?;
        if !response.status().is_success() {
            return Err(map_graph_status(response.status()));
        }
        let bytes = read_graph_bounded(response, MAX_IDENTITY_RESPONSE_BYTES).await?;
        let wire: UserWire =
            serde_json::from_slice(&bytes).map_err(|_| GmailReadError::InvalidResponse)?;
        wire.email().ok_or(GmailReadError::InvalidResponse)
    }

    fn cached_access_token(&self, digest: &TokenDigest) -> Option<SensitiveString> {
        let cache = self.access_tokens.lock().ok()?;
        cache
            .get(digest)
            .filter(|cached| cached.expires_at > Instant::now() + ACCESS_TOKEN_EXPIRY_MARGIN)
            .map(|cached| SensitiveString::new(cached.value.as_str().to_owned()))
    }

    fn cache_access_token(&self, digests: &[TokenDigest], value: &str, lifetime: Duration) {
        let Ok(mut cache) = self.access_tokens.lock() else {
            return;
        };
        let now = Instant::now();
        cache.retain(|_, cached| cached.expires_at > now);
        for digest in digests {
            cache.insert(
                *digest,
                CachedAccessToken {
                    value: Zeroizing::new(value.to_owned()),
                    expires_at: now + lifetime,
                },
            );
        }
    }
}

impl GmailReadPort for MicrosoftHttpClient {
    fn profile<'a>(
        &'a self,
        access_token: &'a SensitiveString,
    ) -> GmailFuture<'a, Result<GmailProfile, GmailReadError>> {
        Box::pin(async move {
            let email = self.mailbox_email(access_token).await?;
            let url = self.graph_url(
                &["mailFolders", "inbox", "messages"],
                &[
                    ("$select", "receivedDateTime"),
                    ("$orderby", "receivedDateTime desc"),
                    ("$top", "1"),
                ],
            )?;
            let page: MessagePageWire = self.graph_page(url, access_token).await?;
            let newest = page
                .value
                .iter()
                .filter_map(|message| message.received_epoch_seconds())
                .max();
            Ok(GmailProfile {
                email,
                history_id: cursor_after(newest, 0).to_string(),
            })
        })
    }

    fn unread_message_ids<'a>(
        &'a self,
        access_token: &'a SensitiveString,
    ) -> GmailFuture<'a, Result<Vec<String>, GmailReadError>> {
        Box::pin(async move {
            let mut next = Some(self.graph_url(
                &["mailFolders", "inbox", "messages"],
                &[
                    ("$filter", "isRead eq false"),
                    ("$select", "id"),
                    ("$top", UNREAD_PAGE_SIZE),
                ],
            )?);
            let mut ids = Vec::new();
            let mut seen = HashSet::new();
            for _ in 0..MAX_UNREAD_PAGES {
                let Some(url) = next.take() else {
                    return Ok(ids);
                };
                let page: MessagePageWire = self.graph_page(url, access_token).await?;
                for message in page.value {
                    // An id outside the persisted identifier alphabet cannot be stored; skipping
                    // it undercounts by one instead of failing the whole account.
                    let Some(id) = stored_identifier(&message.id) else {
                        continue;
                    };
                    if seen.insert(id.clone()) {
                        if ids.len() >= MAX_UNREAD_MESSAGE_IDS {
                            return Err(GmailReadError::InvalidResponse);
                        }
                        ids.push(id);
                    }
                }
                next = self.next_page_url(page.next_link)?;
            }
            if next.is_none() {
                Ok(ids)
            } else {
                Err(GmailReadError::InvalidResponse)
            }
        })
    }

    fn changes_since<'a>(
        &'a self,
        access_token: &'a SensitiveString,
        cursor: &'a str,
    ) -> GmailFuture<'a, Result<GmailChanges, GmailReadError>> {
        Box::pin(async move {
            let since = cursor
                .parse::<i64>()
                .ok()
                .filter(|seconds| *seconds >= 0)
                .ok_or(GmailReadError::InvalidResponse)?;
            let since_timestamp = DateTime::<Utc>::from_timestamp(since, 0)
                .ok_or(GmailReadError::InvalidResponse)?
                .to_rfc3339_opts(SecondsFormat::Secs, true);
            let filter = format!("receivedDateTime ge {since_timestamp}");
            let mut next = Some(self.graph_url(
                &["mailFolders", "inbox", "messages"],
                &[
                    ("$filter", filter.as_str()),
                    ("$orderby", "receivedDateTime desc"),
                    ("$select", "id,conversationId,receivedDateTime"),
                    ("$top", CHANGES_PAGE_SIZE),
                ],
            )?);
            let mut events = Vec::new();
            let mut newest = None;
            // A burst larger than the page budget still advances the cursor past it; the unread
            // set stays authoritative, only the oldest notifications of the burst are dropped.
            for _ in 0..MAX_CHANGES_PAGES {
                let Some(url) = next.take() else {
                    break;
                };
                let page: MessagePageWire = self.graph_page(url, access_token).await?;
                for message in page.value {
                    let Some(received) = message.received_epoch_seconds() else {
                        continue;
                    };
                    newest = newest.max(Some(received));
                    let (Some(id), Some(thread_id)) = (
                        stored_identifier(&message.id),
                        message
                            .conversation_id
                            .as_deref()
                            .and_then(stored_identifier),
                    ) else {
                        continue;
                    };
                    events.push(GmailHistoryEvent::MessageAdded {
                        id,
                        thread_id,
                        label_ids: Vec::new(),
                    });
                }
                next = self.next_page_url(page.next_link)?;
            }
            Ok(GmailChanges {
                cursor: cursor_after(newest, since).to_string(),
                events,
            })
        })
    }

    fn message_metadata<'a>(
        &'a self,
        access_token: &'a SensitiveString,
        message_id: &'a str,
    ) -> GmailFuture<'a, Result<Option<GmailMessageMetadata>, GmailReadError>> {
        Box::pin(async move {
            let provider_id = provider_identifier(message_id);
            let url = self.graph_url(
                &["messages", provider_id.as_str()],
                &[(
                    "$select",
                    "id,conversationId,subject,from,receivedDateTime,isRead",
                )],
            )?;
            let response = self.graph_get(url, access_token).await?;
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(None);
            }
            if !response.status().is_success() {
                return Err(map_graph_status(response.status()));
            }
            let bytes = read_graph_bounded(response, MAX_GRAPH_RESPONSE_BYTES).await?;
            let wire: MessageWire =
                serde_json::from_slice(&bytes).map_err(|_| GmailReadError::InvalidResponse)?;
            let thread_id = wire
                .conversation_id
                .as_deref()
                .and_then(stored_identifier)
                .ok_or(GmailReadError::InvalidResponse)?;
            let received_at = wire
                .received_date_time
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .ok_or(GmailReadError::InvalidResponse)?
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Millis, true);
            Ok(Some(GmailMessageMetadata {
                // The requested id is the stored form the engine already reconciled against.
                id: message_id.to_owned(),
                thread_id,
                sender: wire.from.map(|from| from.display()).unwrap_or_default(),
                subject: bounded_header(wire.subject.as_deref().unwrap_or_default()),
                received_at,
                // Candidates and the unread set are both read from the Inbox folder.
                is_inbox_unread: wire.is_read == Some(false),
            }))
        })
    }
}

impl TokenExchangePort for MicrosoftHttpClient {
    fn exchange<'a>(
        &'a self,
        request: TokenExchangeRequest<'a>,
        cancellation: &'a Cancellation,
    ) -> PortFuture<'a, Result<TokenResponse, AddAccountError>> {
        Box::pin(async move {
            cancellation.ensure_active()?;
            // Public client: PKCE is the only proof. No client secret is ever sent.
            let body = SensitiveString::new(
                url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("client_id", request.client_id.as_str())
                    .append_pair("code", request.code.expose())
                    .append_pair("code_verifier", request.verifier)
                    .append_pair("grant_type", "authorization_code")
                    .append_pair("redirect_uri", request.redirect_uri)
                    .append_pair("scope", MICROSOFT_SCOPES)
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
            let wire: TokenWire =
                serde_json::from_slice(&bytes).map_err(|_| token_exchange_failed())?;
            if wire.access_token.is_empty()
                || wire.expires_in == 0
                || !wire.token_type.eq_ignore_ascii_case("bearer")
                || !grants_mail_read(wire.scope.as_deref().unwrap_or_default())
                || wire
                    .refresh_token
                    .as_ref()
                    .is_some_and(SensitiveString::is_empty)
            {
                return Err(token_exchange_failed());
            }
            Ok(TokenResponse {
                access_token: wire.access_token,
                refresh_token: wire.refresh_token,
                expires_in: Duration::from_secs(wire.expires_in),
            })
        })
    }
}

impl AccessTokenRefreshPort for MicrosoftHttpClient {
    fn refresh_access_token<'a>(
        &'a self,
        client_id: &'a ClientId,
        _client_secret: &'a ClientSecret,
        refresh_token: &'a [u8],
    ) -> GmailFuture<'a, Result<SensitiveString, GmailReadError>> {
        Box::pin(async move {
            let presented =
                std::str::from_utf8(refresh_token).map_err(|_| GmailReadError::AuthRequired)?;
            if presented.is_empty() || presented.len() > MAX_REFRESH_TOKEN_BYTES {
                return Err(GmailReadError::AuthRequired);
            }
            let presented_digest = token_digest(refresh_token);
            if let Some(cached) = self.cached_access_token(&presented_digest) {
                return Ok(cached);
            }
            let body = SensitiveString::new(
                url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("client_id", client_id.as_str())
                    .append_pair("refresh_token", presented)
                    .append_pair("grant_type", "refresh_token")
                    .append_pair("scope", MICROSOFT_SCOPES)
                    .finish(),
            );
            let response = self
                .client
                .post(self.token_endpoint.clone())
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body.expose().to_owned())
                .send()
                .await
                .map_err(|_| GmailReadError::Unavailable)?;
            if !response.status().is_success() {
                return Err(map_refresh_status(response.status()));
            }
            let bytes = read_graph_bounded(response, MAX_TOKEN_RESPONSE_BYTES).await?;
            let wire: TokenWire =
                serde_json::from_slice(&bytes).map_err(|_| GmailReadError::InvalidResponse)?;
            if wire.access_token.is_empty()
                || wire.expires_in == 0
                || !wire.token_type.eq_ignore_ascii_case("bearer")
            {
                return Err(GmailReadError::InvalidResponse);
            }

            let mut digests = vec![presented_digest];
            let replacement = wire.refresh_token.filter(|next| {
                !next.is_empty()
                    && next.expose().len() <= MAX_REFRESH_TOKEN_BYTES
                    && next.expose_bytes() != refresh_token
            });
            if let Some(next) = &replacement {
                digests.push(token_digest(next.expose_bytes()));
            }
            self.cache_access_token(
                &digests,
                wire.access_token.expose(),
                Duration::from_secs(wire.expires_in),
            );
            if let (Some(next), Some(rotation)) = (&replacement, &self.rotation) {
                rotation.rotated(refresh_token, next.expose_bytes()).await;
            }
            Ok(wire.access_token)
        })
    }
}

impl IdentityPort for MicrosoftHttpClient {
    fn identity<'a>(
        &'a self,
        access_token: &'a SensitiveString,
        cancellation: &'a Cancellation,
    ) -> PortFuture<'a, Result<GoogleIdentity, AddAccountError>> {
        Box::pin(async move {
            cancellation.ensure_active()?;
            let email = self.mailbox_email(access_token).await.map_err(|error| {
                AddAccountError::from_tag(match error {
                    GmailReadError::Unavailable => AddAccountErrorTag::NetworkUnavailable,
                    _ => AddAccountErrorTag::IdentityInvalid,
                })
            })?;
            cancellation.ensure_active()?;
            Ok(GoogleIdentity {
                email,
                display_name: None,
            })
        })
    }
}

/// Refresh-token reader for Outlook accounts that also persists Entra's rotated replacements.
///
/// The refresh port only sees token bytes, so this remembers which account each served token
/// belongs to and writes the replacement back to that account's secure-store entry.
pub(crate) struct RotatingRefreshTokens {
    read: Arc<dyn RefreshTokenReadPort>,
    store: Arc<dyn RefreshTokenStorePort>,
    accounts: Arc<dyn AccountStatePort>,
    served: Mutex<HashMap<TokenDigest, Uuid>>,
}

impl RotatingRefreshTokens {
    pub(crate) fn new(
        read: Arc<dyn RefreshTokenReadPort>,
        store: Arc<dyn RefreshTokenStorePort>,
        accounts: Arc<dyn AccountStatePort>,
    ) -> Self {
        Self {
            read,
            store,
            accounts,
            served: Mutex::new(HashMap::new()),
        }
    }

    fn account_exists(&self, account_id: Uuid) -> bool {
        let canonical = account_id.hyphenated().to_string();
        self.accounts
            .snapshot()
            .is_ok_and(|accounts| accounts.iter().any(|account| account.id() == canonical))
    }
}

impl RefreshTokenReadPort for RotatingRefreshTokens {
    fn get<'a>(
        &'a self,
        account_id: Uuid,
    ) -> PortFuture<'a, Result<Option<Zeroizing<Vec<u8>>>, RefreshTokenReadError>> {
        Box::pin(async move {
            let token = self.read.get(account_id).await?;
            if let (Some(token), Ok(mut served)) = (&token, self.served.lock()) {
                served.retain(|_, served_account| *served_account != account_id);
                served.insert(token_digest(token.as_slice()), account_id);
            }
            Ok(token)
        })
    }
}

impl RefreshTokenRotationPort for RotatingRefreshTokens {
    fn rotated<'a>(&'a self, previous: &'a [u8], next: &'a [u8]) -> GmailFuture<'a, ()> {
        Box::pin(async move {
            let Some(account_id) = self
                .served
                .lock()
                .ok()
                .and_then(|served| served.get(&token_digest(previous)).copied())
            else {
                return;
            };
            if !self.account_exists(account_id) {
                return;
            }
            // The previous token stays valid until its own expiry, so a failed write only delays
            // adopting the replacement.
            if self.store.set(account_id, next).await.is_err() {
                return;
            }
            // A concurrent unlink may have deleted the entry before this write recreated it.
            if !self.account_exists(account_id) {
                let _ = self.store.delete(account_id).await;
                return;
            }
            if let Ok(mut served) = self.served.lock() {
                served.retain(|_, served_account| *served_account != account_id);
                served.insert(token_digest(next), account_id);
            }
        })
    }
}

#[derive(Deserialize)]
struct MessagePageWire {
    #[serde(default)]
    value: Vec<MessageWire>,
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageWire {
    #[serde(default)]
    id: String,
    conversation_id: Option<String>,
    subject: Option<String>,
    from: Option<RecipientWire>,
    received_date_time: Option<String>,
    is_read: Option<bool>,
}

impl MessageWire {
    fn received_epoch_seconds(&self) -> Option<i64> {
        self.received_date_time
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|received| received.timestamp())
            .filter(|seconds| *seconds >= 0)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecipientWire {
    email_address: Option<EmailAddressWire>,
}

#[derive(Deserialize)]
struct EmailAddressWire {
    name: Option<String>,
    address: Option<String>,
}

impl RecipientWire {
    /// Mirrors a `From` header so both providers present senders the same way.
    fn display(self) -> String {
        let Some(email_address) = self.email_address else {
            return String::new();
        };
        let name = email_address.name.unwrap_or_default();
        let address = email_address.address.unwrap_or_default();
        let rendered = match (name.trim().is_empty(), address.trim().is_empty()) {
            (false, false) if name.trim() != address.trim() => {
                format!("{} <{}>", name.trim(), address.trim())
            }
            (true, false) | (false, false) => address.trim().to_owned(),
            (false, true) => name.trim().to_owned(),
            (true, true) => String::new(),
        };
        bounded_header(&rendered)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserWire {
    mail: Option<String>,
    user_principal_name: Option<String>,
}

impl UserWire {
    fn email(self) -> Option<String> {
        [self.mail, self.user_principal_name]
            .into_iter()
            .flatten()
            .find(|candidate| valid_email(candidate))
    }
}

#[derive(Deserialize)]
struct TokenWire {
    access_token: SensitiveString,
    refresh_token: Option<SensitiveString>,
    expires_in: u64,
    scope: Option<String>,
    token_type: String,
}

#[derive(Deserialize)]
struct ProviderErrorWire {
    error: String,
}

/// The next cursor is one second past the newest observed message. Graph reports
/// `receivedDateTime` truncated to seconds, so `ge cursor` excludes everything already seen.
fn cursor_after(newest: Option<i64>, current: i64) -> i64 {
    newest
        .map(|seconds| seconds.saturating_add(1))
        .unwrap_or(current)
        .max(current)
}

/// Graph ids are URL-safe base64 with `=` padding. Persisted identifiers allow only
/// alphanumerics, `-`, and `_`, so the padding is dropped for storage and restored for requests.
fn stored_identifier(provider_id: &str) -> Option<String> {
    let stored = provider_id.trim_end_matches('=');
    let valid = !stored.is_empty()
        && stored.len() <= MAX_IDENTIFIER_BYTES
        && stored
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    valid.then(|| stored.to_owned())
}

fn provider_identifier(stored_id: &str) -> String {
    let padding = (4 - stored_id.len() % 4) % 4;
    format!("{stored_id}{}", "=".repeat(padding))
}

fn bounded_header(value: &str) -> String {
    let mut cleaned: String = value
        .chars()
        .filter(|character| !character.is_control())
        .collect();
    if cleaned.len() > MAX_HEADER_BYTES {
        let mut boundary = MAX_HEADER_BYTES;
        while !cleaned.is_char_boundary(boundary) {
            boundary -= 1;
        }
        cleaned.truncate(boundary);
    }
    cleaned
}

fn valid_email(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 320
        || value.trim() != value
        || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return false;
    }
    let mut parts = value.split('@');
    parts.next().is_some_and(|part| !part.is_empty())
        && parts.next().is_some_and(|part| !part.is_empty())
        && parts.next().is_none()
}

/// Entra may return short (`Mail.Read`) or resource-qualified scope names and may add others.
fn grants_mail_read(scope: &str) -> bool {
    scope.split_ascii_whitespace().any(|granted| {
        granted
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("Mail.Read"))
    })
}

fn token_digest(token: &[u8]) -> TokenDigest {
    Sha256::digest(token).into()
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b',') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn map_graph_status(status: StatusCode) -> GmailReadError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => GmailReadError::AuthRequired,
        status if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() => {
            GmailReadError::Unavailable
        }
        _ => GmailReadError::InvalidResponse,
    }
}

fn map_refresh_status(status: StatusCode) -> GmailReadError {
    match status {
        StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            GmailReadError::AuthRequired
        }
        status if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() => {
            GmailReadError::Unavailable
        }
        _ => GmailReadError::InvalidResponse,
    }
}

fn classify_token_status(status: StatusCode, body: &[u8]) -> AddAccountError {
    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        return AddAccountError::from_tag(AddAccountErrorTag::NetworkUnavailable);
    }
    // Only the allowlisted error code is inspected; descriptions and bodies are discarded.
    let tag = serde_json::from_slice::<ProviderErrorWire>(body)
        .ok()
        .map(|wire| match wire.error.as_str() {
            "invalid_client"
            | "unauthorized_client"
            | "invalid_scope"
            | "invalid_request"
            | "unsupported_grant_type"
            | "invalid_resource" => AddAccountErrorTag::ConfigInvalid,
            "access_denied" | "consent_required" | "interaction_required" => {
                AddAccountErrorTag::ProviderDenied
            }
            _ => AddAccountErrorTag::TokenExchangeFailed,
        })
        .unwrap_or(AddAccountErrorTag::TokenExchangeFailed);
    AddAccountError::from_tag(tag)
}

async fn read_graph_bounded(
    mut response: reqwest::Response,
    maximum: usize,
) -> Result<Zeroizing<Vec<u8>>, GmailReadError> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum as u64)
    {
        return Err(GmailReadError::InvalidResponse);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| GmailReadError::Unavailable)?
    {
        if bytes.len().saturating_add(chunk.len()) > maximum {
            return Err(GmailReadError::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
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

fn token_exchange_failed() -> AddAccountError {
    AddAccountError::from_tag(AddAccountErrorTag::TokenExchangeFailed)
}

fn internal() -> AddAccountError {
    AddAccountError::from_tag(AddAccountErrorTag::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_drop_padding_for_storage_and_restore_it_for_requests() {
        for provider_id in [
            "AQMkADAwATY0-_AAAAA=",
            "AQMkADAwATY0-_AAAA==",
            "AQMkADAwATY0-_AAAAAA",
        ] {
            let stored = stored_identifier(provider_id).expect("storable id");
            assert!(!stored.contains('='));
            assert_eq!(provider_identifier(&stored), provider_id);
        }
    }

    #[test]
    fn identifiers_outside_the_persisted_alphabet_or_bound_are_rejected() {
        assert_eq!(stored_identifier(""), None);
        assert_eq!(stored_identifier("===="), None);
        assert_eq!(stored_identifier("abc/def"), None);
        assert_eq!(stored_identifier("abc+def"), None);
        assert_eq!(
            stored_identifier(&"a".repeat(MAX_IDENTIFIER_BYTES + 1)),
            None
        );
        assert!(stored_identifier(&"a".repeat(MAX_IDENTIFIER_BYTES)).is_some());
    }

    #[test]
    fn cursor_moves_one_second_past_the_newest_message_and_never_backwards() {
        assert_eq!(cursor_after(Some(100), 0), 101);
        assert_eq!(cursor_after(None, 101), 101);
        assert_eq!(cursor_after(Some(50), 101), 101);
        assert_eq!(cursor_after(None, 0), 0);
    }

    #[test]
    fn mail_read_grant_accepts_short_and_qualified_names_only() {
        assert!(grants_mail_read("User.Read Mail.Read"));
        assert!(grants_mail_read(
            "https://graph.microsoft.com/Mail.Read openid"
        ));
        assert!(grants_mail_read("mail.read"));
        assert!(!grants_mail_read("User.Read Mail.ReadWrite"));
        assert!(!grants_mail_read(""));
    }

    #[test]
    fn sender_is_rendered_like_a_from_header_without_control_characters() {
        let sender = |name: Option<&str>, address: Option<&str>| {
            RecipientWire {
                email_address: Some(EmailAddressWire {
                    name: name.map(str::to_owned),
                    address: address.map(str::to_owned),
                }),
            }
            .display()
        };
        assert_eq!(
            sender(Some("Ana"), Some("ana@example.com")),
            "Ana <ana@example.com>"
        );
        assert_eq!(sender(None, Some("ana@example.com")), "ana@example.com");
        assert_eq!(
            sender(Some("ana@example.com"), Some("ana@example.com")),
            "ana@example.com"
        );
        assert_eq!(sender(Some("Ana\r\nBcc: x"), None), "AnaBcc: x");
        assert_eq!(
            RecipientWire {
                email_address: None
            }
            .display(),
            ""
        );
    }

    #[test]
    fn graph_queries_use_percent_twenty_and_stay_under_the_me_endpoint() {
        let client = MicrosoftHttpClient::new().expect("client");
        let url = client
            .graph_url(
                &["mailFolders", "inbox", "messages"],
                &[("$filter", "isRead eq false"), ("$select", "id")],
            )
            .expect("url");
        assert_eq!(
            url.as_str(),
            "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages?$filter=isRead%20eq%20false&$select=id"
        );
    }

    #[test]
    fn next_links_must_stay_on_the_graph_origin() {
        let client = MicrosoftHttpClient::new().expect("client");
        assert!(client
            .next_page_url(Some(
                "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages?$skip=10".into()
            ))
            .expect("same origin")
            .is_some());
        assert_eq!(client.next_page_url(None), Ok(None));
        for foreign in [
            "https://graph.microsoft.com.evil.example/v1.0/me/messages",
            "http://graph.microsoft.com/v1.0/me/messages",
            "https://user@graph.microsoft.com/v1.0/me/messages",
            "https://graph.microsoft.com/v1.0/users/other/messages",
        ] {
            assert_eq!(
                client.next_page_url(Some(foreign.into())),
                Err(GmailReadError::InvalidResponse)
            );
        }
    }

    #[test]
    fn mailbox_identity_prefers_mail_and_falls_back_to_the_sign_in_name() {
        let user = |mail: Option<&str>, upn: Option<&str>| UserWire {
            mail: mail.map(str::to_owned),
            user_principal_name: upn.map(str::to_owned),
        };
        assert_eq!(
            user(Some("a@outlook.com"), Some("b@outlook.com"))
                .email()
                .as_deref(),
            Some("a@outlook.com")
        );
        assert_eq!(
            user(None, Some("b@outlook.com")).email().as_deref(),
            Some("b@outlook.com")
        );
        assert_eq!(user(Some("not-an-email"), None).email(), None);
    }
}
