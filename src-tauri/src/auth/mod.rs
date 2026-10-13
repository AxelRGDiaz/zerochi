//! Rust-only Google OAuth foundation.
//!
//! Live browser, callback, exchange, storage, and command wiring are deliberately
//! deferred. Until then, the foundation's crate-private surface is not yet called.

#![allow(dead_code)]

mod attempt;
mod callback;
mod config;
mod error;
mod pkce;
mod ports;
mod remove;
mod request;
mod secret;
mod service;

#[allow(unused_imports)]
pub(crate) use attempt::{AddAccountAttemptGuard, AddAccountAttemptGuardSet, Cancellation};
#[allow(unused_imports)]
pub(crate) use callback::LoopbackCallbackPort;
#[allow(unused_imports)]
pub(crate) use config::{
    ClientId, ClientSecret, Environment, ProcessClientId, ProcessClientSecret, ProcessEnvironment,
    ProcessMicrosoftClientId, PublicClientSecret,
};
pub(crate) use error::{AddAccountError, AddAccountErrorTag};
#[allow(unused_imports)]
pub(crate) use pkce::{generate_pkce, OsRandom, Pkce, RandomSource};
#[allow(unused_imports)]
pub(crate) use ports::{
    AccountIdPort, AuthorizationCallback, BrowserPort, CallbackPort, CallbackSession, ClientIdPort,
    ClientSecretPort, IdentityPort, PortFuture, RefreshTokenReadError, RefreshTokenReadPort,
    RefreshTokenStorePort, TokenExchangePort, TokenExchangeRequest, TokenResponse,
};
#[allow(unused_imports)]
pub(crate) use remove::{
    RemoveAccountError, RemoveAccountErrorTag, RemoveAccountPorts, RemoveAccountService,
};
#[allow(unused_imports)]
pub(crate) use request::{
    AuthorizationRequest, LocalhostRedirect, LoopbackRedirect, APPROVED_SCOPES, MICROSOFT_SCOPES,
};
pub(crate) use secret::SensitiveString;
#[allow(unused_imports)]
pub(crate) use service::{AddAccountPorts, AddAccountService, OsAccountId};
