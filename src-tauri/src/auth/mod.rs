//! Rust-only Google OAuth foundation.
//!
//! Live browser, callback, exchange, storage, and command wiring are deliberately
//! deferred. Until then, the foundation's crate-private surface is not yet called.

#![allow(dead_code)]

mod attempt;
mod config;
mod error;
mod pkce;
mod ports;
mod request;
mod secret;

#[allow(unused_imports)]
pub(crate) use attempt::{AddAccountAttemptGuard, AddAccountAttemptGuardSet, Cancellation};
#[allow(unused_imports)]
pub(crate) use config::{ClientId, Environment, ProcessEnvironment};
pub(crate) use error::{AddAccountError, AddAccountErrorTag};
#[allow(unused_imports)]
pub(crate) use pkce::{generate_pkce, OsRandom, Pkce, RandomSource};
#[allow(unused_imports)]
pub(crate) use ports::{
    AuthorizationCallback, BrowserPort, CallbackPort, PortFuture, RefreshTokenStorePort,
    TokenExchangePort, TokenExchangeRequest, TokenResponse,
};
#[allow(unused_imports)]
pub(crate) use request::{AuthorizationRequest, LoopbackRedirect};
pub(crate) use secret::SensitiveString;
