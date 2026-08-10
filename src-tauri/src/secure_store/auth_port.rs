use std::{fmt, sync::Arc};

use uuid::Uuid;
use zeroize::Zeroizing;

use super::{DeploymentMode, SecureStore, SecureStoreError};
use crate::auth::{AddAccountError, AddAccountErrorTag, PortFuture, RefreshTokenStorePort};

trait RefreshTokenStorage: Send + Sync {
    fn set(&self, account_id: Uuid, refresh_token: &[u8]) -> Result<(), SecureStoreError>;
    fn delete(&self, account_id: Uuid) -> Result<(), SecureStoreError>;
}

impl RefreshTokenStorage for SecureStore {
    fn set(&self, account_id: Uuid, refresh_token: &[u8]) -> Result<(), SecureStoreError> {
        self.set_refresh_token(account_id, refresh_token)
    }

    fn delete(&self, account_id: Uuid) -> Result<(), SecureStoreError> {
        self.delete_refresh_token(account_id)
    }
}

#[derive(Clone)]
pub(crate) struct SecureRefreshTokenStorePort {
    storage: Arc<dyn RefreshTokenStorage>,
}

impl SecureRefreshTokenStorePort {
    pub(crate) fn signed_provisioned() -> Result<Self, AddAccountError> {
        Self::from_deployment(DeploymentMode::SignedProvisioned)
    }

    /// Constructs the explicit legacy path for unsigned debug development.
    /// Release builds fail closed through `SecureStore` and never fall back.
    pub(crate) fn unsigned_development() -> Result<Self, AddAccountError> {
        Self::from_deployment(DeploymentMode::UnsignedDevelopment)
    }

    fn from_deployment(mode: DeploymentMode) -> Result<Self, AddAccountError> {
        SecureStore::new(mode)
            .map(|store| Self {
                storage: Arc::new(store),
            })
            .map_err(secure_store_failed)
    }

    #[cfg(test)]
    fn with_storage(storage: Arc<dyn RefreshTokenStorage>) -> Self {
        Self { storage }
    }

    /// Deletes a token for OAuth rollback or account cleanup.
    ///
    /// The integrated auth trait currently exposes only `set`, so deletion is
    /// an inherent operation until Lead extends that Rust-only port.
    pub(crate) fn delete<'a>(
        &'a self,
        account_id: &'a str,
    ) -> PortFuture<'a, Result<(), AddAccountError>> {
        Box::pin(async move {
            let account_id = parse_account_id(account_id)?;
            self.storage.delete(account_id).map_err(secure_store_failed)
        })
    }
}

impl fmt::Debug for SecureRefreshTokenStorePort {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecureRefreshTokenStorePort([REDACTED])")
    }
}

impl RefreshTokenStorePort for SecureRefreshTokenStorePort {
    fn set<'a>(
        &'a self,
        account_id: &'a str,
        refresh_token: &'a [u8],
    ) -> PortFuture<'a, Result<(), AddAccountError>> {
        Box::pin(async move {
            let account_id = parse_account_id(account_id)?;
            let refresh_token = Zeroizing::new(refresh_token.to_vec());
            self.storage
                .set(account_id, refresh_token.as_slice())
                .map_err(secure_store_failed)
        })
    }
}

fn parse_account_id(value: &str) -> Result<Uuid, AddAccountError> {
    let parsed = Uuid::parse_str(value).map_err(|_| secure_store_failed_sanitized())?;
    let is_canonical = !parsed.is_nil() && parsed.hyphenated().to_string() == value;
    if is_canonical {
        Ok(parsed)
    } else {
        Err(secure_store_failed_sanitized())
    }
}

fn secure_store_failed(_error: SecureStoreError) -> AddAccountError {
    secure_store_failed_sanitized()
}

