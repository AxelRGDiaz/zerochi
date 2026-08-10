//! Rust-only refresh-token storage backed by the macOS Keychain.
//!
//! Secrets returned by this module are zeroized on drop and this module's
//! errors intentionally discard all provider details. Nothing here is a
//! Tauri command or a serializable DTO.

// The Lead will wire this foundation into `add_account` after the auth and
// secure-store branches land together.
#![allow(dead_code)]

mod auth_port;

#[allow(unused_imports)]
pub(crate) use auth_port::SecureRefreshTokenStorePort;

use std::{fmt, sync::Arc};

use uuid::Uuid;
use zeroize::Zeroizing;

pub(crate) const REFRESH_TOKEN_SERVICE: &str = "com.zerochi.desktop.oauth.refresh-token";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeploymentMode {
    SignedProvisioned,
    UnsignedDevelopment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendKind {
    ProtectedData,
    LegacyKeychain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecureStoreErrorKind {
    AccessDeniedOrLocked,
    InvalidSecret,
    Unavailable,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct SecureStoreError {
    kind: SecureStoreErrorKind,
}

impl SecureStoreError {
    pub(crate) fn kind(self) -> SecureStoreErrorKind {
        self.kind
    }

    fn access_denied_or_locked() -> Self {
        Self {
            kind: SecureStoreErrorKind::AccessDeniedOrLocked,
        }
    }

    fn invalid_secret() -> Self {
        Self {
            kind: SecureStoreErrorKind::InvalidSecret,
        }
    }

    fn unavailable() -> Self {
        Self {
            kind: SecureStoreErrorKind::Unavailable,
        }
    }
}

impl fmt::Debug for SecureStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecureStoreError")
            .field("kind", &self.kind)
            .finish()
    }
}

impl fmt::Display for SecureStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self.kind {
            SecureStoreErrorKind::AccessDeniedOrLocked => {
                "secure storage is locked or access was denied"
            }
            SecureStoreErrorKind::InvalidSecret => "secure storage rejected an invalid secret",
            SecureStoreErrorKind::Unavailable => "secure storage is unavailable",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for SecureStoreError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendError {
    AccessDenied,
    Locked,
    Missing,
    Unavailable,
}

trait SecretBackend: Send + Sync {
    fn set(&self, service: &str, account: &str, secret: &[u8]) -> Result<(), BackendError>;
    fn get(&self, service: &str, account: &str) -> Result<Zeroizing<Vec<u8>>, BackendError>;
    fn delete(&self, service: &str, account: &str) -> Result<(), BackendError>;
}

#[derive(Clone)]
pub(crate) struct SecureStore {
    backend: Arc<dyn SecretBackend>,
}

impl fmt::Debug for SecureStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecureStore")
            .field("backend", &"[redacted]")
            .finish()
    }
}

impl SecureStore {
    /// Creates the native store for an explicitly identified deployment.
    ///
    /// The legacy Keychain is never a runtime fallback. It is available only
    /// to debug builds whose caller explicitly identifies them as unsigned
    /// development builds.
    pub(crate) fn new(mode: DeploymentMode) -> Result<Self, SecureStoreError> {
        let kind = select_backend(mode)?;
        Ok(Self {
            backend: native_backend(kind)?,
        })
    }

    #[cfg(test)]
    fn with_backend(backend: Arc<dyn SecretBackend>) -> Self {
        Self { backend }
    }

    pub(crate) fn set_refresh_token(
        &self,
        account_id: Uuid,
        refresh_token: &[u8],
    ) -> Result<(), SecureStoreError> {
        if refresh_token.is_empty() {
            return Err(SecureStoreError::invalid_secret());
        }

        let account = account_key(account_id);
        let secret = Zeroizing::new(refresh_token.to_vec());
        self.backend
            .set(REFRESH_TOKEN_SERVICE, &account, secret.as_slice())
            .map_err(map_backend_error)
    }

