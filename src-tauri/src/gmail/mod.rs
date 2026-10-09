//! Google OAuth token and read-only Gmail HTTP adapter.

#![allow(dead_code)]

use crate::auth::{
    AddAccountError, AddAccountErrorTag, Cancellation, IdentityPort, PortFuture, SensitiveString,
    TokenExchangePort, TokenExchangeRequest, TokenResponse, APPROVED_SCOPES,
};
use reqwest::{redirect::Policy, StatusCode};
use serde::Deserialize;
use std::{collections::HashSet, fmt, future::Future, pin::Pin, time::Duration};
use url::Url;
use zeroize::Zeroizing;

const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const REVOKE_ENDPOINT: &str = "https://oauth2.googleapis.com/revoke";
const GMAIL_PROFILE_ENDPOINT: &str = "https://gmail.googleapis.com/gmail/v1/users/me/profile";
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_TOKEN_RESPONSE_BYTES: usize = 32 * 1024;
const MAX_IDENTITY_RESPONSE_BYTES: usize = 16 * 1024;
// Gmail list/history pages may contain 500 provider records. Keep one response bounded while
// allowing legitimate pages whose identifiers and label events exceed the OAuth-sized limit.
const MAX_GMAIL_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_REVOKE_RESPONSE_BYTES: usize = 8 * 1024;
pub(crate) const MAX_UNREAD_MESSAGE_IDS: usize = 25_000;
const MAX_GMAIL_PAGES: usize = 100;
const MAX_HISTORY_EVENTS: usize = 20_000;
const TOKEN_EXCHANGE_DIAGNOSTIC_PREFIX: &str = "zerochi.oauth.token_exchange";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenExchangeDiagnostic {
    InvalidGrant,
    InvalidClient,
    UnauthorizedClient,
    DeletedClient,
    InvalidScope,
    RedirectUriMismatch,
    InvalidRequestMissingClientSecret,
    InvalidRequestCodeVerifier,
    InvalidRequestRedirectUri,
    InvalidRequestCode,
    InvalidRequestOther,
    UnsupportedGrantType,
    ProviderErrorOther,
    InvalidSuccessResponse,
    Transport,
}

impl TokenExchangeDiagnostic {
    const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidGrant => "invalid_grant",
            Self::InvalidClient => "invalid_client",
            Self::UnauthorizedClient => "unauthorized_client",
            Self::DeletedClient => "deleted_client",
            Self::InvalidScope => "invalid_scope",
            Self::RedirectUriMismatch => "redirect_uri_mismatch",
            Self::InvalidRequestMissingClientSecret => "invalid_request_missing_client_secret",
            Self::InvalidRequestCodeVerifier => "invalid_request_code_verifier",
            Self::InvalidRequestRedirectUri => "invalid_request_redirect_uri",
            Self::InvalidRequestCode => "invalid_request_code",
            Self::InvalidRequestOther => "invalid_request_other",
            Self::UnsupportedGrantType => "unsupported_grant_type",
            Self::ProviderErrorOther => "provider_error_other",
            Self::InvalidSuccessResponse => "invalid_success_response",
            Self::Transport => "transport",
        }
    }
}

#[cfg(debug_assertions)]
fn emit_token_exchange_diagnostic(category: TokenExchangeDiagnostic) {
    eprintln!("{TOKEN_EXCHANGE_DIAGNOSTIC_PREFIX} {}", category.as_str());
}

#[cfg(not(debug_assertions))]
fn emit_token_exchange_diagnostic(_category: TokenExchangeDiagnostic) {}

pub(crate) type GmailFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GmailReadError {
    Unavailable,
    AuthRequired,
    HistoryExpired,
    InvalidResponse,
}

impl fmt::Display for GmailReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Gmail is temporarily unavailable")
    }
}

impl std::error::Error for GmailReadError {}

/// Read-only provider seam used by sync. Tokens remain borrowed Rust-only values.
pub(crate) trait GmailReadPort: Send + Sync {
    fn profile<'a>(
        &'a self,
        access_token: &'a SensitiveString,
    ) -> GmailFuture<'a, Result<GmailProfile, GmailReadError>>;

    fn unread_message_ids<'a>(
        &'a self,
        access_token: &'a SensitiveString,
    ) -> GmailFuture<'a, Result<Vec<String>, GmailReadError>>;

    fn changes_since<'a>(
        &'a self,
        _access_token: &'a SensitiveString,
        _cursor: &'a str,
    ) -> GmailFuture<'a, Result<GmailChanges, GmailReadError>> {
        Box::pin(async { Err(GmailReadError::Unavailable) })
    }

    fn message_metadata<'a>(
        &'a self,
        _access_token: &'a SensitiveString,
        _message_id: &'a str,
    ) -> GmailFuture<'a, Result<Option<GmailMessageMetadata>, GmailReadError>> {
        Box::pin(async { Err(GmailReadError::Unavailable) })
    }
}

pub(crate) trait AccessTokenRefreshPort: Send + Sync {
    fn refresh_access_token<'a>(
        &'a self,
        client_id: &'a crate::auth::ClientId,
        client_secret: &'a crate::auth::ClientSecret,
        refresh_token: &'a [u8],
    ) -> GmailFuture<'a, Result<SensitiveString, GmailReadError>>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TokenRevokeError;

impl fmt::Display for TokenRevokeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Token revocation is unavailable")
    }
}

impl std::error::Error for TokenRevokeError {}

