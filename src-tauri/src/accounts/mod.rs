//! Normalized, non-secret account state used by the OAuth application service.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    future::Future,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    pin::Pin,
    sync::Mutex,
};
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const STATE_VERSION: u32 = 1;
const MAX_STATE_BYTES: usize = 1024 * 1024;
const MAX_ACCOUNTS: usize = 128;
const MAX_DISPLAY_NAME_BYTES: usize = 512;
const MAX_HISTORY_ID_BYTES: usize = 256;
const MAX_SYNC_AT_BYTES: usize = 64;

pub(crate) type AccountFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MailAccount {
    id: String,
    email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    pending_count: u64,
    active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_history_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_sync_at: Option<String>,
}

impl MailAccount {
    pub(crate) fn new(id: String, email: String, display_name: Option<String>) -> Self {
        Self {
            id,
            email,
            display_name,
            pending_count: 0,
            active: true,
            last_history_id: None,
            last_sync_at: None,
        }
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn email(&self) -> &str {
        &self.email
    }

    fn validate(&self) -> Result<(), AccountStateError> {
        let parsed_id = Uuid::parse_str(&self.id).map_err(|_| AccountStateError)?;
        if parsed_id.to_string() != self.id
            || !valid_email(&self.email)
            || self.display_name.as_ref().is_some_and(|name| {
                name.is_empty()
                    || name.len() > MAX_DISPLAY_NAME_BYTES
                    || name.chars().any(char::is_control)
            })
            || self
                .last_history_id
                .as_ref()
                .is_some_and(|value| invalid_bounded_text(value, MAX_HISTORY_ID_BYTES))
            || self
                .last_sync_at
                .as_ref()
                .is_some_and(|value| invalid_bounded_text(value, MAX_SYNC_AT_BYTES))
        {
            return Err(AccountStateError);
        }
        Ok(())
    }
}

impl fmt::Debug for MailAccount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MailAccount")
            .field("id", &"[redacted]")
            .field("email", &"[redacted]")
            .field(
                "display_name",
                &self.display_name.as_ref().map(|_| "[redacted]"),
            )
            .field("pending_count", &self.pending_count)
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AccountStateError;

impl fmt::Display for AccountStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("account state is unavailable")
    }
}

impl std::error::Error for AccountStateError {}

pub(crate) trait AccountStatePort: Send + Sync {
    fn commit<'a>(
        &'a self,
        account: MailAccount,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>>;
}

/// Durable V1 account state. The caller injects the complete `accounts.json` path.
pub(crate) struct JsonAccountState {
    path: PathBuf,
    accounts: Mutex<Vec<MailAccount>>,
}

impl JsonAccountState {
    /// Opens and validates an existing state document, or initializes empty state when absent.
    pub(crate) fn new(path: PathBuf) -> Result<Self, AccountStateError> {
        validate_state_path(&path)?;
        let accounts = load_document(&path)?;
        Ok(Self {
            path,
            accounts: Mutex::new(accounts),
        })
    }

    #[cfg(test)]
    fn snapshot(&self) -> Result<Vec<MailAccount>, AccountStateError> {
        self.accounts
            .lock()
            .map(|accounts| accounts.clone())
            .map_err(|_| AccountStateError)
    }
}