    pub(crate) fn get_refresh_token(
        &self,
        account_id: Uuid,
    ) -> Result<Option<Zeroizing<Vec<u8>>>, SecureStoreError> {
        let account = account_key(account_id);
        match self.backend.get(REFRESH_TOKEN_SERVICE, &account) {
            Ok(secret) => Ok(Some(secret)),
            Err(BackendError::Missing) => Ok(None),
            Err(error) => Err(map_backend_error(error)),
        }
    }

    pub(crate) fn delete_refresh_token(&self, account_id: Uuid) -> Result<(), SecureStoreError> {
        let account = account_key(account_id);
        match self.backend.delete(REFRESH_TOKEN_SERVICE, &account) {
            Ok(()) | Err(BackendError::Missing) => Ok(()),
            Err(error) => Err(map_backend_error(error)),
        }
    }
}

fn account_key(account_id: Uuid) -> String {
    account_id.hyphenated().to_string()
}

fn select_backend(mode: DeploymentMode) -> Result<BackendKind, SecureStoreError> {
    match mode {
        DeploymentMode::SignedProvisioned => Ok(BackendKind::ProtectedData),
        DeploymentMode::UnsignedDevelopment if cfg!(debug_assertions) => {
            Ok(BackendKind::LegacyKeychain)
        }
        DeploymentMode::UnsignedDevelopment => Err(SecureStoreError::unavailable()),
    }
}

fn map_backend_error(error: BackendError) -> SecureStoreError {
    match error {
        BackendError::AccessDenied | BackendError::Locked => {
            SecureStoreError::access_denied_or_locked()
        }
        BackendError::Missing | BackendError::Unavailable => SecureStoreError::unavailable(),
    }
}

#[cfg(target_os = "macos")]
fn native_backend(kind: BackendKind) -> Result<Arc<dyn SecretBackend>, SecureStoreError> {
    Ok(Arc::new(KeyringBackend::new(kind)?))
}

#[cfg(not(target_os = "macos"))]
fn native_backend(_kind: BackendKind) -> Result<Arc<dyn SecretBackend>, SecureStoreError> {
    Err(SecureStoreError::unavailable())
}

#[cfg(target_os = "macos")]
struct KeyringBackend {
    store: Arc<keyring_core::CredentialStore>,
}

#[cfg(target_os = "macos")]
impl KeyringBackend {
    fn new(kind: BackendKind) -> Result<Self, SecureStoreError> {
        let store: Arc<keyring_core::CredentialStore> = match kind {
            BackendKind::ProtectedData => apple_native_keyring_store::protected::Store::new()
                .map_err(map_keyring_init_error)?,
            BackendKind::LegacyKeychain => apple_native_keyring_store::keychain::Store::new()
                .map_err(map_keyring_init_error)?,
        };
        Ok(Self { store })
    }

    fn entry(&self, service: &str, account: &str) -> Result<keyring_core::Entry, BackendError> {
        self.store
            .build(service, account, None)
            .map_err(map_keyring_error)
    }
}

#[cfg(target_os = "macos")]
impl SecretBackend for KeyringBackend {
    fn set(&self, service: &str, account: &str, secret: &[u8]) -> Result<(), BackendError> {
        self.entry(service, account)?
            .set_secret(secret)
            .map_err(map_keyring_error)
    }

    fn get(&self, service: &str, account: &str) -> Result<Zeroizing<Vec<u8>>, BackendError> {
        self.entry(service, account)?
            .get_secret()
            .map(Zeroizing::new)
            .map_err(map_keyring_error)
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), BackendError> {
        self.entry(service, account)?
            .delete_credential()
            .map_err(map_keyring_error)
    }
}

#[cfg(target_os = "macos")]
fn map_keyring_init_error(error: keyring_core::Error) -> SecureStoreError {
    map_backend_error(map_keyring_error(error))
}