fn secure_store_failed_sanitized() -> AddAccountError {
    AddAccountError::from_tag(AddAccountErrorTag::SecureStoreFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secure_store::SecureStoreErrorKind;
    use std::{
        future::Future,
        pin::Pin,
        sync::Mutex,
        task::{Context, Poll, Waker},
    };

    const ACCOUNT_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const ACCOUNT_UUID: Uuid = Uuid::from_u128(0xaaaaaaaa_aaaa_4aaa_8aaa_aaaaaaaaaaaa);
    const SECRET: [u8; 32] = [0xA5; 32];

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Operation {
        Set(Uuid),
        Delete(Uuid),
    }

    struct FakeStorage {
        operations: Mutex<Vec<Operation>>,
        failure: Option<SecureStoreErrorKind>,
    }

    impl FakeStorage {
        fn available() -> Self {
            Self {
                operations: Mutex::new(Vec::new()),
                failure: None,
            }
        }

        fn failing(kind: SecureStoreErrorKind) -> Self {
            Self {
                operations: Mutex::new(Vec::new()),
                failure: Some(kind),
            }
        }

        fn result(&self) -> Result<(), SecureStoreError> {
            match self.failure {
                None => Ok(()),
                Some(SecureStoreErrorKind::AccessDeniedOrLocked) => {
                    Err(SecureStoreError::access_denied_or_locked())
                }
                Some(SecureStoreErrorKind::InvalidSecret) => {
                    Err(SecureStoreError::invalid_secret())
                }
                Some(SecureStoreErrorKind::Unavailable) => Err(SecureStoreError::unavailable()),
            }
        }
    }

    impl RefreshTokenStorage for FakeStorage {
        fn set(&self, account_id: Uuid, refresh_token: &[u8]) -> Result<(), SecureStoreError> {
            assert_eq!(refresh_token, SECRET);
            self.operations
                .lock()
                .map_err(|_| SecureStoreError::unavailable())?
                .push(Operation::Set(account_id));
            self.result()
        }

        fn delete(&self, account_id: Uuid) -> Result<(), SecureStoreError> {
            self.operations
                .lock()
                .map_err(|_| SecureStoreError::unavailable())?
                .push(Operation::Delete(account_id));
            self.result()
        }
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
    fn set_and_delete_preserve_order_and_canonical_uuid() {
        let storage = Arc::new(FakeStorage::available());
        let adapter = SecureRefreshTokenStorePort::with_storage(storage.clone());

        block_on_ready(adapter.set(ACCOUNT_ID, &SECRET)).expect("set");
        block_on_ready(adapter.delete(ACCOUNT_ID)).expect("delete");

        assert_eq!(
            storage.operations.lock().expect("operations").as_slice(),
            [
                Operation::Set(ACCOUNT_UUID),
                Operation::Delete(ACCOUNT_UUID)
            ]
        );
    }

    #[test]
    fn malformed_or_noncanonical_uuid_never_reaches_storage() {
        let storage = Arc::new(FakeStorage::available());
        let adapter = SecureRefreshTokenStorePort::with_storage(storage.clone());

        for invalid in [
            "not-a-uuid",
            "aaaaaaaaaaaa4aaa8aaaaaaaaaaaaaaa",
            "AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA",
            "00000000-0000-0000-0000-000000000000",
        ] {
            let error = block_on_ready(adapter.set(invalid, &SECRET)).unwrap_err();
            let rendered = format!("{error}\n{error:?}");
            assert_eq!(error.tag(), AddAccountErrorTag::SecureStoreFailed);
            assert!(!rendered.contains(invalid));
        }

        assert!(storage.operations.lock().expect("operations").is_empty());
    }

    #[test]
    fn denied_or_locked_and_unavailable_storage_errors_are_redacted() {
        for kind in [
            SecureStoreErrorKind::AccessDeniedOrLocked,
            SecureStoreErrorKind::Unavailable,
        ] {
            let storage = Arc::new(FakeStorage::failing(kind));
            let adapter = SecureRefreshTokenStorePort::with_storage(storage);

            let set_error = block_on_ready(adapter.set(ACCOUNT_ID, &SECRET)).unwrap_err();
            let delete_error = block_on_ready(adapter.delete(ACCOUNT_ID)).unwrap_err();

            for error in [set_error, delete_error] {
                assert_eq!(error.tag(), AddAccountErrorTag::SecureStoreFailed);
                assert_eq!(
                    error.to_string(),
                    "Zerochi could not store the account credential securely."
                );
                assert!(!format!("{error:?}").contains("Keychain"));
            }
        }
    }

    #[test]
    fn signed_and_unsigned_constructors_use_explicit_backend_policy() {
        assert_eq!(
            super::super::select_backend(DeploymentMode::SignedProvisioned).unwrap(),
            super::super::BackendKind::ProtectedData
        );

        let unsigned = super::super::select_backend(DeploymentMode::UnsignedDevelopment);
        if cfg!(debug_assertions) {
            assert_eq!(unsigned.unwrap(), super::super::BackendKind::LegacyKeychain);
        } else {
            assert_eq!(
                unsigned.unwrap_err().kind(),
                SecureStoreErrorKind::Unavailable
            );
        }
    }

    #[test]
    fn adapter_debug_never_exposes_storage_details() {
        let adapter = SecureRefreshTokenStorePort::with_storage(Arc::new(FakeStorage::available()));
        assert_eq!(
            format!("{adapter:?}"),
            "SecureRefreshTokenStorePort([REDACTED])"
        );
    }
}
