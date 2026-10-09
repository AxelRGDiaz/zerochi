use std::{fmt, sync::Arc};

use uuid::Uuid;
use zeroize::Zeroizing;

use super::{DeploymentMode, SecureStore, SecureStoreError};
use crate::auth::{
    AddAccountError, AddAccountErrorTag, PortFuture, RefreshTokenReadError, RefreshTokenReadPort,
    RefreshTokenStorePort,
};

trait RefreshTokenStorage: Send + Sync {
    fn set(&self, account_id: Uuid, refresh_token: &[u8]) -> Result<(), SecureStoreError>;
    fn get(&self, account_id: Uuid) -> Result<Option<Zeroizing<Vec<u8>>>, SecureStoreError>;
    fn delete(&self, account_id: Uuid) -> Result<(), SecureStoreError>;
}

impl RefreshTokenStorage for SecureStore {
    fn set(&self, account_id: Uuid, refresh_token: &[u8]) -> Result<(), SecureStoreError> {
        self.set_refresh_token(account_id, refresh_token)
    }

    fn get(&self, account_id: Uuid) -> Result<Option<Zeroizing<Vec<u8>>>, SecureStoreError> {
        self.get_refresh_token(account_id)
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
}

impl fmt::Debug for SecureRefreshTokenStorePort {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecureRefreshTokenStorePort([REDACTED])")
    }
}

impl RefreshTokenStorePort for SecureRefreshTokenStorePort {
    fn set<'a>(
        &'a self,
        account_id: Uuid,
        refresh_token: &'a [u8],
    ) -> PortFuture<'a, Result<(), AddAccountError>> {
        Box::pin(async move {
            let refresh_token = Zeroizing::new(refresh_token.to_vec());
            self.storage
                .set(account_id, refresh_token.as_slice())
                .map_err(secure_store_failed)
        })
    }

    fn delete<'a>(&'a self, account_id: Uuid) -> PortFuture<'a, Result<(), AddAccountError>> {
        Box::pin(async move { self.storage.delete(account_id).map_err(secure_store_failed) })
    }
}

impl RefreshTokenReadPort for SecureRefreshTokenStorePort {
    fn get<'a>(
        &'a self,
        account_id: Uuid,
    ) -> PortFuture<'a, Result<Option<Zeroizing<Vec<u8>>>, RefreshTokenReadError>> {
        Box::pin(async move {
            self.storage
                .get(account_id)
                .map_err(refresh_token_read_failed)
        })
    }
}

fn secure_store_failed(_error: SecureStoreError) -> AddAccountError {
    secure_store_failed_sanitized()
}

fn secure_store_failed_sanitized() -> AddAccountError {
    AddAccountError::from_tag(AddAccountErrorTag::SecureStoreFailed)
}