pub(crate) trait TokenRevokePort: Send + Sync {
    fn revoke<'a>(
        &'a self,
        refresh_token: &'a [u8],
    ) -> GmailFuture<'a, Result<(), TokenRevokeError>>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GmailMessageMetadata {
    pub(crate) id: String,
    pub(crate) thread_id: String,
    pub(crate) sender: String,
    pub(crate) subject: String,
    pub(crate) received_at: String,
    pub(crate) is_inbox_unread: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GmailProfile {
    pub(crate) email: String,
    pub(crate) history_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GmailHistoryEvent {
    MessageAdded {
        id: String,
        thread_id: String,
        label_ids: Vec<String>,
    },
    MessageDeleted {
        id: String,
    },
    LabelsAdded {
        id: String,
        label_ids: Vec<String>,
    },
    LabelsRemoved {
        id: String,
        label_ids: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GmailChanges {
    pub(crate) cursor: String,
    pub(crate) events: Vec<GmailHistoryEvent>,
}

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
    gmail_profile_endpoint: Url,
    revoke_endpoint: Url,
}

impl GoogleHttpClient {
    pub(crate) fn new() -> Result<Self, AddAccountError> {
        Self::with_provider_endpoints(
            TOKEN_ENDPOINT,
            GMAIL_PROFILE_ENDPOINT,
            GMAIL_PROFILE_ENDPOINT,
            REVOKE_ENDPOINT,
        )
    }

    fn with_endpoints(token: &str, identity: &str) -> Result<Self, AddAccountError> {
        Self::with_all_endpoints(token, identity, identity)
    }

    fn with_all_endpoints(
        token: &str,
        identity: &str,
        gmail_profile: &str,
    ) -> Result<Self, AddAccountError> {
        Self::with_provider_endpoints(token, identity, gmail_profile, token)
    }

    fn with_provider_endpoints(
        token: &str,
        identity: &str,
        gmail_profile: &str,
        revoke: &str,
    ) -> Result<Self, AddAccountError> {
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
        let gmail_profile_endpoint = Url::parse(gmail_profile).map_err(|_| internal())?;
        let revoke_endpoint = Url::parse(revoke).map_err(|_| internal())?;
        if !cfg!(test)
            && (token_endpoint.scheme() != "https"
                || identity_endpoint.scheme() != "https"
                || gmail_profile_endpoint.scheme() != "https"
                || revoke_endpoint.scheme() != "https")
        {
            return Err(internal());
        }
        Ok(Self {
            client,
            token_endpoint,
            identity_endpoint,
            gmail_profile_endpoint,
            revoke_endpoint,
        })
    }
}

impl GmailReadPort for GoogleHttpClient {
    fn profile<'a>(
        &'a self,
        access_token: &'a SensitiveString,
    ) -> GmailFuture<'a, Result<GmailProfile, GmailReadError>> {
        Box::pin(async move {
            let response = self
                .client
                .get(self.gmail_profile_endpoint.clone())
                .bearer_auth(access_token.expose())
                .send()
                .await
                .map_err(|_| GmailReadError::Unavailable)?;
            if !response.status().is_success() {
                return Err(map_gmail_status(response.status()));
            }
            let bytes = read_gmail_bounded(response).await?;
            let profile: GmailProfileWire =
                serde_json::from_slice(&bytes).map_err(|_| GmailReadError::InvalidResponse)?;
            if invalid_history_id(&profile.history_id) || !valid_email(&profile.email_address) {
                return Err(GmailReadError::InvalidResponse);
            }
            Ok(GmailProfile {
                email: profile.email_address,
                history_id: profile.history_id,
            })
        })
    }

    fn unread_message_ids<'a>(
        &'a self,
        access_token: &'a SensitiveString,
    ) -> GmailFuture<'a, Result<Vec<String>, GmailReadError>> {
        Box::pin(async move {
            let mut page_token: Option<String> = None;
            let mut ids = Vec::new();
            let mut seen = HashSet::new();
            for _ in 0..MAX_GMAIL_PAGES {
                let mut url = gmail_sibling_url(&self.gmail_profile_endpoint, "messages")?;
                {
                    let mut query = url.query_pairs_mut();
                    query
                        .append_pair("q", "is:unread")
                        .append_pair("labelIds", "INBOX")
                        .append_pair("maxResults", "500");
                    if let Some(token) = &page_token {
                        query.append_pair("pageToken", token);
                    }
                }
                let response = self
                    .client
                    .get(url)
                    .bearer_auth(access_token.expose())
                    .send()
                    .await
                    .map_err(|_| GmailReadError::Unavailable)?;
                if !response.status().is_success() {
                    return Err(map_gmail_status(response.status()));
                }
                let bytes = read_gmail_bounded(response).await?;
                let page: MessageListPageWire =
                    serde_json::from_slice(&bytes).map_err(|_| GmailReadError::InvalidResponse)?;
                for reference in page.messages.unwrap_or_default() {
                    if invalid_message_id(&reference.id) || invalid_message_id(&reference.thread_id)
                    {
                        return Err(GmailReadError::InvalidResponse);
                    }
                    if seen.insert(reference.id.clone()) {
                        if ids.len() >= MAX_UNREAD_MESSAGE_IDS {
                            return Err(GmailReadError::InvalidResponse);
                        }
                        ids.push(reference.id);
                    }
                }
                page_token = validated_next_page_token(page.next_page_token)?;
                if page_token.is_none() {
                    return Ok(ids);
                }
            }
            Err(GmailReadError::InvalidResponse)
        })
    }

    fn changes_since<'a>(
        &'a self,
        access_token: &'a SensitiveString,
        cursor: &'a str,
    ) -> GmailFuture<'a, Result<GmailChanges, GmailReadError>> {
        Box::pin(async move {
            if invalid_history_id(cursor) {
                return Err(GmailReadError::InvalidResponse);
            }
            let mut page_token: Option<String> = None;
            let mut events = Vec::new();
            for _ in 0..MAX_GMAIL_PAGES {
                let mut url = gmail_sibling_url(&self.gmail_profile_endpoint, "history")?;
                {
                    let mut query = url.query_pairs_mut();
                    query
                        .append_pair("startHistoryId", cursor)
                        .append_pair("maxResults", "500");
                    if let Some(token) = &page_token {
                        query.append_pair("pageToken", token);
                    }
                }
                let response = self
                    .client
                    .get(url)
                    .bearer_auth(access_token.expose())
                    .send()
                    .await
                    .map_err(|_| GmailReadError::Unavailable)?;
                if !response.status().is_success() {
                    return Err(map_gmail_status(response.status()));
                }
                let bytes = read_gmail_bounded(response).await?;
                let page: HistoryPageWire =
                    serde_json::from_slice(&bytes).map_err(|_| GmailReadError::InvalidResponse)?;
                if invalid_history_id(&page.history_id) {
                    return Err(GmailReadError::InvalidResponse);
                }
                let next_cursor = page.history_id;
                for record in page.history.unwrap_or_default() {
                    for added in record.messages_added.unwrap_or_default() {
                        let reference = added.message;
                        validate_reference(&reference)?;
                        push_history_event(
                            &mut events,
                            GmailHistoryEvent::MessageAdded {
                                id: reference.id,
                                thread_id: reference.thread_id,
                                label_ids: validate_label_ids(reference.label_ids)?,
                            },
                        )?;
                    }
                    for added in record.labels_added.unwrap_or_default() {
                        validate_reference(&added.message)?;
                        push_history_event(
                            &mut events,
                            GmailHistoryEvent::LabelsAdded {
                                id: added.message.id,
                                label_ids: validate_label_ids(added.label_ids)?,
                            },
                        )?;
                    }
                    for removed in record.labels_removed.unwrap_or_default() {
                        validate_reference(&removed.message)?;
                        push_history_event(
                            &mut events,
                            GmailHistoryEvent::LabelsRemoved {
                                id: removed.message.id,
                                label_ids: validate_label_ids(removed.label_ids)?,
                            },
                        )?;
                    }
                    // Deletion wins over every label observation in the same history record.
                    for deleted in record.messages_deleted.unwrap_or_default() {
                        validate_reference(&deleted.message)?;
                        push_history_event(
                            &mut events,
                            GmailHistoryEvent::MessageDeleted {
                                id: deleted.message.id,
                            },
                        )?;
                    }
                }
                page_token = validated_next_page_token(page.next_page_token)?;
                if page_token.is_none() {
                    return Ok(GmailChanges {
                        cursor: next_cursor,
                        events,
                    });
                }
            }
            Err(GmailReadError::InvalidResponse)
        })
    }

    fn message_metadata<'a>(
        &'a self,
        access_token: &'a SensitiveString,
        message_id: &'a str,
    ) -> GmailFuture<'a, Result<Option<GmailMessageMetadata>, GmailReadError>> {
        Box::pin(async move { self.fetch_message_metadata(access_token, message_id).await })
    }
}

