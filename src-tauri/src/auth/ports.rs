use super::{AddAccountError, AuthorizationRequest, Cancellation, ClientId, SensitiveString};
use std::{future::Future, pin::Pin, time::Duration};

pub(crate) type PortFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

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

/// Loopback adapter seam. Implementations validate method/path/state before returning a code.
pub(crate) trait CallbackPort: Send + Sync {
    fn receive<'a>(
        &'a self,
        expected_state: &'a str,
        cancellation: &'a Cancellation,
    ) -> PortFuture<'a, Result<AuthorizationCallback, AddAccountError>>;
}

pub(crate) struct TokenExchangeRequest<'a> {
    pub(crate) client_id: &'a ClientId,
    pub(crate) redirect_uri: &'a str,
    pub(crate) code: &'a SensitiveString,
    pub(crate) verifier: &'a str,
}

pub(crate) struct TokenResponse {
    pub(crate) access_token: SensitiveString,
    pub(crate) refresh_token: Option<SensitiveString>,
    pub(crate) expires_in: Duration,
}

/// HTTP adapter seam. No concrete network client is selected in this milestone.
pub(crate) trait TokenExchangePort: Send + Sync {
    fn exchange<'a>(
        &'a self,
        request: TokenExchangeRequest<'a>,
        cancellation: &'a Cancellation,
    ) -> PortFuture<'a, Result<TokenResponse, AddAccountError>>;
}

/// Add-account subset of the existing SecureStore port.
pub(crate) trait RefreshTokenStorePort: Send + Sync {
    fn set<'a>(
        &'a self,
        account_id: &'a str,
        refresh_token: &'a [u8],
    ) -> PortFuture<'a, Result<(), AddAccountError>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NetworkFreePorts;

    impl BrowserPort for NetworkFreePorts {
        fn open<'a>(
            &'a self,
            _request: &'a AuthorizationRequest,
            _cancellation: &'a Cancellation,
        ) -> PortFuture<'a, Result<(), AddAccountError>> {
            Box::pin(async { Ok(()) })
        }
    }

    impl CallbackPort for NetworkFreePorts {
        fn receive<'a>(
            &'a self,
            _expected_state: &'a str,
            _cancellation: &'a Cancellation,
        ) -> PortFuture<'a, Result<AuthorizationCallback, AddAccountError>> {
            Box::pin(async {
                Err(AddAccountError::from_tag(
                    super::super::AddAccountErrorTag::Cancelled,
                ))
            })
        }
    }

    impl TokenExchangePort for NetworkFreePorts {
        fn exchange<'a>(
            &'a self,
            _request: TokenExchangeRequest<'a>,
            _cancellation: &'a Cancellation,
        ) -> PortFuture<'a, Result<TokenResponse, AddAccountError>> {
            Box::pin(async {
                Err(AddAccountError::from_tag(
                    super::super::AddAccountErrorTag::TokenExchangeFailed,
                ))
            })
        }
    }

    impl RefreshTokenStorePort for NetworkFreePorts {
        fn set<'a>(
            &'a self,
            _account_id: &'a str,
            _refresh_token: &'a [u8],
        ) -> PortFuture<'a, Result<(), AddAccountError>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[test]
    fn all_external_operations_are_injected_without_concrete_or_network_adapters() {
        fn accepts_ports(
            _browser: &dyn BrowserPort,
            _callback: &dyn CallbackPort,
            _exchange: &dyn TokenExchangePort,
            _store: &dyn RefreshTokenStorePort,
        ) {
        }

        accepts_ports(
            &NetworkFreePorts,
            &NetworkFreePorts,
            &NetworkFreePorts,
            &NetworkFreePorts,
        );
    }
}