fn refresh_token_read_failed(error: SecureStoreError) -> RefreshTokenReadError {
    match error.kind() {
        super::SecureStoreErrorKind::AccessDeniedOrLocked => {
            RefreshTokenReadError::AccessDeniedOrLocked
        }
        super::SecureStoreErrorKind::InvalidSecret | super::SecureStoreErrorKind::Unavailable => {
            RefreshTokenReadError::Unavailable
        }
    }
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

    const ACCOUNT_UUID: Uuid = Uuid::from_u128(0xaaaaaaaa_aaaa_4aaa_8aaa_aaaaaaaaaaaa);
    const ACCOUNT_UUID_B: Uuid = Uuid::from_u128(0xbbbbbbbb_bbbb_4bbb_8bbb_bbbbbbbbbbbb);
    const SECRET: [u8; 32] = [0xA5; 32];

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Operation {
        Set(Uuid),
        Get(Uuid),
        Delete(Uuid),
    }

    struct FakeStorage {
        operations: Mutex<Vec<Operation>>,
        failure: Option<SecureStoreErrorKind>,
        secret: Option<Vec<u8>>,
    }

    impl FakeStorage {
        fn available() -> Self {
            Self {
                operations: Mutex::new(Vec::new()),
                failure: None,
                secret: Some(SECRET.to_vec()),
            }
        }

        fn missing() -> Self {
            Self {
                operations: Mutex::new(Vec::new()),
                failure: None,
                secret: None,
            }
        }

        fn failing(kind: SecureStoreErrorKind) -> Self {
            Self {
                operations: Mutex::new(Vec::new()),
                failure: Some(kind),
                secret: Some(SECRET.to_vec()),
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

        fn get(&self, account_id: Uuid) -> Result<Option<Zeroizing<Vec<u8>>>, SecureStoreError> {
            self.operations
                .lock()
                .map_err(|_| SecureStoreError::unavailable())?
                .push(Operation::Get(account_id));
            self.result()?;
            Ok(self.secret.clone().map(Zeroizing::new))
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
    fn set_and_delete_preserve_order_for_typed_uuid() {
        let storage = Arc::new(FakeStorage::available());
        let adapter = SecureRefreshTokenStorePort::with_storage(storage.clone());

        block_on_ready(adapter.set(ACCOUNT_UUID, &SECRET)).expect("set");
        block_on_ready(adapter.delete(ACCOUNT_UUID)).expect("delete");

        assert_eq!(
            storage.operations.lock().expect("operations").as_slice(),
            [
                Operation::Set(ACCOUNT_UUID),
                Operation::Delete(ACCOUNT_UUID)
            ]
        );
    }

    #[test]
    fn get_returns_zeroizing_bytes_for_the_typed_uuid() {
        let storage = Arc::new(FakeStorage::available());
        let adapter = SecureRefreshTokenStorePort::with_storage(storage.clone());

        let secret = block_on_ready(RefreshTokenReadPort::get(&adapter, ACCOUNT_UUID))
            .expect("get")
            .expect("stored refresh token");

        assert_eq!(secret.as_slice(), SECRET);
        assert_eq!(
            storage.operations.lock().expect("operations").as_slice(),
            [Operation::Get(ACCOUNT_UUID)]
        );
    }

    #[test]
    fn missing_refresh_token_is_none_without_synthesizing_a_secret() {
        let storage = Arc::new(FakeStorage::missing());
        let adapter = SecureRefreshTokenStorePort::with_storage(storage);

        let result = block_on_ready(RefreshTokenReadPort::get(&adapter, ACCOUNT_UUID))
            .expect("missing is not a storage failure");

        assert!(result.is_none());
    }

    #[test]
    fn locked_read_is_redacted_and_keeps_only_the_internal_category() {
        let storage = Arc::new(FakeStorage::failing(
            SecureStoreErrorKind::AccessDeniedOrLocked,
        ));
        let adapter = SecureRefreshTokenStorePort::with_storage(storage);

        let error = block_on_ready(RefreshTokenReadPort::get(&adapter, ACCOUNT_UUID))
            .expect_err("locked store must fail closed");

        assert_eq!(error, RefreshTokenReadError::AccessDeniedOrLocked);
        let rendered = format!("{error:?} {error}");
        assert!(!rendered.contains(&ACCOUNT_UUID.to_string()));
        assert!(!rendered.contains("Keychain"));
        assert!(!rendered.contains("refresh"));
    }

    #[test]
    fn reads_preserve_typed_uuid_account_isolation() {
        let storage = Arc::new(FakeStorage::available());
        let adapter = SecureRefreshTokenStorePort::with_storage(storage.clone());

        block_on_ready(RefreshTokenReadPort::get(&adapter, ACCOUNT_UUID)).expect("account A");
        block_on_ready(RefreshTokenReadPort::get(&adapter, ACCOUNT_UUID_B)).expect("account B");

        assert_eq!(
            storage.operations.lock().expect("operations").as_slice(),
            [Operation::Get(ACCOUNT_UUID), Operation::Get(ACCOUNT_UUID_B)]
        );
    }

    #[test]
    fn typed_uuid_accounts_remain_isolated_during_delete() {
        let storage = Arc::new(FakeStorage::available());
        let adapter = SecureRefreshTokenStorePort::with_storage(storage.clone());

        block_on_ready(adapter.set(ACCOUNT_UUID, &SECRET)).expect("set account A");
        block_on_ready(adapter.set(ACCOUNT_UUID_B, &SECRET)).expect("set account B");
        block_on_ready(adapter.delete(ACCOUNT_UUID)).expect("delete account A");

        assert_eq!(
            storage.operations.lock().expect("operations").as_slice(),
            [
                Operation::Set(ACCOUNT_UUID),
                Operation::Set(ACCOUNT_UUID_B),
                Operation::Delete(ACCOUNT_UUID)
            ]
        );
    }

    #[test]
    fn denied_or_locked_and_unavailable_storage_errors_are_redacted() {
        for kind in [
            SecureStoreErrorKind::AccessDeniedOrLocked,
            SecureStoreErrorKind::Unavailable,
        ] {
            let storage = Arc::new(FakeStorage::failing(kind));
            let adapter = SecureRefreshTokenStorePort::with_storage(storage);

            let set_error = block_on_ready(adapter.set(ACCOUNT_UUID, &SECRET)).unwrap_err();
            let delete_error = block_on_ready(adapter.delete(ACCOUNT_UUID)).unwrap_err();

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