impl AccountStatePort for JsonAccountState {
    fn commit<'a>(
        &'a self,
        account: MailAccount,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        Box::pin(async move {
            account.validate()?;
            let mut accounts = self.accounts.lock().map_err(|_| AccountStateError)?;
            if accounts.iter().any(|existing| {
                existing.id == account.id || existing.email.eq_ignore_ascii_case(&account.email)
            }) {
                return Err(AccountStateError);
            }
            if accounts.len() >= MAX_ACCOUNTS {
                return Err(AccountStateError);
            }

            let mut next = accounts.clone();
            next.push(account.clone());
            write_document(&self.path, &next)?;
            *accounts = next;
            Ok(account)
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateDocument {
    version: u32,
    accounts: Vec<MailAccount>,
}

fn load_document(path: &Path) -> Result<Vec<MailAccount>, AccountStateError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(AccountStateError),
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_STATE_BYTES as u64
    {
        return Err(AccountStateError);
    }
    let mut file = open_read_no_follow(path)?;
    restrict_open_file(&file)?;
    let opened_metadata = file.metadata().map_err(|_| AccountStateError)?;
    if !opened_metadata.is_file() || opened_metadata.len() > MAX_STATE_BYTES as u64 {
        return Err(AccountStateError);
    }
    let mut bytes = Vec::with_capacity(opened_metadata.len() as usize);
    Read::by_ref(&mut file)
        .take((MAX_STATE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| AccountStateError)?;
    if bytes.len() > MAX_STATE_BYTES {
        return Err(AccountStateError);
    }
    let document: StateDocument = serde_json::from_slice(&bytes).map_err(|_| AccountStateError)?;
    validate_document(document)
}

fn validate_document(document: StateDocument) -> Result<Vec<MailAccount>, AccountStateError> {
    if document.version != STATE_VERSION || document.accounts.len() > MAX_ACCOUNTS {
        return Err(AccountStateError);
    }
    for (index, account) in document.accounts.iter().enumerate() {
        account.validate()?;
        if document.accounts[..index].iter().any(|existing| {
            existing.id == account.id || existing.email.eq_ignore_ascii_case(&account.email)
        }) {
            return Err(AccountStateError);
        }
    }
    Ok(document.accounts)
}

fn write_document(path: &Path, accounts: &[MailAccount]) -> Result<(), AccountStateError> {
    validate_state_path(path)?;
    if accounts.len() > MAX_ACCOUNTS {
        return Err(AccountStateError);
    }
    let bytes = serde_json::to_vec(&StateDocument {
        version: STATE_VERSION,
        accounts: accounts.to_vec(),
    })
    .map_err(|_| AccountStateError)?;
    if bytes.len() > MAX_STATE_BYTES {
        return Err(AccountStateError);
    }

    let parent = path.parent().ok_or(AccountStateError)?;
    ensure_parent(parent)?;
    reject_symlink(path)?;
    let temporary = unique_temporary_path(path)?;
    let result = (|| {
        let mut file = create_private_file(&temporary)?;
        file.write_all(&bytes).map_err(|_| AccountStateError)?;
        file.sync_all().map_err(|_| AccountStateError)?;
        reject_symlink(path)?;
        fs::rename(&temporary, path).map_err(|_| AccountStateError)?;
        sync_parent(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn validate_state_path(path: &Path) -> Result<(), AccountStateError> {
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(AccountStateError);
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                current.push(component.as_os_str());
            }
            _ => return Err(AccountStateError),
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Err(AccountStateError),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(AccountStateError),
        }
    }
    Ok(())
}

fn ensure_parent(parent: &Path) -> Result<(), AccountStateError> {
    fs::create_dir_all(parent).map_err(|_| AccountStateError)?;
    let metadata = fs::symlink_metadata(parent).map_err(|_| AccountStateError)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AccountStateError);
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<(), AccountStateError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(AccountStateError),
        Ok(metadata) if !metadata.is_file() => Err(AccountStateError),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(AccountStateError),
    }
}

fn unique_temporary_path(path: &Path) -> Result<PathBuf, AccountStateError> {
    let parent = path.parent().ok_or(AccountStateError)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(AccountStateError)?;
    for _ in 0..8 {
        let mut nonce = [0_u8; 8];
        getrandom::fill(&mut nonce).map_err(|_| AccountStateError)?;
        let suffix = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let candidate = parent.join(format!(".{name}.{suffix}.tmp"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(AccountStateError)
}

fn open_read_no_follow(path: &Path) -> Result<File, AccountStateError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(no_follow_flag());
    options.open(path).map_err(|_| AccountStateError)
}

fn create_private_file(path: &Path) -> Result<File, AccountStateError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(no_follow_flag());
    options.open(path).map_err(|_| AccountStateError)
}

fn restrict_open_file(file: &File) -> Result<(), AccountStateError> {
    #[cfg(unix)]
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| AccountStateError)?;
    Ok(())
}

fn sync_parent(parent: &Path) -> Result<(), AccountStateError> {
    #[cfg(unix)]
    open_read_no_follow(parent)?
        .sync_all()
        .map_err(|_| AccountStateError)?;
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "android"))]
const fn no_follow_flag() -> i32 {
    0x20_000
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
const fn no_follow_flag() -> i32 {
    0x100
}

fn valid_email(email: &str) -> bool {
    if email.is_empty()
        || email.len() > 320
        || email.trim() != email
        || email.bytes().any(|byte| byte.is_ascii_control())
    {
        return false;
    }
    let mut parts = email.split('@');
    parts.next().is_some_and(|local| !local.is_empty())
        && parts.next().is_some_and(|domain| !domain.is_empty())
        && parts.next().is_none()
}

fn invalid_bounded_text(value: &str, maximum: usize) -> bool {
    value.is_empty() || value.len() > maximum || value.chars().any(char::is_control)
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct InMemoryAccountState {
    accounts: Mutex<Vec<MailAccount>>,
}

/// Compatibility placeholder for the Lead-owned command wiring.
///
/// It deliberately fails closed so production cannot report a successful account commit until
/// the command constructs `JsonAccountState` from the Tauri app-data path.
#[cfg(not(test))]
#[derive(Default)]
pub(crate) struct InMemoryAccountState {
    _disabled: (),
}

#[cfg(not(test))]
impl AccountStatePort for InMemoryAccountState {
    fn commit<'a>(
        &'a self,
        _account: MailAccount,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        Box::pin(async { Err(AccountStateError) })
    }
}

#[cfg(test)]
impl AccountStatePort for InMemoryAccountState {
    fn commit<'a>(
        &'a self,
        account: MailAccount,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        Box::pin(async move {
            let mut accounts = self.accounts.lock().map_err(|_| AccountStateError)?;
            if accounts.iter().any(|existing| {
                existing.id == account.id || existing.email.eq_ignore_ascii_case(&account.email)
            }) {
                return Err(AccountStateError);
            }
            accounts.push(account.clone());
            Ok(account)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "zerochi-account-state-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("create test directory");
            Self(path.canonicalize().expect("canonical test directory"))
        }

        fn state_path(&self) -> PathBuf {
            self.0.join("accounts.json")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn account(id: &str, email: &str) -> MailAccount {
        MailAccount::new(id.into(), email.into(), Some("Fixture Person".into()))
    }

    fn commit(
        state: &impl AccountStatePort,
        account: MailAccount,
    ) -> Result<MailAccount, AccountStateError> {
        tauri::async_runtime::block_on(state.commit(account))
    }

    #[test]
    fn normalized_account_matches_the_shared_contract_without_secret_fields() {
        let account = account(
            "00000000-0000-4000-8000-000000000001",
            "person@example.test",
        );
        let value = serde_json::to_value(&account).expect("serialize account");

        assert_eq!(value["pendingCount"], 0);
        assert_eq!(value["active"], true);
        assert!(value.get("lastHistoryId").is_none());
        assert!(value.get("lastSyncAt").is_none());
        assert!(value.get("accessToken").is_none());
        assert!(value.get("refreshToken").is_none());
    }

    #[test]
    fn debug_and_errors_do_not_disclose_identity_or_paths() {
        let account = account(
            "00000000-0000-4000-8000-000000000001",
            "person@example.test",
        );
        let rendered = format!("{account:?}");

        assert!(!rendered.contains(account.id()));
        assert!(!rendered.contains(account.email()));
        assert!(!rendered.contains("Fixture Person"));
        assert_eq!(
            AccountStateError.to_string(),
            "account state is unavailable"
        );
    }

    #[test]
    fn durable_state_survives_restart_and_rejects_id_or_email_duplicates() {
        let directory = TestDirectory::new();
        let path = directory.state_path();
        let first = account(
            "00000000-0000-4000-8000-000000000001",
            "person@example.test",
        );
        let state = JsonAccountState::new(path.clone()).expect("open state");
        commit(&state, first.clone()).expect("commit account");
        drop(state);

        let restarted = JsonAccountState::new(path).expect("reload state");
        assert_eq!(restarted.snapshot().expect("snapshot"), vec![first.clone()]);
        assert!(commit(&restarted, first).is_err());
        assert!(commit(
            &restarted,
            account(
                "00000000-0000-4000-8000-000000000002",
                "PERSON@example.test"
            )
        )
        .is_err());
    }

    #[test]
    fn persisted_document_is_versioned_bounded_and_contains_no_token_fields() {
        let directory = TestDirectory::new();
        let path = directory.state_path();
        let state = JsonAccountState::new(path.clone()).expect("open state");
        commit(
            &state,
            account(
                "00000000-0000-4000-8000-000000000001",
                "person@example.test",
            ),
        )
        .expect("commit account");
        let contents = fs::read_to_string(path).expect("read state");

        assert!(contents.len() <= MAX_STATE_BYTES);
        assert!(contents.contains("\"version\":1"));
        assert!(!contents.to_ascii_lowercase().contains("token"));
    }

    #[test]
    fn rejects_corrupt_oversized_unknown_schema_and_secret_shaped_fields() {
        for contents in [
            b"not json".to_vec(),
            br#"{"version":2,"accounts":[]}"#.to_vec(),
            br#"{"version":1,"accounts":[{"id":"00000000-0000-4000-8000-000000000001","email":"person@example.test","pendingCount":0,"active":true,"refreshToken":"fixture"}]}"#.to_vec(),
            vec![b'x'; MAX_STATE_BYTES + 1],
        ] {
            let directory = TestDirectory::new();
            let path = directory.state_path();
            fs::write(&path, contents).expect("write fixture");
            assert!(JsonAccountState::new(path).is_err());
        }
    }

    #[test]
    fn rejects_relative_directory_and_parent_file_paths() {
        assert!(JsonAccountState::new(PathBuf::from("accounts.json")).is_err());

        let directory = TestDirectory::new();
        assert!(JsonAccountState::new(directory.0.clone()).is_err());
        let parent_file = directory.0.join("not-a-directory");
        fs::write(&parent_file, b"fixture").expect("write parent file");
        assert!(JsonAccountState::new(parent_file.join("accounts.json")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn creates_private_file_and_rejects_target_or_parent_symlinks() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let directory = TestDirectory::new();
        let path = directory.state_path();
        let state = JsonAccountState::new(path.clone()).expect("open state");
        commit(
            &state,
            account(
                "00000000-0000-4000-8000-000000000001",
                "person@example.test",
            ),
        )
        .expect("commit account");
        assert_eq!(
            fs::metadata(&path).expect("metadata").permissions().mode() & 0o777,
            0o600
        );

        let target = directory.0.join("target.json");
        fs::write(&target, br#"{"version":1,"accounts":[]}"#).expect("write target");
        let link = directory.0.join("linked.json");
        symlink(&target, &link).expect("create file symlink");
        assert!(JsonAccountState::new(link).is_err());

        let actual_parent = directory.0.join("actual-parent");
        fs::create_dir(&actual_parent).expect("create actual parent");
        let linked_parent = directory.0.join("linked-parent");
        symlink(&actual_parent, &linked_parent).expect("create parent symlink");
        assert!(JsonAccountState::new(linked_parent.join("accounts.json")).is_err());
    }

    #[test]
    fn failed_write_does_not_mutate_the_loaded_snapshot() {
        let directory = TestDirectory::new();
        let parent = directory.0.join("parent");
        fs::create_dir(&parent).expect("create parent");
        let state = JsonAccountState::new(parent.join("accounts.json")).expect("initial load");
        fs::remove_dir(&parent).expect("remove parent");
        fs::write(&parent, b"fixture").expect("replace parent with file");
        assert!(commit(
            &state,
            account(
                "00000000-0000-4000-8000-000000000001",
                "person@example.test"
            )
        )
        .is_err());
        assert!(state.snapshot().expect("snapshot").is_empty());
    }

    #[test]
    fn test_fake_is_not_a_live_adapter_and_rejects_duplicates() {
        let state = InMemoryAccountState::default();
        let account = account(
            "00000000-0000-4000-8000-000000000001",
            "person@example.test",
        );
        assert_eq!(
            commit(&state, account.clone()).expect("first commit"),
            account
        );
        assert!(commit(&state, account).is_err());
    }
}
