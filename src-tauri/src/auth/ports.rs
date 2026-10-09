use super::{
    AddAccountError, AuthorizationRequest, Cancellation, ClientId, ClientSecret, SensitiveString,
};
use crate::gmail::GoogleIdentity;
use std::{future::Future, pin::Pin, time::Duration};
use uuid::Uuid;
use zeroize::Zeroizing;

pub(crate) type PortFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub(crate) trait ClientIdPort: Send + Sync {
    fn load(&self) -> Result<ClientId, AddAccountError>;
}

pub(crate) trait ClientSecretPort: Send + Sync {
    fn load(&self) -> Result<ClientSecret, AddAccountError>;
}

/// Native adapter seam. Implementations must not log the authorization URL.
pub(crate) trait BrowserPort: Send + Sync {
    fn open<'a>(
        &'a self,
        request: &'a AuthorizationRequest,
        cancellation: &'a Cancellation,
    ) -> PortFuture<'a, Result<(), AddAccountError>>;
}

pub(crate) struct AuthorizationCallback {
    pub(crate) code: SensitiveString,
}

impl std::fmt::Debug for AuthorizationCallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AuthorizationCallback([REDACTED])")
    }
}

pub(crate) trait CallbackSession: Send {
    fn redirect_uri(&self) -> &str;

    fn receive<'a>(
        &'a mut self,
        expected_state: &'a str,
        cancellation: &'a Cancellation,
    ) -> PortFuture<'a, Result<AuthorizationCallback, AddAccountError>>;
}

/// A callback is bound before browser launch so its exact ephemeral redirect is authorized.
pub(crate) trait CallbackPort: Send + Sync {
    fn bind(&self) -> Result<Box<dyn CallbackSession>, AddAccountError>;
}

pub(crate) struct TokenExchangeRequest<'a> {
    pub(crate) client_id: &'a ClientId,
    pub(crate) client_secret: &'a ClientSecret,
    pub(crate) redirect_uri: &'a str,
    pub(crate) code: &'a SensitiveString,
    pub(crate) verifier: &'a str,
}

pub(crate) struct TokenResponse {
    pub(crate) access_token: SensitiveString,
    pub(crate) refresh_token: Option<SensitiveString>,
    pub(crate) expires_in: Duration,
}

impl std::fmt::Debug for TokenResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TokenResponse")
            .field("access_token", &"[redacted]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[redacted]"),
            )
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

pub(crate) trait TokenExchangePort: Send + Sync {
    fn exchange<'a>(
        &'a self,
        request: TokenExchangeRequest<'a>,
        cancellation: &'a Cancellation,
    ) -> PortFuture<'a, Result<TokenResponse, AddAccountError>>;
}

pub(crate) trait IdentityPort: Send + Sync {
    fn identity<'a>(
        &'a self,
        access_token: &'a SensitiveString,
        cancellation: &'a Cancellation,
    ) -> PortFuture<'a, Result<GoogleIdentity, AddAccountError>>;
}

/// Add-account subset of the existing SecureStore port.
pub(crate) trait RefreshTokenStorePort: Send + Sync {
    fn set<'a>(
        &'a self,
        account_id: Uuid,
        refresh_token: &'a [u8],
    ) -> PortFuture<'a, Result<(), AddAccountError>>;

    fn delete<'a>(&'a self, account_id: Uuid) -> PortFuture<'a, Result<(), AddAccountError>>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RefreshTokenReadError {
    AccessDeniedOrLocked,
    Unavailable,
}

impl std::fmt::Display for RefreshTokenReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Account credentials are unavailable")
    }
}

impl std::error::Error for RefreshTokenReadError {}

/// Polling-only Rust port. Missing credentials are `Ok(None)`; returned buffers scrub on drop.
pub(crate) trait RefreshTokenReadPort: Send + Sync {
    fn get<'a>(
        &'a self,
        account_id: Uuid,
    ) -> PortFuture<'a, Result<Option<Zeroizing<Vec<u8>>>, RefreshTokenReadError>>;
}

pub(crate) trait AccountIdPort: Send + Sync {
    fn generate(&self) -> Result<Uuid, AddAccountError>;
}