#[cfg(target_os = "macos")]
fn map_keyring_error(error: keyring_core::Error) -> BackendError {
    use keyring_core::Error;
    use zeroize::Zeroize;

    match error {
        Error::NoEntry => BackendError::Missing,
        Error::NoStorageAccess(_) => BackendError::AccessDenied,
        Error::BadEncoding(mut bytes) => {
            bytes.zeroize();
            BackendError::Unavailable
        }
        Error::BadDataFormat(mut bytes, _) => {
            bytes.zeroize();
            BackendError::Unavailable
        }
        _ => BackendError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};
    use zeroize::Zeroize;

    const ACCOUNT_A: Uuid = Uuid::from_u128(0xaaaaaaaa_aaaa_4aaa_8aaa_aaaaaaaaaaaa);
    const ACCOUNT_B: Uuid = Uuid::from_u128(0xbbbbbbbb_bbbb_4bbb_8bbb_bbbbbbbbbbbb);
    const SECRET_A: [u8; 32] = [0xA5; 32];
    const SECRET_B: [u8; 32] = [0x5A; 32];
    type EntryKey = (String, String);
    type Entries = HashMap<EntryKey, Zeroizing<Vec<u8>>>;

    #[derive(Default)]
    struct FakeBackend {
        entries: Mutex<Entries>,
        failure: Mutex<Option<BackendError>>,
    }

    impl FakeBackend {
        fn failing(error: BackendError) -> Self {
            Self {
                entries: Mutex::new(HashMap::new()),
                failure: Mutex::new(Some(error)),
            }
        }

        fn requested_failure(&self) -> Result<Option<BackendError>, BackendError> {
            self.failure
                .lock()
                .map(|failure| *failure)
                .map_err(|_| BackendError::Unavailable)
        }
    }

    impl SecretBackend for FakeBackend {
        fn set(&self, service: &str, account: &str, secret: &[u8]) -> Result<(), BackendError> {
            if let Some(error) = self.requested_failure()? {
                return Err(error);
            }
            self.entries
                .lock()
                .map_err(|_| BackendError::Unavailable)?
                .insert(
                    (service.to_owned(), account.to_owned()),
                    Zeroizing::new(secret.to_vec()),
                );
            Ok(())
        }

        fn get(&self, service: &str, account: &str) -> Result<Zeroizing<Vec<u8>>, BackendError> {
            if let Some(error) = self.requested_failure()? {
                return Err(error);
            }
            self.entries
                .lock()
                .map_err(|_| BackendError::Unavailable)?
                .get(&(service.to_owned(), account.to_owned()))
                .cloned()
                .ok_or(BackendError::Missing)
        }

        fn delete(&self, service: &str, account: &str) -> Result<(), BackendError> {
            if let Some(error) = self.requested_failure()? {
                return Err(error);
            }
            self.entries
                .lock()
                .map_err(|_| BackendError::Unavailable)?
                .remove(&(service.to_owned(), account.to_owned()))
                .map(drop)
                .ok_or(BackendError::Missing)
        }
    }

    fn fake_store() -> SecureStore {
        SecureStore::with_backend(Arc::new(FakeBackend::default()))
    }

    #[test]
    fn creates_and_reads_refresh_token() {
        let store = fake_store();

        store.set_refresh_token(ACCOUNT_A, &SECRET_A).unwrap();
        let secret = store.get_refresh_token(ACCOUNT_A).unwrap().unwrap();

        assert_eq!(secret.as_slice(), SECRET_A);
    }

    #[test]
    fn setting_existing_account_updates_in_place() {
        let store = fake_store();

        store.set_refresh_token(ACCOUNT_A, &SECRET_A).unwrap();
        store.set_refresh_token(ACCOUNT_A, &SECRET_B).unwrap();

        assert_eq!(
            store
                .get_refresh_token(ACCOUNT_A)
                .unwrap()
                .unwrap()
                .as_slice(),
            SECRET_B
        );
    }

    #[test]
    fn delete_is_idempotent_and_missing_read_is_none() {
        let store = fake_store();
        store.set_refresh_token(ACCOUNT_A, &SECRET_A).unwrap();

        store.delete_refresh_token(ACCOUNT_A).unwrap();
        store.delete_refresh_token(ACCOUNT_A).unwrap();

        assert!(store.get_refresh_token(ACCOUNT_A).unwrap().is_none());
    }

    #[test]
    fn account_uuids_are_isolated_under_the_fixed_service() {
        let backend = Arc::new(FakeBackend::default());
        let store = SecureStore::with_backend(backend.clone());

        store.set_refresh_token(ACCOUNT_A, &SECRET_A).unwrap();
        store.set_refresh_token(ACCOUNT_B, &SECRET_B).unwrap();
        store.delete_refresh_token(ACCOUNT_A).unwrap();

        assert!(store.get_refresh_token(ACCOUNT_A).unwrap().is_none());
        assert_eq!(
            store
                .get_refresh_token(ACCOUNT_B)
                .unwrap()
                .unwrap()
                .as_slice(),
            SECRET_B
        );

        let entries = backend.entries.lock().unwrap();
        assert!(entries
            .keys()
            .all(|(service, _)| service == REFRESH_TOKEN_SERVICE));
        assert!(entries
            .keys()
            .any(|(_, account)| account == &ACCOUNT_B.hyphenated().to_string()));
    }

    #[test]
    fn denied_backend_fails_closed_with_redacted_error() {
        assert_redacted_failure(BackendError::AccessDenied);
    }

    #[test]
    fn locked_backend_fails_closed_with_redacted_error() {
        assert_redacted_failure(BackendError::Locked);
    }

    fn assert_redacted_failure(backend_error: BackendError) {
        let store = SecureStore::with_backend(Arc::new(FakeBackend::failing(backend_error)));
        let errors = [
            store.set_refresh_token(ACCOUNT_A, &SECRET_A).unwrap_err(),
            store.get_refresh_token(ACCOUNT_A).unwrap_err(),
            store.delete_refresh_token(ACCOUNT_A).unwrap_err(),
        ];

        for error in errors {
            assert_eq!(error.kind(), SecureStoreErrorKind::AccessDeniedOrLocked);
            assert_eq!(
                error.to_string(),
                "secure storage is locked or access was denied"
            );
            assert_eq!(
                format!("{error:?}"),
                "SecureStoreError { kind: AccessDeniedOrLocked }"
            );
        }
    }

    #[test]
    fn unavailable_backend_fails_closed_without_provider_details() {
        let store =
            SecureStore::with_backend(Arc::new(FakeBackend::failing(BackendError::Unavailable)));
        let error = store.get_refresh_token(ACCOUNT_A).unwrap_err();

        assert_eq!(error.kind(), SecureStoreErrorKind::Unavailable);
        assert_eq!(error.to_string(), "secure storage is unavailable");
    }

    #[test]
    fn signed_provisioned_build_selects_protected_data() {
        assert_eq!(
            select_backend(DeploymentMode::SignedProvisioned).unwrap(),
            BackendKind::ProtectedData
        );
    }

    #[test]
    fn unsigned_development_selects_legacy_only_in_debug_builds() {
        let selection = select_backend(DeploymentMode::UnsignedDevelopment);
        if cfg!(debug_assertions) {
            assert_eq!(selection.unwrap(), BackendKind::LegacyKeychain);
        } else {
            assert_eq!(
                selection.unwrap_err().kind(),
                SecureStoreErrorKind::Unavailable
            );
        }
    }

    #[test]
    fn debug_output_redacts_backend_identity() {
        let store = fake_store();
        assert_eq!(
            format!("{store:?}"),
            "SecureStore { backend: \"[redacted]\" }"
        );
    }

    #[test]
    fn empty_secret_is_rejected_before_backend_access() {
        let store = fake_store();
        let error = store.set_refresh_token(ACCOUNT_A, &[]).unwrap_err();

        assert_eq!(error.kind(), SecureStoreErrorKind::InvalidSecret);
        assert!(store.get_refresh_token(ACCOUNT_A).unwrap().is_none());
    }

    #[test]
    fn returned_secret_buffer_can_be_scrubbed_in_place() {
        let store = fake_store();
        store.set_refresh_token(ACCOUNT_A, &SECRET_A).unwrap();
        let mut secret = store.get_refresh_token(ACCOUNT_A).unwrap().unwrap();

        secret.zeroize();

        assert!(secret.iter().all(|byte| *byte == 0));
    }
}