impl GoogleHttpClient {
    async fn fetch_message_metadata(
        &self,
        access_token: &SensitiveString,
        message_id: &str,
    ) -> Result<Option<GmailMessageMetadata>, GmailReadError> {
        let mut url = gmail_sibling_url(&self.gmail_profile_endpoint, "messages")?;
        url.path_segments_mut()
            .map_err(|_| GmailReadError::InvalidResponse)?
            .push(message_id);
        url.query_pairs_mut()
            .append_pair("format", "metadata")
            .append_pair("metadataHeaders", "From")
            .append_pair("metadataHeaders", "To")
            .append_pair("metadataHeaders", "Subject");
        let response = self
            .client
            .get(url)
            .bearer_auth(access_token.expose())
            .send()
            .await
            .map_err(|_| GmailReadError::Unavailable)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(map_gmail_status(response.status()));
        }
        let bytes = read_gmail_bounded(response).await?;
        let wire: MessageWire =
            serde_json::from_slice(&bytes).map_err(|_| GmailReadError::InvalidResponse)?;
        validate_message_wire(wire).map(Some)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GmailProfileWire {
    email_address: String,
    history_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageListPageWire {
    messages: Option<Vec<MessageReferenceWire>>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryPageWire {
    history: Option<Vec<HistoryRecordWire>>,
    next_page_token: Option<String>,
    history_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryRecordWire {
    messages_added: Option<Vec<MessageAddedWire>>,
    messages_deleted: Option<Vec<MessageDeletedWire>>,
    labels_added: Option<Vec<LabelsChangedWire>>,
    labels_removed: Option<Vec<LabelsChangedWire>>,
}

#[derive(Deserialize)]
struct MessageAddedWire {
    message: MessageReferenceWire,
}

#[derive(Deserialize)]
struct MessageDeletedWire {
    message: MessageReferenceWire,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LabelsChangedWire {
    message: MessageReferenceWire,
    label_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageReferenceWire {
    id: String,
    thread_id: String,
    #[serde(default)]
    label_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageWire {
    id: String,
    thread_id: String,
    internal_date: String,
    #[serde(default)]
    label_ids: Vec<String>,
    payload: MessagePayloadWire,
}

#[derive(Deserialize)]
struct MessagePayloadWire {
    headers: Vec<MessageHeaderWire>,
}

#[derive(Deserialize)]
struct MessageHeaderWire {
    name: String,
    value: String,
}

async fn read_gmail_bounded(
    mut response: reqwest::Response,
) -> Result<Zeroizing<Vec<u8>>, GmailReadError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_GMAIL_RESPONSE_BYTES as u64)
    {
        return Err(GmailReadError::InvalidResponse);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| GmailReadError::Unavailable)?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_GMAIL_RESPONSE_BYTES {
            return Err(GmailReadError::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn read_revoke_bounded(mut response: reqwest::Response) -> Result<(), TokenRevokeError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_REVOKE_RESPONSE_BYTES as u64)
    {
        return Err(TokenRevokeError);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    while let Some(chunk) = response.chunk().await.map_err(|_| TokenRevokeError)? {
        if bytes.len().saturating_add(chunk.len()) > MAX_REVOKE_RESPONSE_BYTES {
            return Err(TokenRevokeError);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(())
}

fn invalid_history_id(value: &str) -> bool {
    value.is_empty() || value.len() > 256 || !value.bytes().all(|byte| byte.is_ascii_digit())
}

fn invalid_message_id(value: &str) -> bool {
    value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_email(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed != value {
        return false;
    }
    let value = trimmed;
    if value.is_empty() || value.len() > 320 || value.bytes().any(|byte| byte.is_ascii_control()) {
        return false;
    }
    let mut parts = value.split('@');
    parts.next().is_some_and(|part| !part.is_empty())
        && parts.next().is_some_and(|part| !part.is_empty())
        && parts.next().is_none()
}

fn validate_reference(reference: &MessageReferenceWire) -> Result<(), GmailReadError> {
    if invalid_message_id(&reference.id) || invalid_message_id(&reference.thread_id) {
        Err(GmailReadError::InvalidResponse)
    } else {
        Ok(())
    }
}

fn validate_label_ids(label_ids: Vec<String>) -> Result<Vec<String>, GmailReadError> {
    if label_ids.len() > 256
        || label_ids.iter().any(|label| {
            label.is_empty()
                || label.len() > 256
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
    {
        return Err(GmailReadError::InvalidResponse);
    }
    Ok(label_ids)
}

fn push_history_event(
    events: &mut Vec<GmailHistoryEvent>,
    event: GmailHistoryEvent,
) -> Result<(), GmailReadError> {
    if events.len() >= MAX_HISTORY_EVENTS {
        return Err(GmailReadError::InvalidResponse);
    }
    events.push(event);
    Ok(())
}

fn validated_next_page_token(token: Option<String>) -> Result<Option<String>, GmailReadError> {
    match token {
        Some(token) if !token.is_empty() && token.len() <= 1_024 => Ok(Some(token)),
        Some(_) => Err(GmailReadError::InvalidResponse),
        None => Ok(None),
    }
}

fn gmail_sibling_url(profile: &Url, sibling: &str) -> Result<Url, GmailReadError> {
    let mut url = profile.clone();
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| GmailReadError::InvalidResponse)?;
    segments.pop_if_empty().pop().push(sibling);
    drop(segments);
    url.set_query(None);
    Ok(url)
}

fn map_gmail_status(status: StatusCode) -> GmailReadError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => GmailReadError::AuthRequired,
        StatusCode::NOT_FOUND => GmailReadError::HistoryExpired,
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

fn validate_message_wire(wire: MessageWire) -> Result<GmailMessageMetadata, GmailReadError> {
    if invalid_message_id(&wire.id) || invalid_message_id(&wire.thread_id) {
        return Err(GmailReadError::InvalidResponse);
    }
    let millis = wire
        .internal_date
        .parse::<i64>()
        .map_err(|_| GmailReadError::InvalidResponse)?;
    let received_at = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis)
        .ok_or(GmailReadError::InvalidResponse)?
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let label_ids = validate_label_ids(wire.label_ids)?;
    let is_inbox_unread = label_ids.iter().any(|label| label == "INBOX")
        && label_ids.iter().any(|label| label == "UNREAD");
    let mut sender = None;
    let mut subject = None;
    for header in wire.payload.headers {
        if header.value.len() > 2_048 || header.value.chars().any(char::is_control) {
            return Err(GmailReadError::InvalidResponse);
        }
        if header.name.eq_ignore_ascii_case("from") {
            if sender.replace(header.value).is_some() {
                return Err(GmailReadError::InvalidResponse);
            }
        } else if header.name.eq_ignore_ascii_case("subject")
            && subject.replace(header.value).is_some()
        {
            return Err(GmailReadError::InvalidResponse);
        }
    }
    Ok(GmailMessageMetadata {
        id: wire.id,
        thread_id: wire.thread_id,
        sender: sender.unwrap_or_default(),
        subject: subject.unwrap_or_default(),
        received_at,
        is_inbox_unread,
    })
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
                    .append_pair("client_secret", request.client_secret.expose())
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
                .map_err(map_token_exchange_transport)?;
            cancellation.ensure_active()?;
            let status = response.status();
            let bytes = read_bounded(
                response,
                MAX_TOKEN_RESPONSE_BYTES,
                AddAccountErrorTag::TokenExchangeFailed,
            )
            .await
            .inspect_err(|_| {
                emit_token_exchange_diagnostic(TokenExchangeDiagnostic::Transport);
            })?;

            if !status.is_success() {
                return Err(classify_token_status(status, &bytes));
            }
            let wire: TokenWire = serde_json::from_slice(&bytes).map_err(|_| {
                emit_token_exchange_diagnostic(TokenExchangeDiagnostic::InvalidSuccessResponse);
                AddAccountError::from_tag(AddAccountErrorTag::TokenExchangeFailed)
            })?;
            validate_scopes(&wire.scope).inspect_err(|_| {
                emit_token_exchange_diagnostic(TokenExchangeDiagnostic::InvalidSuccessResponse);
            })?;
            if wire.access_token.is_empty()
                || wire.expires_in == 0
                || !wire.token_type.eq_ignore_ascii_case("bearer")
                || wire
                    .refresh_token
                    .as_ref()
                    .is_some_and(SensitiveString::is_empty)
            {
                emit_token_exchange_diagnostic(TokenExchangeDiagnostic::InvalidSuccessResponse);
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

impl AccessTokenRefreshPort for GoogleHttpClient {
    fn refresh_access_token<'a>(
        &'a self,
        client_id: &'a crate::auth::ClientId,
        client_secret: &'a crate::auth::ClientSecret,
        refresh_token: &'a [u8],
    ) -> GmailFuture<'a, Result<SensitiveString, GmailReadError>> {
        Box::pin(async move {
            let refresh_token =
                std::str::from_utf8(refresh_token).map_err(|_| GmailReadError::AuthRequired)?;
            if refresh_token.is_empty() || refresh_token.len() > 8_192 {
                return Err(GmailReadError::AuthRequired);
            }
            let body = SensitiveString::new(
                url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("client_id", client_id.as_str())
                    .append_pair("client_secret", client_secret.expose())
                    .append_pair("refresh_token", refresh_token)
                    .append_pair("grant_type", "refresh_token")
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
            let bytes = read_gmail_bounded(response).await?;
            let wire: RefreshWire =
                serde_json::from_slice(&bytes).map_err(|_| GmailReadError::InvalidResponse)?;
            if wire.access_token.is_empty()
                || wire.expires_in == 0
                || !wire.token_type.eq_ignore_ascii_case("bearer")
            {
                return Err(GmailReadError::InvalidResponse);
            }
            if let Some(scope) = wire.scope {
                validate_scopes(&scope).map_err(|_| GmailReadError::InvalidResponse)?;
            }
            Ok(wire.access_token)
        })
    }
}

impl TokenRevokePort for GoogleHttpClient {
    fn revoke<'a>(
        &'a self,
        refresh_token: &'a [u8],
    ) -> GmailFuture<'a, Result<(), TokenRevokeError>> {
        Box::pin(async move {
            let Ok(refresh_token) = std::str::from_utf8(refresh_token) else {
                return Ok(());
            };
            if refresh_token.is_empty() || refresh_token.len() > 8_192 {
                return Ok(());
            }
            let body = SensitiveString::new(
                url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("token", refresh_token)
                    .finish(),
            );
            let response = self
                .client
                .post(self.revoke_endpoint.clone())
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body.expose().to_owned())
                .send()
                .await
                .map_err(|_| TokenRevokeError)?;
            let status = response.status();
            read_revoke_bounded(response).await?;
            if status.is_success()
                || matches!(
                    status,
                    StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED | StatusCode::NOT_FOUND
                )
            {
                Ok(())
            } else {
                Err(TokenRevokeError)
            }
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
            let wire: GmailProfileWire = serde_json::from_slice(&bytes)
                .map_err(|_| AddAccountError::from_tag(AddAccountErrorTag::IdentityInvalid))?;
            if invalid_history_id(&wire.history_id) || !valid_email(&wire.email_address) {
                return Err(AddAccountError::from_tag(
                    AddAccountErrorTag::IdentityInvalid,
                ));
            }
            Ok(GoogleIdentity {
                email: wire.email_address,
                display_name: None,
            })
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
struct RefreshWire {
    access_token: SensitiveString,
    expires_in: u64,
    scope: Option<String>,
    token_type: String,
}

#[derive(Deserialize)]
struct ProviderErrorWire {
    error: String,
    #[serde(default)]
    error_description: Option<SensitiveString>,
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
    let mut unique = HashSet::new();
    for granted in scope.split_ascii_whitespace() {
        let canonical = match granted {
            "https://www.googleapis.com/auth/gmail.readonly" => {
                "https://www.googleapis.com/auth/gmail.readonly"
            }
            _ => return Err(token_exchange_failed()),
        };
        if !unique.insert(canonical) {
            return Err(token_exchange_failed());
        }
    }
    let approved: HashSet<_> = APPROVED_SCOPES.split_ascii_whitespace().collect();
    if unique != approved {
        return Err(token_exchange_failed());
    }
    Ok(())
}

fn token_exchange_failed() -> AddAccountError {
    AddAccountError::from_tag(AddAccountErrorTag::TokenExchangeFailed)
}

fn classify_token_status(status: StatusCode, body: &[u8]) -> AddAccountError {
    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        emit_token_exchange_diagnostic(TokenExchangeDiagnostic::ProviderErrorOther);
        return AddAccountError::from_tag(AddAccountErrorTag::NetworkUnavailable);
    }
    let (tag, diagnostic) = serde_json::from_slice::<ProviderErrorWire>(body)
        .ok()
        .map(|wire| {
            classify_provider_error_code(
                &wire.error,
                wire.error_description.as_ref().map(SensitiveString::expose),
            )
        })
        .unwrap_or((
            AddAccountErrorTag::TokenExchangeFailed,
            TokenExchangeDiagnostic::ProviderErrorOther,
        ));
    emit_token_exchange_diagnostic(diagnostic);
    AddAccountError::from_tag(tag)
}

fn classify_provider_error_code(
    error: &str,
    description: Option<&str>,
) -> (AddAccountErrorTag, TokenExchangeDiagnostic) {
    match error {
        "invalid_client" => (
            AddAccountErrorTag::ConfigInvalid,
            TokenExchangeDiagnostic::InvalidClient,
        ),
        "unauthorized_client" => (
            AddAccountErrorTag::ConfigInvalid,
            TokenExchangeDiagnostic::UnauthorizedClient,
        ),
        "deleted_client" => (
            AddAccountErrorTag::ConfigInvalid,
            TokenExchangeDiagnostic::DeletedClient,
        ),
        "invalid_scope" => (
            AddAccountErrorTag::ConfigInvalid,
            TokenExchangeDiagnostic::InvalidScope,
        ),
        "redirect_uri_mismatch" => (
            AddAccountErrorTag::ConfigInvalid,
            TokenExchangeDiagnostic::RedirectUriMismatch,
        ),
        "invalid_request" => (
            AddAccountErrorTag::ConfigInvalid,
            classify_invalid_request_description(description),
        ),
        "unsupported_grant_type" => (
            AddAccountErrorTag::ConfigInvalid,
            TokenExchangeDiagnostic::UnsupportedGrantType,
        ),
        "admin_policy_enforced" | "org_internal" | "restricted_client" => (
            AddAccountErrorTag::WorkspaceRestricted,
            TokenExchangeDiagnostic::ProviderErrorOther,
        ),
        "access_denied" => (
            AddAccountErrorTag::ProviderDenied,
            TokenExchangeDiagnostic::ProviderErrorOther,
        ),
        "invalid_grant" => (
            AddAccountErrorTag::TokenExchangeFailed,
            TokenExchangeDiagnostic::InvalidGrant,
        ),
        _ => (
            AddAccountErrorTag::TokenExchangeFailed,
            TokenExchangeDiagnostic::ProviderErrorOther,
        ),
    }
}

fn classify_invalid_request_description(description: Option<&str>) -> TokenExchangeDiagnostic {
    let Some(description) = description else {
        return TokenExchangeDiagnostic::InvalidRequestOther;
    };
    let normalized = Zeroizing::new(description.to_ascii_lowercase());
    if normalized.contains("client_secret")
        && (normalized.contains("missing") || normalized.contains("required"))
    {
        TokenExchangeDiagnostic::InvalidRequestMissingClientSecret
    } else if normalized.contains("code_verifier") {
        TokenExchangeDiagnostic::InvalidRequestCodeVerifier
    } else if normalized.contains("redirect_uri") {
        TokenExchangeDiagnostic::InvalidRequestRedirectUri
    } else if [
        "parameter: code",
        "parameter 'code'",
        "parameter \"code\"",
        "parameter `code`",
        "parameter code",
        "missing code",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
    {
        TokenExchangeDiagnostic::InvalidRequestCode
    } else {
        TokenExchangeDiagnostic::InvalidRequestOther
    }
}

fn map_token_exchange_transport(error: reqwest::Error) -> AddAccountError {
    emit_token_exchange_diagnostic(TokenExchangeDiagnostic::Transport);
    map_transport(error, AddAccountErrorTag::TokenExchangeFailed)
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
    use crate::auth::{ClientId, ClientSecret, Environment, SensitiveString};
    use std::{
        ffi::OsString,
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
    };

    const CLIENT_ID: &str =
        "123456789012-abcdefghijklmnopqrstuvwxyz012345.apps.googleusercontent.com";
    const SCOPES_JSON: &str = "https://www.googleapis.com/auth/gmail.readonly";

    struct Env;

    impl Environment for Env {
        fn var_os(&self, _name: &str) -> Option<OsString> {
            Some(CLIENT_ID.into())
        }
    }

    fn client_secret() -> ClientSecret {
        struct SecretEnv;
        impl Environment for SecretEnv {
            fn var_os(&self, _name: &str) -> Option<OsString> {
                Some("fixture-client-secret-value".into())
            }
        }
        ClientSecret::load_from(&SecretEnv).expect("client secret")
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

    fn mock_server_sequence(
        responses: Vec<(String, &'static str)>,
    ) -> (String, mpsc::Receiver<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let address = listener.local_addr().expect("mock address");
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut captured = Vec::new();
            for (response_body, status) in responses {
                let (mut stream, _) = listener.accept().expect("accept mock request");
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .expect("read timeout");
                let mut bytes = vec![0; 16 * 1024];
                let mut used = 0;
                loop {
                    let read = stream.read(&mut bytes[used..]).expect("read request");
                    used += read;
                    if bytes[..used].windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                captured.push(String::from_utf8_lossy(&bytes[..used]).into_owned());
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
                    response_body.len()
                )
                .expect("write mock response");
            }
            let _ = sender.send(captured);
        });
        (format!("http://{address}/profile"), receiver)
    }

    fn cancellation() -> (crate::auth::AddAccountAttemptGuard, Cancellation) {
        let attempts = crate::auth::AddAccountAttemptGuardSet::new();
        let guard = attempts.begin().expect("attempt");
        let cancellation = guard.cancellation();
        (guard, cancellation)
    }

    #[test]
    fn production_provider_source_excludes_forbidden_google_services_and_content_requests() {
        let source = include_str!("mod.rs")
            .rsplit_once("#[cfg(test)]\nmod tests")
            .expect("test module boundary")
            .0;
        assert!(source.contains("https://oauth2.googleapis.com/token"));
        assert!(source.contains("https://gmail.googleapis.com/gmail/v1/users/me/profile"));
        for forbidden in [
            "openidconnect.googleapis.com",
            "userinfo.email",
            "userinfo.profile",
            "historyTypes",
            "format\", \"full",
            "format\", \"raw",
            "attachments",
            "snippet",
            "messages_unread",
        ] {
            assert!(
                !source.contains(forbidden),
                "forbidden provider source: {forbidden}"
            );
        }
    }

    #[test]
    fn exchanges_code_with_exact_redacted_client_credential_form() {
        let body = format!(
            "{{\"access_token\":\"fixture-access\",\"refresh_token\":\"fixture-refresh\",\"expires_in\":3599,\"scope\":\"{SCOPES_JSON}\",\"token_type\":\"Bearer\"}}"
        );
        let (endpoint, captured) = mock_server(body, "200 OK");
        let client = GoogleHttpClient::with_endpoints(&endpoint, &endpoint).expect("client");
        let client_id = ClientId::load_from(&Env).expect("client id");
        let client_secret = client_secret();
        let code = SensitiveString::new("fixture-code".into());
        let (_guard, cancellation) = cancellation();
        let response = tauri::async_runtime::block_on(client.exchange(
            TokenExchangeRequest {
                client_id: &client_id,
                client_secret: &client_secret,
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
        let encoded_body = request
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .expect("form body");
        let fields: Vec<_> = url::form_urlencoded::parse(encoded_body.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(
            fields,
            vec![
                ("client_id".to_owned(), CLIENT_ID.to_owned()),
                (
                    "client_secret".to_owned(),
                    "fixture-client-secret-value".to_owned(),
                ),
                ("code".to_owned(), "fixture-code".to_owned()),
                ("code_verifier".to_owned(), "fixture-verifier".to_owned()),
                ("grant_type".to_owned(), "authorization_code".to_owned()),
                (
                    "redirect_uri".to_owned(),
                    "http://127.0.0.1:49152/oauth/callback".to_owned(),
                ),
            ]
        );
        assert_eq!(format!("{client_secret:?}"), "ClientSecret([REDACTED])");
    }

    #[test]
    fn accepts_only_the_exact_gmail_readonly_scope() {
        validate_scopes(SCOPES_JSON).expect("approved exact scope");
    }

    #[test]
    fn rejects_duplicate_missing_extra_or_broader_scope_forms() {
        for granted in [
            "openid email profile https://www.googleapis.com/auth/gmail.readonly",
            "openid email https://www.googleapis.com/auth/userinfo.email profile https://www.googleapis.com/auth/gmail.readonly",
            "openid profile https://www.googleapis.com/auth/gmail.readonly",
            "openid email profile https://mail.google.com/",
            "openid email profile https://www.googleapis.com/auth/gmail.modify",
            "openid email profile https://www.googleapis.com/auth/gmail.readonly extra",
            "",
        ] {
            let error = validate_scopes(granted).unwrap_err();
            assert_eq!(error.tag(), AddAccountErrorTag::TokenExchangeFailed);
            if !granted.is_empty() {
                assert!(!format!("{error:?}").contains(granted));
            }
        }
    }

    #[test]
    fn token_error_allowlist_maps_configuration_and_retry_metadata_without_leakage() {
        for provider_error in [
            "invalid_client",
            "unauthorized_client",
            "deleted_client",
            "invalid_scope",
            "redirect_uri_mismatch",
            "invalid_request",
            "unsupported_grant_type",
        ] {
            let body = format!(
                "{{\"error\":\"{provider_error}\",\"error_description\":\"provider diagnostic fixture-secret\"}}"
            );
            let error = classify_token_status(StatusCode::BAD_REQUEST, body.as_bytes());
            assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
            let value = serde_json::to_value(&error).expect("serialize error");
            assert_eq!(value["retryable"], false);
            assert_eq!(value["actionRequired"], true);
            let rendered = format!("{error:?} {error}");
            assert!(!rendered.contains(provider_error));
            assert!(!rendered.contains("provider diagnostic"));
            assert!(!rendered.contains("fixture-secret"));
        }

        let invalid_grant = classify_token_status(
            StatusCode::BAD_REQUEST,
            br#"{"error":"invalid_grant","error_description":"expired fixture-auth-code"}"#,
        );
        assert_eq!(invalid_grant.tag(), AddAccountErrorTag::TokenExchangeFailed);
        let value = serde_json::to_value(&invalid_grant).expect("serialize invalid grant");
        assert_eq!(value["retryable"], true);
        assert_eq!(value["actionRequired"], false);
        assert_eq!(
            value["message"],
            "Google could not complete account authorization. Restart authorization and try again."
        );
        assert!(!format!("{invalid_grant:?}").contains("fixture-auth-code"));

        let malformed = classify_token_status(StatusCode::OK, b"malformed fixture-secret");
        assert_eq!(malformed.tag(), AddAccountErrorTag::TokenExchangeFailed);
        assert!(!format!("{malformed:?}").contains("fixture-secret"));
    }

    #[test]
    fn debug_diagnostic_categories_are_closed_static_and_never_provider_controlled() {
        assert_eq!(
            TOKEN_EXCHANGE_DIAGNOSTIC_PREFIX,
            "zerochi.oauth.token_exchange"
        );
        let cases = [
            (
                "invalid_grant",
                AddAccountErrorTag::TokenExchangeFailed,
                TokenExchangeDiagnostic::InvalidGrant,
                "invalid_grant",
            ),
            (
                "invalid_client",
                AddAccountErrorTag::ConfigInvalid,
                TokenExchangeDiagnostic::InvalidClient,
                "invalid_client",
            ),
            (
                "unauthorized_client",
                AddAccountErrorTag::ConfigInvalid,
                TokenExchangeDiagnostic::UnauthorizedClient,
                "unauthorized_client",
            ),
            (
                "deleted_client",
                AddAccountErrorTag::ConfigInvalid,
                TokenExchangeDiagnostic::DeletedClient,
                "deleted_client",
            ),
            (
                "invalid_scope",
                AddAccountErrorTag::ConfigInvalid,
                TokenExchangeDiagnostic::InvalidScope,
                "invalid_scope",
            ),
            (
                "redirect_uri_mismatch",
                AddAccountErrorTag::ConfigInvalid,
                TokenExchangeDiagnostic::RedirectUriMismatch,
                "redirect_uri_mismatch",
            ),
            (
                "invalid_request",
                AddAccountErrorTag::ConfigInvalid,
                TokenExchangeDiagnostic::InvalidRequestOther,
                "invalid_request_other",
            ),
            (
                "unsupported_grant_type",
                AddAccountErrorTag::ConfigInvalid,
                TokenExchangeDiagnostic::UnsupportedGrantType,
                "unsupported_grant_type",
            ),
            (
                "arbitrary-provider-text fixture-secret someone@example.test",
                AddAccountErrorTag::TokenExchangeFailed,
                TokenExchangeDiagnostic::ProviderErrorOther,
                "provider_error_other",
            ),
        ];
        for (provider_code, expected_tag, expected_category, expected_static) in cases {
            let (tag, category) = classify_provider_error_code(provider_code, None);
            assert_eq!(tag, expected_tag);
            assert_eq!(category, expected_category);
            assert_eq!(category.as_str(), expected_static);
            if category == TokenExchangeDiagnostic::ProviderErrorOther {
                assert!(!category.as_str().contains(provider_code));
                assert!(!category.as_str().contains("fixture-secret"));
                assert!(!category.as_str().contains("someone@example.test"));
            }
        }

        assert_eq!(
            TokenExchangeDiagnostic::InvalidSuccessResponse.as_str(),
            "invalid_success_response"
        );
        assert_eq!(TokenExchangeDiagnostic::Transport.as_str(), "transport");
    }

    #[test]
    fn invalid_request_description_collapses_parameter_names_to_static_categories() {
        let cases = [
            (
                "Missing required parameter: CLIENT_SECRET canary-secret-value",
                TokenExchangeDiagnostic::InvalidRequestMissingClientSecret,
                "invalid_request_missing_client_secret",
            ),
            (
                "Invalid CODE_VERIFIER canary-verifier-value",
                TokenExchangeDiagnostic::InvalidRequestCodeVerifier,
                "invalid_request_code_verifier",
            ),
            (
                "Invalid Redirect_URI https://canary.example/callback?code=secret",
                TokenExchangeDiagnostic::InvalidRequestRedirectUri,
                "invalid_request_redirect_uri",
            ),
            (
                "Missing required parameter: code canary-auth-code-value",
                TokenExchangeDiagnostic::InvalidRequestCode,
                "invalid_request_code",
            ),
            (
                "Provider canary description without an approved parameter",
                TokenExchangeDiagnostic::InvalidRequestOther,
                "invalid_request_other",
            ),
        ];

        for (description, expected, static_name) in cases {
            let (tag, category) =
                classify_provider_error_code("invalid_request", Some(description));
            assert_eq!(tag, AddAccountErrorTag::ConfigInvalid);
            assert_eq!(category, expected);
            assert_eq!(category.as_str(), static_name);
            assert!(!category.as_str().contains("canary"));
            assert!(!category.as_str().contains("secret-value"));

            let body = format!(
                "{{\"error\":\"invalid_request\",\"error_description\":{}}}",
                serde_json::to_string(description).expect("encode description")
            );
            let error = classify_token_status(StatusCode::BAD_REQUEST, body.as_bytes());
            assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
            let rendered = format!("{error:?} {error}");
            assert!(!rendered.contains("canary"));
            assert!(!rendered.contains(description));
        }

        assert_eq!(
            classify_provider_error_code("invalid_request", None).1,
            TokenExchangeDiagnostic::InvalidRequestOther
        );
    }

    #[test]
    fn rejects_missing_or_broadened_scopes_invalid_expiry_and_oversized_body() {
        for body in [
            "malformed-success-body".to_owned(),
            "{}".to_owned(),
            format!("{{\"access_token\":\"\",\"refresh_token\":\"r\",\"expires_in\":1,\"scope\":\"{SCOPES_JSON}\",\"token_type\":\"Bearer\"}}"),
            format!("{{\"access_token\":\"a\",\"refresh_token\":\"\",\"expires_in\":1,\"scope\":\"{SCOPES_JSON}\",\"token_type\":\"Bearer\"}}"),
            format!("{{\"access_token\":\"a\",\"refresh_token\":\"r\",\"expires_in\":1,\"scope\":\"{SCOPES_JSON}\",\"token_type\":\"MAC\"}}"),
            "{\"access_token\":\"a\",\"refresh_token\":\"r\",\"expires_in\":0,\"scope\":\"openid email profile https://www.googleapis.com/auth/gmail.readonly\",\"token_type\":\"Bearer\"}".to_owned(),
            "{\"access_token\":\"a\",\"refresh_token\":\"r\",\"expires_in\":1,\"scope\":\"openid email profile https://www.googleapis.com/auth/gmail.readonly extra\",\"token_type\":\"Bearer\"}".to_owned(),
            "x".repeat(MAX_TOKEN_RESPONSE_BYTES + 1),
        ] {
            let (endpoint, _) = mock_server(body.clone(), "200 OK");
            let client = GoogleHttpClient::with_endpoints(&endpoint, &endpoint).expect("client");
            let client_id = ClientId::load_from(&Env).expect("client id");
            let client_secret = client_secret();
            let code = SensitiveString::new("fixture-code".into());
            let (_guard, cancellation) = cancellation();
            let error = tauri::async_runtime::block_on(client.exchange(
                TokenExchangeRequest {
                    client_id: &client_id,
                    client_secret: &client_secret,
                    redirect_uri: "http://127.0.0.1:49152/oauth/callback",
                    code: &code,
                    verifier: "fixture-verifier",
                },
                &cancellation,
            ))
            .unwrap_err();
            assert_eq!(error.tag(), AddAccountErrorTag::TokenExchangeFailed);
            let serialized = serde_json::to_string(&error).expect("serialize sanitized error");
            assert!(!serialized.contains(&body));
        }
    }

    #[test]
    fn fetches_only_a_verified_identity_with_bearer_access() {
        let (endpoint, captured) = mock_server(
            "{\"emailAddress\":\"person@example.test\",\"historyId\":\"123\"}".into(),
            "200 OK",
        );
        let client = GoogleHttpClient::with_endpoints(&endpoint, &endpoint).expect("client");
        let access = SensitiveString::new("fixture-access".into());
        let (_guard, cancellation) = cancellation();
        let identity = tauri::async_runtime::block_on(client.identity(&access, &cancellation))
            .expect("identity");

        assert_eq!(identity.email, "person@example.test");
        assert_eq!(identity.display_name, None);
        let request = captured.recv().expect("captured request");
        assert!(
            request.contains("authorization: Bearer fixture-access")
                || request.contains("Authorization: Bearer fixture-access")
        );
    }

    #[test]
    fn unverified_identity_and_workspace_forbidden_are_sanitized() {
        let (endpoint, _) = mock_server(
            "{\"emailAddress\":\"invalid\",\"historyId\":\"123\"}".into(),
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

    #[test]
    fn reads_only_the_gmail_profile_cursor_with_bearer_access() {
        let (endpoint, captured) = mock_server(
            "{\"emailAddress\":\"person@example.test\",\"messagesTotal\":999,\"historyId\":\"123456789\"}"
                .into(),
            "200 OK",
        );
        let client =
            GoogleHttpClient::with_all_endpoints(&endpoint, &endpoint, &endpoint).expect("client");
        let access = SensitiveString::new("fixture-access".into());
        let profile = tauri::async_runtime::block_on(client.profile(&access)).expect("profile");

        assert_eq!(profile.history_id, "123456789");
        assert_eq!(profile.email, "person@example.test");
        let request = captured.recv().expect("captured request");
        assert!(request.starts_with("GET / HTTP/1.1"));
        assert!(
            request.contains("authorization: Bearer fixture-access")
                || request.contains("Authorization: Bearer fixture-access")
        );
        assert!(!request.contains("messagesTotal=999"));
    }

    #[test]
    fn rejects_invalid_or_failed_gmail_profile_without_disclosing_provider_data() {
        for (body, status) in [
            (
                "{\"emailAddress\":\"person@example.test\",\"historyId\":\"not-a-cursor\"}",
                "200 OK",
            ),
            ("provider-body-fixture-access", "503 Service Unavailable"),
        ] {
            let (endpoint, _) = mock_server(body.into(), status);
            let client = GoogleHttpClient::with_all_endpoints(&endpoint, &endpoint, &endpoint)
                .expect("client");
            let access = SensitiveString::new("fixture-access".into());
            let error = tauri::async_runtime::block_on(client.profile(&access)).unwrap_err();
            let rendered = format!("{error:?} {error}");
            assert!(!rendered.contains("fixture-access"));
            assert!(!rendered.contains("provider-body"));
        }
    }

    #[test]
    fn classifies_gmail_and_refresh_statuses_across_all_read_endpoints_without_leakage() {
        #[derive(Clone, Copy, Debug)]
        enum Operation {
            Profile,
            UnreadList,
            History,
            Metadata,
            Refresh,
        }

        let operations = [
            Operation::Profile,
            Operation::UnreadList,
            Operation::History,
            Operation::Metadata,
            Operation::Refresh,
        ];
        let statuses = [
            ("401 Unauthorized", GmailReadError::AuthRequired),
            ("403 Forbidden", GmailReadError::AuthRequired),
            ("429 Too Many Requests", GmailReadError::Unavailable),
            ("503 Service Unavailable", GmailReadError::Unavailable),
        ];

        for operation in operations {
            for (status, expected) in statuses {
                let (endpoint, _) = mock_server(
                    "provider-status-canary fixture-access fixture-refresh".into(),
                    status,
                );
                let client = GoogleHttpClient::with_all_endpoints(&endpoint, &endpoint, &endpoint)
                    .expect("client");
                let access = SensitiveString::new("fixture-access".into());
                let result = match operation {
                    Operation::Profile => {
                        tauri::async_runtime::block_on(client.profile(&access)).map(|_| ())
                    }
                    Operation::UnreadList => {
                        tauri::async_runtime::block_on(client.unread_message_ids(&access))
                            .map(|_| ())
                    }
                    Operation::History => {
                        tauri::async_runtime::block_on(client.changes_since(&access, "100"))
                            .map(|_| ())
                    }
                    Operation::Metadata => tauri::async_runtime::block_on(
                        client.message_metadata(&access, "message-1"),
                    )
                    .map(|_| ()),
                    Operation::Refresh => {
                        let client_id = ClientId::load_from(&Env).expect("client id");
                        let client_secret = client_secret();
                        tauri::async_runtime::block_on(client.refresh_access_token(
                            &client_id,
                            &client_secret,
                            b"fixture-refresh",
                        ))
                        .map(|_| ())
                    }
                };
                let error = result.expect_err("provider status must fail");
                assert_eq!(error, expected, "{operation:?} at {status}");
                let rendered = format!("{error:?} {error}");
                for forbidden in [
                    "provider-status-canary",
                    "fixture-access",
                    "fixture-refresh",
                ] {
                    assert!(!rendered.contains(forbidden), "{operation:?} at {status}");
                }
            }
        }
    }

    #[test]
    fn paginates_history_and_preserves_all_supported_event_categories() {
        let (profile_endpoint, captured) = mock_server_sequence(vec![
            (
                "{\"history\":[{\"messagesAdded\":[{\"message\":{\"id\":\"m1\",\"threadId\":\"t1\",\"labelIds\":[\"UNREAD\"]}}],\"labelsRemoved\":[{\"message\":{\"id\":\"old\",\"threadId\":\"old-thread\"},\"labelIds\":[\"UNREAD\"]}]}],\"nextPageToken\":\"page-2\",\"historyId\":\"101\"}".into(),
                "200 OK",
            ),
            (
                "{\"history\":[{\"labelsAdded\":[{\"message\":{\"id\":\"old\",\"threadId\":\"old-thread\"},\"labelIds\":[\"UNREAD\"]}],\"messagesDeleted\":[{\"message\":{\"id\":\"gone\",\"threadId\":\"gone-thread\"}}]}],\"historyId\":\"102\"}".into(),
                "200 OK",
            ),
        ]);
        let client = GoogleHttpClient::with_all_endpoints(
            &profile_endpoint,
            &profile_endpoint,
            &profile_endpoint,
        )
        .expect("client");
        let access = SensitiveString::new("fixture-access".into());
        let changes =
            tauri::async_runtime::block_on(client.changes_since(&access, "100")).expect("changes");

        assert_eq!(changes.cursor, "102");
        assert_eq!(changes.events.len(), 4);
        assert!(matches!(
            changes.events[0],
            GmailHistoryEvent::MessageAdded { .. }
        ));
        assert!(matches!(
            changes.events[1],
            GmailHistoryEvent::LabelsRemoved { .. }
        ));
        assert!(matches!(
            changes.events[2],
            GmailHistoryEvent::LabelsAdded { .. }
        ));
        assert!(matches!(
            changes.events[3],
            GmailHistoryEvent::MessageDeleted { .. }
        ));
        let requests = captured.recv().expect("requests");
        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("GET /history?startHistoryId=100"));
        assert!(!requests[0].contains("historyTypes="));
        assert!(requests[1].contains("pageToken=page-2"));
    }

    #[test]
    fn accepts_history_page_larger_than_legacy_limit_and_paginates() {
        let records = (0..500)
            .map(|index| {
                let id = format!("message-{index:04}-{}", "a".repeat(64));
                let thread_id = format!("thread-{index:04}-{}", "b".repeat(64));
                format!(
                    "{{\"messagesAdded\":[{{\"message\":{{\"id\":\"{id}\",\"threadId\":\"{thread_id}\",\"labelIds\":[\"UNREAD\"]}}}}]}}"
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let first_page = format!(
            "{{\"history\":[{records}],\"nextPageToken\":\"page-2\",\"historyId\":\"101\"}}"
        );
        assert!(first_page.len() > 32 * 1024);
        assert!(first_page.len() <= MAX_GMAIL_RESPONSE_BYTES);
        let (profile_endpoint, captured) = mock_server_sequence(vec![
            (first_page, "200 OK"),
            ("{\"historyId\":\"102\"}".into(), "200 OK"),
        ]);
        let client = GoogleHttpClient::with_all_endpoints(
            &profile_endpoint,
            &profile_endpoint,
            &profile_endpoint,
        )
        .expect("client");
        let access = SensitiveString::new("fixture-access".into());

        let changes =
            tauri::async_runtime::block_on(client.changes_since(&access, "100")).expect("changes");

        assert_eq!(changes.cursor, "102");
        assert_eq!(changes.events.len(), 500);
        let requests = captured.recv().expect("requests");
        assert_eq!(requests.len(), 2);
        assert!(requests[1].contains("pageToken=page-2"));
    }

    #[test]
    fn fetches_exact_metadata_headers_and_discards_to() {
        let (endpoint, captured) = mock_server(
            "{\"id\":\"kept\",\"threadId\":\"kept-thread\",\"internalDate\":\"1775995260000\",\"labelIds\":[\"INBOX\",\"UNREAD\"],\"payload\":{\"headers\":[{\"name\":\"From\",\"value\":\"Kept <kept@example.test>\"},{\"name\":\"To\",\"value\":\"recipient@example.test\"},{\"name\":\"Subject\",\"value\":\"Still here\"}]}}".into(),
            "200 OK",
        );
        let profile_endpoint = format!("{endpoint}/profile");
        let client = GoogleHttpClient::with_all_endpoints(&endpoint, &endpoint, &profile_endpoint)
            .expect("client");
        let access = SensitiveString::new("fixture-access".into());
        let message = tauri::async_runtime::block_on(client.message_metadata(&access, "kept"))
            .expect("metadata")
            .expect("message");

        assert_eq!(message.subject, "Still here");
        assert!(message.is_inbox_unread);
        let request = captured.recv().expect("request");
        assert!(request.starts_with("GET /messages/kept?"));
        assert!(request.contains("format=metadata"));
        assert!(request.contains("metadataHeaders=From"));
        assert!(request.contains("metadataHeaders=To"));
        assert!(request.contains("metadataHeaders=Subject"));
        for forbidden in ["format=full", "format=raw", "snippet", "attachments"] {
            assert!(!request.contains(forbidden));
        }
    }

    #[test]
    fn metadata_eligibility_requires_both_inbox_and_unread_labels() {
        let message = |id: &str, labels: &str| {
            format!(
                "{{\"id\":\"{id}\",\"threadId\":\"thread-{id}\",\"internalDate\":\"1775995260000\",\"labelIds\":{labels},\"payload\":{{\"headers\":[]}}}}"
            )
        };
        let (endpoint, _) = mock_server_sequence(vec![
            (message("archived", "[\"UNREAD\"]"), "200 OK"),
            (message("read", "[\"INBOX\"]"), "200 OK"),
            (message("eligible", "[\"INBOX\",\"UNREAD\"]"), "200 OK"),
        ]);
        let profile = format!("{endpoint}/profile");
        let client =
            GoogleHttpClient::with_all_endpoints(&endpoint, &endpoint, &profile).expect("client");
        let access = SensitiveString::new("fixture-access".into());

        for (id, expected) in [("archived", false), ("read", false), ("eligible", true)] {
            let metadata = tauri::async_runtime::block_on(client.message_metadata(&access, id))
                .expect("metadata")
                .expect("message");
            assert_eq!(metadata.is_inbox_unread, expected, "{id}");
        }
    }

    #[test]
    fn only_history_list_404_is_classified_for_zero_pending_recovery() {
        let (endpoint, _) = mock_server("{}".into(), "404 Not Found");
        let client =
            GoogleHttpClient::with_all_endpoints(&endpoint, &endpoint, &endpoint).expect("client");
        let access = SensitiveString::new("fixture-access".into());
        assert_eq!(
            tauri::async_runtime::block_on(client.changes_since(&access, "100")).unwrap_err(),
            GmailReadError::HistoryExpired
        );
    }

    #[test]
    fn paginates_unread_list_with_exact_query_and_deduplicates_ids() {
        let (profile, captured) = mock_server_sequence(vec![
            (
                "{\"messages\":[{\"id\":\"m1\",\"threadId\":\"t1\"}],\"nextPageToken\":\"next\"}".into(),
                "200 OK",
            ),
            (
                "{\"messages\":[{\"id\":\"m1\",\"threadId\":\"t1\"},{\"id\":\"m2\",\"threadId\":\"t2\"}]}".into(),
                "200 OK",
            ),
        ]);
        let client =
            GoogleHttpClient::with_all_endpoints(&profile, &profile, &profile).expect("client");
        let access = SensitiveString::new("fixture-access".into());

        let unread =
            tauri::async_runtime::block_on(client.unread_message_ids(&access)).expect("unread ids");

        assert_eq!(unread, ["m1", "m2"]);
        let requests = captured.recv().expect("requests");
        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("GET /messages?q=is%3Aunread&labelIds=INBOX&maxResults=500"));
        assert!(requests[1].contains("pageToken=next"));
        assert!(requests[0]
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-access\r\n"));
        assert!(!requests.iter().any(|request| request.contains("/labels/")));
    }

    #[test]
    fn accepts_unread_page_larger_than_legacy_limit_and_paginates() {
        let messages = (0..500)
            .map(|index| {
                let id = format!("message-{index:04}-{}", "a".repeat(64));
                let thread_id = format!("thread-{index:04}-{}", "b".repeat(64));
                format!("{{\"id\":\"{id}\",\"threadId\":\"{thread_id}\"}}")
            })
            .collect::<Vec<_>>()
            .join(",");
        let first_page = format!("{{\"messages\":[{messages}],\"nextPageToken\":\"page-2\"}}");
        assert!(first_page.len() > 32 * 1024);
        assert!(first_page.len() <= MAX_GMAIL_RESPONSE_BYTES);
        let (profile, captured) = mock_server_sequence(vec![
            (first_page, "200 OK"),
            (
                "{\"messages\":[{\"id\":\"last\",\"threadId\":\"last-thread\"}]}".into(),
                "200 OK",
            ),
        ]);
        let client =
            GoogleHttpClient::with_all_endpoints(&profile, &profile, &profile).expect("client");
        let access = SensitiveString::new("fixture-access".into());

        let unread =
            tauri::async_runtime::block_on(client.unread_message_ids(&access)).expect("unread ids");

        assert_eq!(unread.len(), 501);
        assert_eq!(unread.last().map(String::as_str), Some("last"));
        let requests = captured.recv().expect("requests");
        assert_eq!(requests.len(), 2);
        assert!(requests[1].contains("pageToken=page-2"));
    }

    #[test]
    fn unread_list_rejects_malformed_and_oversized_responses_without_leakage() {
        for body in [
            "{\"messages\":[{\"id\":\"bad/id\",\"threadId\":\"t1\"}]}".to_owned(),
            "{\"nextPageToken\":\"\"}".to_owned(),
            "{\"messages\":\"fixture-canary\"}".to_owned(),
            "x".repeat(MAX_GMAIL_RESPONSE_BYTES + 1),
        ] {
            let (endpoint, _) = mock_server(body, "200 OK");
            let profile = format!("{endpoint}/profile");
            let client = GoogleHttpClient::with_all_endpoints(&endpoint, &endpoint, &profile)
                .expect("client");
            let access = SensitiveString::new("fixture-access".into());
            let error =
                tauri::async_runtime::block_on(client.unread_message_ids(&access)).unwrap_err();
            assert_eq!(error, GmailReadError::InvalidResponse);
            assert!(!format!("{error:?} {error}").contains("fixture-canary"));
        }
    }

    #[test]
    fn revoke_is_exact_bounded_redirect_free_and_redacted() {
        let (endpoint, captured) = mock_server(String::new(), "200 OK");
        let client =
            GoogleHttpClient::with_provider_endpoints(&endpoint, &endpoint, &endpoint, &endpoint)
                .expect("client");
        tauri::async_runtime::block_on(client.revoke(b"fixture-refresh-token")).expect("revoke");
        let request = captured.recv().expect("request");
        assert!(request.starts_with("POST / HTTP/1.1\r\n"));
        assert!(request.contains("content-type: application/x-www-form-urlencoded"));
        assert!(request.ends_with("token=fixture-refresh-token"));

        for (body, status, expected_ok) in [
            (
                "invalid fixture provider detail".to_owned(),
                "400 Bad Request",
                true,
            ),
            (
                "redirect fixture provider detail".to_owned(),
                "302 Found",
                false,
            ),
            (
                "x".repeat(MAX_REVOKE_RESPONSE_BYTES + 1),
                "500 Server Error",
                false,
            ),
        ] {
            let (endpoint, _) = mock_server(body, status);
            let client = GoogleHttpClient::with_provider_endpoints(
                &endpoint, &endpoint, &endpoint, &endpoint,
            )
            .expect("client");
            let result = tauri::async_runtime::block_on(client.revoke(b"fixture-refresh-token"));
            assert_eq!(result.is_ok(), expected_ok);
            if let Err(error) = result {
                assert!(!format!("{error:?} {error}").contains("fixture"));
            }
        }

        assert!(tauri::async_runtime::block_on(client.revoke(&[])).is_ok());
        assert!(tauri::async_runtime::block_on(client.revoke(&[0xff])).is_ok());
    }

    #[test]
    fn revoke_network_failure_is_sanitized_for_best_effort_callers() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("reserve endpoint");
        let endpoint = format!("http://{}", listener.local_addr().expect("address"));
        drop(listener);
        let client =
            GoogleHttpClient::with_provider_endpoints(&endpoint, &endpoint, &endpoint, &endpoint)
                .expect("client");
        let error =
            tauri::async_runtime::block_on(client.revoke(b"fixture-refresh-token")).unwrap_err();
        assert_eq!(format!("{error:?}"), "TokenRevokeError");
        assert!(!format!("{error:?} {error}").contains("fixture-refresh-token"));
    }

    #[test]
    fn refreshes_access_with_client_credential_and_redacts_failures() {
        let (endpoint, captured) = mock_server(
            format!("{{\"access_token\":\"fixture-new-access\",\"expires_in\":3600,\"scope\":\"{SCOPES_JSON}\",\"token_type\":\"Bearer\"}}"),
            "200 OK",
        );
        let client =
            GoogleHttpClient::with_all_endpoints(&endpoint, &endpoint, &endpoint).expect("client");
        let client_id = ClientId::load_from(&Env).expect("client id");
        let client_secret = client_secret();
        let token = tauri::async_runtime::block_on(client.refresh_access_token(
            &client_id,
            &client_secret,
            b"fixture-refresh",
        ))
        .expect("refresh");
        assert_eq!(token.expose(), "fixture-new-access");
        let request = captured.recv().expect("request");
        let encoded_body = request
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .expect("form body");
        let fields: Vec<_> = url::form_urlencoded::parse(encoded_body.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(
            fields,
            vec![
                ("client_id".to_owned(), CLIENT_ID.to_owned()),
                (
                    "client_secret".to_owned(),
                    "fixture-client-secret-value".to_owned(),
                ),
                ("refresh_token".to_owned(), "fixture-refresh".to_owned()),
                ("grant_type".to_owned(), "refresh_token".to_owned()),
            ]
        );

        let (endpoint, _) =
            mock_server("fixture-refresh provider detail".into(), "400 Bad Request");
        let client =
            GoogleHttpClient::with_all_endpoints(&endpoint, &endpoint, &endpoint).expect("client");
        let error = tauri::async_runtime::block_on(client.refresh_access_token(
            &client_id,
            &client_secret,
            b"fixture-refresh",
        ))
        .unwrap_err();
        assert_eq!(error, GmailReadError::AuthRequired);
        assert!(!format!("{error:?} {error}").contains("fixture-refresh"));
    }
}
