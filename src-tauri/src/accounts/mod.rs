//! Normalized, non-secret account state used by the OAuth application service.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
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

const STATE_VERSION: u32 = 4;
// 25k accepted IDs can occupy ~6.5 MiB at the 256-byte identifier bound. Reserve another MiB
// for the account/message envelope while retaining a hard cap on durable non-secret state.
const MAX_STATE_BYTES: usize = 8 * 1024 * 1024;
const MAX_ACCOUNTS: usize = 128;
const MAX_DISPLAY_NAME_BYTES: usize = 512;
const MAX_HISTORY_ID_BYTES: usize = 256;
const MAX_SYNC_AT_BYTES: usize = 64;
const MAX_MESSAGES: usize = 100_000;
const MAX_MESSAGE_ID_BYTES: usize = 256;
const MAX_HEADER_BYTES: usize = 2_048;
const MAX_UNREAD_MESSAGE_IDS: usize = 25_000;
const MAX_UNREAD_SET_SERIALIZED_BYTES: usize = MAX_UNREAD_MESSAGE_IDS * (MAX_MESSAGE_ID_BYTES + 3);
const UNREAD_SET_STATE_RESERVE_BYTES: usize = 1024 * 1024;
const _: () =
    assert!(MAX_UNREAD_SET_SERIALIZED_BYTES + UNREAD_SET_STATE_RESERVE_BYTES <= MAX_STATE_BYTES);

pub(crate) type AccountFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MailAccount {
    id: String,
    email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    pending_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    inbox_unread_count: Option<u64>,
    active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_history_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_sync_at: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MailMessage {
    id: String,
    thread_id: String,
    account_id: String,
    sender: String,
    subject: String,
    received_at: String,
    detected_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    opened_from_zerochi_at: Option<String>,
}

impl MailMessage {
    pub(crate) fn detected(
        id: String,
        thread_id: String,
        account_id: String,
        sender: String,
        subject: String,
        received_at: String,
        detected_at: String,
    ) -> Result<Self, AccountStateError> {
        let message = Self {
            id,
            thread_id,
            account_id,
            sender,
            subject,
            received_at,
            detected_at,
            opened_from_zerochi_at: None,
        };
        message.validate()?;
        Ok(message)
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn thread_id(&self) -> &str {
        &self.thread_id
    }

    pub(crate) fn account_id(&self) -> &str {
        &self.account_id
    }

    pub(crate) fn sender(&self) -> &str {
        &self.sender
    }

    pub(crate) fn subject(&self) -> &str {
        &self.subject
    }

    fn validate(&self) -> Result<(), AccountStateError> {
        if invalid_identifier(&self.id)
            || invalid_identifier(&self.thread_id)
            || !Uuid::parse_str(&self.account_id).is_ok_and(|id| id.to_string() == self.account_id)
            || invalid_header(&self.sender)
            || invalid_header(&self.subject)
            || invalid_timestamp(&self.received_at)
            || invalid_timestamp(&self.detected_at)
            || self
                .opened_from_zerochi_at
                .as_ref()
                .is_some_and(|value| invalid_timestamp(value))
        {
            return Err(AccountStateError);
        }
        Ok(())
    }
}

impl fmt::Debug for MailMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MailMessage")
            .field("id", &"[redacted]")
            .field("thread_id", &"[redacted]")
            .field("account_id", &"[redacted]")
            .field("opened", &self.opened_from_zerochi_at.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MailStateSnapshot {
    pub(crate) accounts: Vec<MailAccount>,
    pub(crate) messages: Vec<MailMessage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IncrementalCommitOutcome {
    pub(crate) account: MailAccount,
    pub(crate) inserted_messages: Vec<MailMessage>,
}

impl MailAccount {
    pub(crate) fn new(id: String, email: String, display_name: Option<String>) -> Self {
        Self {
            id,
            email,
            display_name,
            pending_count: 0,
            inbox_unread_count: None,
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

    pub(crate) fn pending_count(&self) -> u64 {
        self.pending_count
    }

    pub(crate) fn inbox_unread_count(&self) -> Option<u64> {
        self.inbox_unread_count
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    pub(crate) fn last_history_id(&self) -> Option<&str> {
        self.last_history_id.as_deref()
    }

    fn with_sync_state(
        mut self,
        history_id: String,
        synced_at: String,
        inbox_unread_count: u64,
    ) -> Result<Self, AccountStateError> {
        self.last_history_id = Some(history_id);
        self.last_sync_at = Some(synced_at);
        self.inbox_unread_count = Some(inbox_unread_count);
        self.validate()?;
        Ok(self)
    }

    fn clear_provider_sync_state(&mut self) {
        self.inbox_unread_count = None;
        self.last_history_id = None;
        self.last_sync_at = None;
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
            || self.last_history_id.as_ref().is_some_and(|value| {
                invalid_bounded_text(value, MAX_HISTORY_ID_BYTES)
                    || !value.bytes().all(|byte| byte.is_ascii_digit())
            })
            || self.last_sync_at.as_ref().is_some_and(|value| {
                invalid_bounded_text(value, MAX_SYNC_AT_BYTES)
                    || chrono::DateTime::parse_from_rfc3339(value).is_err()
            })
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
            .field("inbox_unread_count", &self.inbox_unread_count)
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
    fn snapshot(&self) -> Result<Vec<MailAccount>, AccountStateError>;

    fn mail_snapshot(&self) -> Result<MailStateSnapshot, AccountStateError> {
        self.snapshot().map(|accounts| MailStateSnapshot {
            accounts,
            messages: Vec::new(),
        })
    }

    fn commit<'a>(
        &'a self,
        account: MailAccount,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>>;

    fn update_sync_cursor<'a>(
        &'a self,
        account_id: &'a str,
        history_id: String,
        synced_at: String,
        inbox_unread_count: u64,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>>;

    fn unread_message_ids(&self, _account_id: &str) -> Result<Vec<String>, AccountStateError> {
        Err(AccountStateError)
    }

    fn commit_baseline<'a>(
        &'a self,
        _account_id: &'a str,
        _expected_cursor: Option<&'a str>,
        _profile_email: &'a str,
        _next_cursor: String,
        _synced_at: String,
        _unread_message_ids: Vec<String>,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        Box::pin(async { Err(AccountStateError) })
    }

    fn commit_inbox_unread<'a>(
        &'a self,
        _account_id: &'a str,
        _inbox_unread_count: u64,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        Box::pin(async { Err(AccountStateError) })
    }

    fn commit_incremental<'a>(
        &'a self,
        _account_id: &'a str,
        _expected_cursor: &'a str,
        _next_cursor: String,
        _synced_at: String,
        _messages: Vec<MailMessage>,
        _unread_message_ids: Vec<String>,
    ) -> AccountFuture<'a, Result<IncrementalCommitOutcome, AccountStateError>> {
        Box::pin(async { Err(AccountStateError) })
    }

    fn remove_account<'a>(
        &'a self,
        _account_id: Uuid,
    ) -> AccountFuture<'a, Result<MailStateSnapshot, AccountStateError>> {
        Box::pin(async { Err(AccountStateError) })
    }
}

#[derive(Clone)]
struct NormalizedState {
    accounts: Vec<MailAccount>,
    messages: Vec<MailMessage>,
    unread_message_ids: BTreeMap<String, BTreeSet<String>>,
}

/// Durable versioned account state. The caller injects the complete `accounts.json` path.
pub(crate) struct JsonAccountState {
    path: PathBuf,
    state: Mutex<NormalizedState>,
}

impl JsonAccountState {
    /// Opens and validates an existing state document, or initializes empty state when absent.
    pub(crate) fn new(path: PathBuf) -> Result<Self, AccountStateError> {
        validate_state_path(&path)?;
        let state = load_document(&path)?;
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    fn loaded_snapshot(&self) -> Result<Vec<MailAccount>, AccountStateError> {
        self.state
            .lock()
            .map(|state| state.accounts.clone())
            .map_err(|_| AccountStateError)
    }
}

impl AccountStatePort for JsonAccountState {
    fn snapshot(&self) -> Result<Vec<MailAccount>, AccountStateError> {
        self.loaded_snapshot()
    }

    fn mail_snapshot(&self) -> Result<MailStateSnapshot, AccountStateError> {
        self.state
            .lock()
            .map(|state| MailStateSnapshot {
                accounts: state.accounts.clone(),
                messages: state.messages.clone(),
            })
            .map_err(|_| AccountStateError)
    }

    fn commit<'a>(
        &'a self,
        account: MailAccount,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        Box::pin(async move {
            account.validate()?;
            let mut state = self.state.lock().map_err(|_| AccountStateError)?;
            if state.accounts.iter().any(|existing| {
                existing.id == account.id || existing.email.eq_ignore_ascii_case(&account.email)
            }) {
                return Err(AccountStateError);
            }
            if state.accounts.len() >= MAX_ACCOUNTS {
                return Err(AccountStateError);
            }

            let mut next = state.clone();
            next.accounts.push(account.clone());
            write_document(&self.path, &next)?;
            *state = next;
            Ok(account)
        })
    }

    fn update_sync_cursor<'a>(
        &'a self,
        account_id: &'a str,
        history_id: String,
        synced_at: String,
        inbox_unread_count: u64,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        Box::pin(async move {
            if inbox_unread_count != 0 {
                return Err(AccountStateError);
            }
            let mut state = self.state.lock().map_err(|_| AccountStateError)?;
            let index = state
                .accounts
                .iter()
                .position(|account| account.id == account_id)
                .ok_or(AccountStateError)?;
            let updated = state.accounts[index].clone().with_sync_state(
                history_id,
                synced_at,
                inbox_unread_count,
            )?;
            let mut next = state.clone();
            next.accounts[index] = updated.clone();
            next.unread_message_ids
                .insert(account_id.to_owned(), BTreeSet::new());
            write_document(&self.path, &next)?;
            *state = next;
            Ok(updated)
        })
    }

    fn unread_message_ids(&self, account_id: &str) -> Result<Vec<String>, AccountStateError> {
        let state = self.state.lock().map_err(|_| AccountStateError)?;
        let account = state
            .accounts
            .iter()
            .find(|account| account.id == account_id && account.active)
            .ok_or(AccountStateError)?;
        if account.last_history_id.is_none() {
            return Err(AccountStateError);
        }
        state
            .unread_message_ids
            .get(account_id)
            .map(|ids| ids.iter().cloned().collect())
            .ok_or(AccountStateError)
    }

    fn commit_baseline<'a>(
        &'a self,
        account_id: &'a str,
        expected_cursor: Option<&'a str>,
        profile_email: &'a str,
        next_cursor: String,
        synced_at: String,
        unread_message_ids: Vec<String>,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        Box::pin(async move {
            let unread_message_ids = validated_unread_set(unread_message_ids)?;
            let unread_count =
                u64::try_from(unread_message_ids.len()).map_err(|_| AccountStateError)?;
            let mut state = self.state.lock().map_err(|_| AccountStateError)?;
            let index = state
                .accounts
                .iter()
                .position(|account| account.id == account_id && account.active)
                .ok_or(AccountStateError)?;
            if state.accounts[index].last_history_id.as_deref() != expected_cursor
                || !state.accounts[index]
                    .email
                    .eq_ignore_ascii_case(profile_email)
            {
                return Err(AccountStateError);
            }
            let updated = state.accounts[index].clone().with_sync_state(
                next_cursor,
                synced_at,
                unread_count,
            )?;
            let mut next = state.clone();
            next.accounts[index] = updated.clone();
            next.unread_message_ids
                .insert(account_id.to_owned(), unread_message_ids);
            write_document(&self.path, &next)?;
            *state = next;
            Ok(updated)
        })
    }

    fn commit_inbox_unread<'a>(
        &'a self,
        account_id: &'a str,
        inbox_unread_count: u64,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        let _ = (account_id, inbox_unread_count);
        Box::pin(async { Err(AccountStateError) })
    }

    fn commit_incremental<'a>(
        &'a self,
        account_id: &'a str,
        expected_cursor: &'a str,
        next_cursor: String,
        synced_at: String,
        messages: Vec<MailMessage>,
        unread_message_ids: Vec<String>,
    ) -> AccountFuture<'a, Result<IncrementalCommitOutcome, AccountStateError>> {
        Box::pin(async move {
            let unread_message_ids = validated_unread_set(unread_message_ids)?;
            let unread_count =
                u64::try_from(unread_message_ids.len()).map_err(|_| AccountStateError)?;
            let mut state = self.state.lock().map_err(|_| AccountStateError)?;
            let index = state
                .accounts
                .iter()
                .position(|account| account.id == account_id)
                .ok_or(AccountStateError)?;
            if state.accounts[index].last_history_id.as_deref() != Some(expected_cursor) {
                return Err(AccountStateError);
            }
            for message in &messages {
                message.validate()?;
                if message.account_id != account_id {
                    return Err(AccountStateError);
                }
            }
            let mut next = state.clone();
            let mut inserted_messages = Vec::new();
            for message in messages {
                if !next.messages.iter().any(|existing| {
                    existing.account_id == message.account_id && existing.id == message.id
                }) {
                    if next.messages.len() >= MAX_MESSAGES {
                        return Err(AccountStateError);
                    }
                    inserted_messages.push(message.clone());
                    next.messages.push(message);
                }
            }
            let pending_count = next
                .messages
                .iter()
                .filter(|message| {
                    message.account_id == account_id && message.opened_from_zerochi_at.is_none()
                })
                .count() as u64;
            let mut updated = next.accounts[index].clone().with_sync_state(
                next_cursor,
                synced_at,
                unread_count,
            )?;
            updated.pending_count = pending_count;
            updated.validate()?;
            next.accounts[index] = updated.clone();
            next.unread_message_ids
                .insert(account_id.to_owned(), unread_message_ids);
            validate_state(&next)?;
            write_document(&self.path, &next)?;
            *state = next;
            Ok(IncrementalCommitOutcome {
                account: updated,
                inserted_messages,
            })
        })
    }

    fn remove_account<'a>(
        &'a self,
        account_id: Uuid,
    ) -> AccountFuture<'a, Result<MailStateSnapshot, AccountStateError>> {
        Box::pin(async move {
            let account_id = account_id.hyphenated().to_string();
            let mut state = self.state.lock().map_err(|_| AccountStateError)?;
            if !state
                .accounts
                .iter()
                .any(|account| account.id == account_id)
            {
                return Err(AccountStateError);
            }
            let mut next = state.clone();
            next.accounts.retain(|account| account.id != account_id);
            next.messages
                .retain(|message| message.account_id != account_id);
            next.unread_message_ids.remove(&account_id);
            write_document(&self.path, &next)?;
            *state = next.clone();
            Ok(MailStateSnapshot {
                accounts: next.accounts,
                messages: next.messages,
            })
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StateDocument {
    version: u32,
    accounts: Vec<MailAccount>,
    messages: Vec<MailMessage>,
    unread_message_ids: Vec<UnreadMessageIdsDocument>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UnreadMessageIdsDocument {
    account_id: String,
    message_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyStateDocument {
    version: u32,
    accounts: Vec<MailAccount>,
    messages: Vec<MailMessage>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateDocumentV1 {
    version: u32,
    accounts: Vec<MailAccount>,
}

fn load_document(path: &Path) -> Result<NormalizedState, AccountStateError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(NormalizedState {
                accounts: Vec::new(),
                messages: Vec::new(),
                unread_message_ids: BTreeMap::new(),
            })
        }
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
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| AccountStateError)?;
    match value.get("version").and_then(serde_json::Value::as_u64) {
        Some(1) => {
            let document: StateDocumentV1 =
                serde_json::from_value(value).map_err(|_| AccountStateError)?;
            if document.version != 1 {
                return Err(AccountStateError);
            }
            let state = migrate_legacy_state(
                document
                    .accounts
                    .into_iter()
                    .map(|mut account| {
                        account.pending_count = 0;
                        account
                    })
                    .collect(),
                Vec::new(),
            );
            validate_state(&state)?;
            Ok(state)
        }
        Some(2 | 3) => {
            let document: LegacyStateDocument =
                serde_json::from_value(value).map_err(|_| AccountStateError)?;
            if !matches!(document.version, 2 | 3) {
                return Err(AccountStateError);
            }
            let state = migrate_legacy_state(document.accounts, document.messages);
            validate_state(&state)?;
            Ok(state)
        }
        Some(version) if version == u64::from(STATE_VERSION) => {
            let document: StateDocument =
                serde_json::from_value(value).map_err(|_| AccountStateError)?;
            let mut unread_message_ids = BTreeMap::new();
            for entry in document.unread_message_ids {
                if unread_message_ids
                    .insert(entry.account_id, validated_unread_set(entry.message_ids)?)
                    .is_some()
                {
                    return Err(AccountStateError);
                }
            }
            let state = NormalizedState {
                accounts: document.accounts,
                messages: document.messages,
                unread_message_ids,
            };
            validate_state(&state)?;
            Ok(state)
        }
        _ => Err(AccountStateError),
    }
}

fn migrate_legacy_state(
    mut accounts: Vec<MailAccount>,
    messages: Vec<MailMessage>,
) -> NormalizedState {
    for account in &mut accounts {
        account.clear_provider_sync_state();
    }
    NormalizedState {
        accounts,
        messages,
        unread_message_ids: BTreeMap::new(),
    }
}

fn validated_unread_set(ids: Vec<String>) -> Result<BTreeSet<String>, AccountStateError> {
    if ids.len() > MAX_UNREAD_MESSAGE_IDS {
        return Err(AccountStateError);
    }
    let mut unique = BTreeSet::new();
    for id in ids {
        if invalid_identifier(&id) || !unique.insert(id) {
            return Err(AccountStateError);
        }
    }
    Ok(unique)
}

fn validate_state(state: &NormalizedState) -> Result<(), AccountStateError> {
    if state.accounts.len() > MAX_ACCOUNTS || state.messages.len() > MAX_MESSAGES {
        return Err(AccountStateError);
    }
    for (index, account) in state.accounts.iter().enumerate() {
        account.validate()?;
        if state.accounts[..index].iter().any(|existing| {
            existing.id == account.id || existing.email.eq_ignore_ascii_case(&account.email)
        }) {
            return Err(AccountStateError);
        }
        let expected_pending = state
            .messages
            .iter()
            .filter(|message| {
                message.account_id == account.id && message.opened_from_zerochi_at.is_none()
            })
            .count() as u64;
        if account.pending_count != expected_pending {
            return Err(AccountStateError);
        }
        match state.unread_message_ids.get(&account.id) {
            Some(ids)
                if account.last_history_id.is_some()
                    && account.last_sync_at.is_some()
                    && account.inbox_unread_count == Some(ids.len() as u64)
                    && ids.len() <= MAX_UNREAD_MESSAGE_IDS
                    && ids.iter().all(|id| !invalid_identifier(id)) => {}
            None if account.last_history_id.is_none()
                && account.last_sync_at.is_none()
                && account.inbox_unread_count.is_none() => {}
            _ => return Err(AccountStateError),
        }
    }
    for (index, message) in state.messages.iter().enumerate() {
        message.validate()?;
        if !state
            .accounts
            .iter()
            .any(|account| account.id == message.account_id)
            || state.messages[..index].iter().any(|existing| {
                existing.account_id == message.account_id && existing.id == message.id
            })
        {
            return Err(AccountStateError);
        }
    }
    if state.unread_message_ids.keys().any(|account_id| {
        !state
            .accounts
            .iter()
            .any(|account| account.id == *account_id)
    }) {
        return Err(AccountStateError);
    }
    Ok(())
}

fn write_document(path: &Path, state: &NormalizedState) -> Result<(), AccountStateError> {
    validate_state_path(path)?;
    validate_state(state)?;
    let bytes = serde_json::to_vec(&StateDocument {
        version: STATE_VERSION,
        accounts: state.accounts.clone(),
        messages: state.messages.clone(),
        unread_message_ids: state
            .unread_message_ids
            .iter()
            .map(|(account_id, message_ids)| UnreadMessageIdsDocument {
                account_id: account_id.clone(),
                message_ids: message_ids.iter().cloned().collect(),
            })
            .collect(),
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

fn invalid_identifier(value: &str) -> bool {
    invalid_bounded_text(value, MAX_MESSAGE_ID_BYTES)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn invalid_header(value: &str) -> bool {
    value.len() > MAX_HEADER_BYTES || value.chars().any(char::is_control)
}

fn invalid_timestamp(value: &str) -> bool {
    invalid_bounded_text(value, MAX_SYNC_AT_BYTES)
        || chrono::DateTime::parse_from_rfc3339(value).is_err()
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct InMemoryAccountState {
    accounts: Mutex<Vec<MailAccount>>,
    messages: Mutex<Vec<MailMessage>>,
    unread_message_ids: Mutex<BTreeMap<String, BTreeSet<String>>>,
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
    fn snapshot(&self) -> Result<Vec<MailAccount>, AccountStateError> {
        Err(AccountStateError)
    }

    fn commit<'a>(
        &'a self,
        _account: MailAccount,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        Box::pin(async { Err(AccountStateError) })
    }

    fn update_sync_cursor<'a>(
        &'a self,
        _account_id: &'a str,
        _history_id: String,
        _synced_at: String,
        _inbox_unread_count: u64,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        Box::pin(async { Err(AccountStateError) })
    }

    fn remove_account<'a>(
        &'a self,
        _account_id: Uuid,
    ) -> AccountFuture<'a, Result<MailStateSnapshot, AccountStateError>> {
        Box::pin(async { Err(AccountStateError) })
    }
}

#[cfg(test)]
impl AccountStatePort for InMemoryAccountState {
    fn snapshot(&self) -> Result<Vec<MailAccount>, AccountStateError> {
        self.accounts
            .lock()
            .map(|accounts| accounts.clone())
            .map_err(|_| AccountStateError)
    }

    fn mail_snapshot(&self) -> Result<MailStateSnapshot, AccountStateError> {
        Ok(MailStateSnapshot {
            accounts: self.accounts.lock().map_err(|_| AccountStateError)?.clone(),
            messages: self.messages.lock().map_err(|_| AccountStateError)?.clone(),
        })
    }

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

    fn update_sync_cursor<'a>(
        &'a self,
        account_id: &'a str,
        history_id: String,
        synced_at: String,
        inbox_unread_count: u64,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        Box::pin(async move {
            if inbox_unread_count != 0 {
                return Err(AccountStateError);
            }
            let mut accounts = self.accounts.lock().map_err(|_| AccountStateError)?;
            let index = accounts
                .iter()
                .position(|account| account.id == account_id)
                .ok_or(AccountStateError)?;
            let updated = accounts[index].clone().with_sync_state(
                history_id,
                synced_at,
                inbox_unread_count,
            )?;
            accounts[index] = updated.clone();
            self.unread_message_ids
                .lock()
                .map_err(|_| AccountStateError)?
                .insert(account_id.to_owned(), BTreeSet::new());
            Ok(updated)
        })
    }

    fn unread_message_ids(&self, account_id: &str) -> Result<Vec<String>, AccountStateError> {
        self.unread_message_ids
            .lock()
            .map_err(|_| AccountStateError)?
            .get(account_id)
            .map(|ids| ids.iter().cloned().collect())
            .ok_or(AccountStateError)
    }

    fn commit_baseline<'a>(
        &'a self,
        account_id: &'a str,
        expected_cursor: Option<&'a str>,
        profile_email: &'a str,
        next_cursor: String,
        synced_at: String,
        unread_message_ids: Vec<String>,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        Box::pin(async move {
            let unread_message_ids = validated_unread_set(unread_message_ids)?;
            let mut accounts = self.accounts.lock().map_err(|_| AccountStateError)?;
            let index = accounts
                .iter()
                .position(|account| account.id == account_id && account.active)
                .ok_or(AccountStateError)?;
            if accounts[index].last_history_id.as_deref() != expected_cursor
                || !accounts[index].email.eq_ignore_ascii_case(profile_email)
            {
                return Err(AccountStateError);
            }
            let updated = accounts[index].clone().with_sync_state(
                next_cursor,
                synced_at,
                unread_message_ids.len() as u64,
            )?;
            accounts[index] = updated.clone();
            self.unread_message_ids
                .lock()
                .map_err(|_| AccountStateError)?
                .insert(account_id.to_owned(), unread_message_ids);
            Ok(updated)
        })
    }

    fn commit_inbox_unread<'a>(
        &'a self,
        account_id: &'a str,
        inbox_unread_count: u64,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        let _ = (account_id, inbox_unread_count);
        Box::pin(async { Err(AccountStateError) })
    }

    fn commit_incremental<'a>(
        &'a self,
        account_id: &'a str,
        expected_cursor: &'a str,
        next_cursor: String,
        synced_at: String,
        messages: Vec<MailMessage>,
        unread_message_ids: Vec<String>,
    ) -> AccountFuture<'a, Result<IncrementalCommitOutcome, AccountStateError>> {
        Box::pin(async move {
            let unread_message_ids = validated_unread_set(unread_message_ids)?;
            let mut accounts = self.accounts.lock().map_err(|_| AccountStateError)?;
            let mut persisted = self.messages.lock().map_err(|_| AccountStateError)?;
            let index = accounts
                .iter()
                .position(|account| account.id == account_id)
                .ok_or(AccountStateError)?;
            if accounts[index].last_history_id.as_deref() != Some(expected_cursor) {
                return Err(AccountStateError);
            }
            let mut inserted_messages = Vec::new();
            for message in messages {
                message.validate()?;
                if message.account_id != account_id {
                    return Err(AccountStateError);
                }
                if !persisted.iter().any(|existing| {
                    existing.account_id == message.account_id && existing.id == message.id
                }) {
                    inserted_messages.push(message.clone());
                    persisted.push(message);
                }
            }
            let pending_count = persisted
                .iter()
                .filter(|message| {
                    message.account_id == account_id && message.opened_from_zerochi_at.is_none()
                })
                .count() as u64;
            let mut updated = accounts[index].clone().with_sync_state(
                next_cursor,
                synced_at,
                unread_message_ids.len() as u64,
            )?;
            updated.pending_count = pending_count;
            accounts[index] = updated.clone();
            self.unread_message_ids
                .lock()
                .map_err(|_| AccountStateError)?
                .insert(account_id.to_owned(), unread_message_ids);
            Ok(IncrementalCommitOutcome {
                account: updated,
                inserted_messages,
            })
        })
    }

    fn remove_account<'a>(
        &'a self,
        account_id: Uuid,
    ) -> AccountFuture<'a, Result<MailStateSnapshot, AccountStateError>> {
        Box::pin(async move {
            let account_id = account_id.hyphenated().to_string();
            let mut accounts = self.accounts.lock().map_err(|_| AccountStateError)?;
            if !accounts.iter().any(|account| account.id == account_id) {
                return Err(AccountStateError);
            }
            let mut messages = self.messages.lock().map_err(|_| AccountStateError)?;
            accounts.retain(|account| account.id != account_id);
            messages.retain(|message| message.account_id != account_id);
            self.unread_message_ids
                .lock()
                .map_err(|_| AccountStateError)?
                .remove(&account_id);
            Ok(MailStateSnapshot {
                accounts: accounts.clone(),
                messages: messages.clone(),
            })
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

    fn message(account_id: &str, id: &str) -> MailMessage {
        MailMessage::detected(
            id.into(),
            format!("thread-{id}"),
            account_id.into(),
            "Sender <sender@example.test>".into(),
            "Subject".into(),
            "2026-08-10T11:59:00Z".into(),
            "2026-08-10T12:00:00Z".into(),
        )
        .expect("message")
    }

    fn unread_ids(count: usize) -> Vec<String> {
        (0..count).map(|index| format!("unread-{index}")).collect()
    }

    fn baseline(
        state: &dyn AccountStatePort,
        account: &MailAccount,
        cursor: &str,
        unread_count: usize,
    ) -> MailAccount {
        tauri::async_runtime::block_on(state.commit_baseline(
            account.id(),
            account.last_history_id(),
            account.email(),
            cursor.into(),
            "2026-08-10T12:00:00Z".into(),
            unread_ids(unread_count),
        ))
        .expect("baseline")
    }

    fn assert_legacy_migration_round_trip(
        fixture: &[u8],
        baseline_cursor: &str,
        incremental_cursor: &str,
        baseline_unread: usize,
    ) {
        let directory = TestDirectory::new();
        let path = directory.state_path();
        fs::write(&path, fixture).expect("write legacy fixture");
        let state = JsonAccountState::new(path.clone()).expect("load legacy state");
        let migrated_snapshot = state.mail_snapshot().expect("migrated snapshot");
        assert_eq!(migrated_snapshot.accounts.len(), 1);
        assert!(migrated_snapshot.messages.is_empty());
        let migrated = migrated_snapshot.accounts[0].clone();
        assert_eq!(migrated.pending_count(), 0);
        assert_eq!(migrated.inbox_unread_count(), None);
        assert_eq!(migrated.last_history_id(), None);
        assert!(state.unread_message_ids(migrated.id()).is_err());

        let baselined = baseline(&state, &migrated, baseline_cursor, baseline_unread);
        assert_eq!(baselined.pending_count(), 0);
        assert_eq!(baselined.inbox_unread_count(), Some(baseline_unread as u64));
        drop(state);

        let restarted = JsonAccountState::new(path.clone()).expect("restart v4 baseline");
        let restarted_snapshot = restarted.mail_snapshot().expect("restarted snapshot");
        assert!(restarted_snapshot.messages.is_empty());
        assert_eq!(restarted_snapshot.accounts[0].pending_count(), 0);
        assert_eq!(
            restarted_snapshot.accounts[0].inbox_unread_count(),
            Some(baseline_unread as u64)
        );
        assert_eq!(
            restarted_snapshot.accounts[0].last_history_id(),
            Some(baseline_cursor)
        );

        let detected = message(migrated.id(), "message-after-restart");
        let committed = tauri::async_runtime::block_on(restarted.commit_incremental(
            migrated.id(),
            baseline_cursor,
            incremental_cursor.into(),
            "2026-08-10T12:01:00Z".into(),
            vec![detected.clone()],
            unread_ids(baseline_unread + 1),
        ))
        .expect("incremental commit after restart");
        assert_eq!(committed.inserted_messages, vec![detected.clone()]);
        assert_eq!(committed.account.pending_count(), 1);
        assert_eq!(
            committed.account.inbox_unread_count(),
            Some((baseline_unread + 1) as u64)
        );
        assert_eq!(
            committed.account.last_history_id(),
            Some(incremental_cursor)
        );
        drop(restarted);

        let final_state = JsonAccountState::new(path.clone()).expect("reopen incremental v4");
        let final_snapshot = final_state.mail_snapshot().expect("final snapshot");
        assert_eq!(final_snapshot.messages, vec![detected]);
        assert_eq!(final_snapshot.accounts[0].pending_count(), 1);
        assert_eq!(
            final_snapshot.accounts[0].inbox_unread_count(),
            Some((baseline_unread + 1) as u64)
        );
        assert_eq!(
            final_snapshot.accounts[0].last_history_id(),
            Some(incremental_cursor)
        );
        assert!(fs::read_to_string(path)
            .expect("v4 document")
            .contains("\"version\":4"));
    }

    fn message_with_metadata(
        sender: String,
        subject: String,
    ) -> Result<MailMessage, AccountStateError> {
        MailMessage::detected(
            "message-1".into(),
            "thread-message-1".into(),
            "00000000-0000-4000-8000-000000000001".into(),
            sender,
            subject,
            "2026-08-10T11:59:00Z".into(),
            "2026-08-10T12:00:00Z".into(),
        )
    }

    #[test]
    fn message_metadata_accessors_preserve_empty_and_maximum_unicode_headers() {
        let empty = message_with_metadata(String::new(), String::new()).expect("empty metadata");
        assert_eq!(empty.sender(), "");
        assert_eq!(empty.subject(), "");

        let maximum_unicode = "é".repeat(MAX_HEADER_BYTES / "é".len());
        assert_eq!(maximum_unicode.len(), MAX_HEADER_BYTES);
        let bounded = message_with_metadata(maximum_unicode.clone(), maximum_unicode.clone())
            .expect("maximum Unicode metadata");
        assert_eq!(bounded.sender(), maximum_unicode);
        assert_eq!(bounded.subject(), maximum_unicode);
    }

    #[test]
    fn message_thread_id_accessor_returns_the_validated_persisted_identifier() {
        let message = message_with_metadata("Sender".into(), "Subject".into()).expect("message");

        assert_eq!(message.thread_id(), "thread-message-1");
        assert!(!format!("{message:?}").contains(message.thread_id()));
    }

    #[test]
    fn message_metadata_rejects_controls_and_oversize_values() {
        for invalid in [
            "line\nbreak".to_owned(),
            "null\0byte".to_owned(),
            "x".repeat(MAX_HEADER_BYTES + 1),
        ] {
            assert!(message_with_metadata(invalid.clone(), "Subject".into()).is_err());
            assert!(message_with_metadata("Sender".into(), invalid).is_err());
        }
    }

    #[test]
    fn message_debug_and_validation_errors_redact_metadata() {
        let sender = "Private Sender <private@example.test>";
        let subject = "Confidential subject";
        let message = message_with_metadata(sender.into(), subject.into()).expect("message");
        let rendered = format!("{message:?}");

        assert!(!rendered.contains(sender));
        assert!(!rendered.contains(subject));
        assert!(!rendered.contains(message.sender()));
        assert!(!rendered.contains(message.subject()));
        assert_eq!(
            message_with_metadata("control\nvalue".into(), subject.into())
                .expect_err("control metadata must fail")
                .to_string(),
            "account state is unavailable"
        );
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
    fn sync_cursor_and_timestamp_commit_atomically_without_changing_pending() {
        let directory = TestDirectory::new();
        let path = directory.state_path();
        let state = JsonAccountState::new(path.clone()).expect("open state");
        let original = account(
            "00000000-0000-4000-8000-000000000001",
            "person@example.test",
        );
        commit(&state, original.clone()).expect("commit account");

        let updated = baseline(&state, &original, "123456789", 8);
        let value = serde_json::to_value(&updated).expect("serialize updated account");
        assert_eq!(value["pendingCount"], 0);
        assert_eq!(value["inboxUnreadCount"], 8);
        assert_eq!(value["lastHistoryId"], "123456789");
        assert_eq!(value["lastSyncAt"], "2026-08-10T12:00:00Z");
        drop(state);

        let restarted = JsonAccountState::new(path).expect("reload state");
        assert_eq!(restarted.snapshot().expect("snapshot"), vec![updated]);
    }

    #[test]
    fn baseline_unread_set_is_durable_and_derived_into_the_compatible_count() {
        let directory = TestDirectory::new();
        let path = directory.state_path();
        let state = JsonAccountState::new(path.clone()).expect("open state");
        let original = account(
            "00000000-0000-4000-8000-000000000001",
            "person@example.test",
        );
        commit(&state, original.clone()).expect("commit account");

        let updated = baseline(&state, &original, "123", 11);
        let value = serde_json::to_value(&updated).expect("serialize account");
        assert_eq!(value["inboxUnreadCount"], 11);
        assert_eq!(value["pendingCount"], 0);
        assert_eq!(value["lastHistoryId"], "123");
        assert_eq!(
            state.unread_message_ids(original.id()).expect("ids").len(),
            11
        );
        drop(state);

        let restarted = JsonAccountState::new(path).expect("restart state");
        assert_eq!(restarted.snapshot().expect("snapshot"), vec![updated]);
    }

    #[test]
    fn unread_commit_rejects_inactive_accounts_without_mutation() {
        let directory = TestDirectory::new();
        let state = JsonAccountState::new(directory.state_path()).expect("state");
        let mut inactive = account(
            "00000000-0000-4000-8000-000000000001",
            "inactive@example.test",
        );
        inactive.active = false;
        commit(&state, inactive.clone()).expect("commit inactive account");

        assert!(
            tauri::async_runtime::block_on(state.commit_inbox_unread(inactive.id(), 14),).is_err()
        );
        assert_eq!(state.snapshot().expect("snapshot"), vec![inactive]);
    }

    #[test]
    fn invalid_cursor_update_does_not_mutate_durable_or_loaded_state() {
        let directory = TestDirectory::new();
        let path = directory.state_path();
        let state = JsonAccountState::new(path.clone()).expect("open state");
        let original = account(
            "00000000-0000-4000-8000-000000000001",
            "person@example.test",
        );
        commit(&state, original.clone()).expect("commit account");
        assert!(tauri::async_runtime::block_on(state.update_sync_cursor(
            original.id(),
            String::new(),
            "2026-08-10T12:00:00Z".into(),
            0,
        ))
        .is_err());
        assert_eq!(state.snapshot().expect("snapshot"), vec![original.clone()]);
        drop(state);
        assert_eq!(
            JsonAccountState::new(path)
                .expect("reload")
                .snapshot()
                .expect("snapshot"),
            vec![original]
        );
    }

    #[test]
    fn migrates_v1_to_v4_and_requires_a_fresh_baseline() {
        assert_legacy_migration_round_trip(
            br#"{"version":1,"accounts":[{"id":"00000000-0000-4000-8000-000000000001","email":"person@example.test","pendingCount":0,"active":true}]}"#,
            "123",
            "124",
            3,
        );
    }

    #[test]
    fn incremental_commit_deduplicates_and_cursor_cas_is_atomic() {
        let directory = TestDirectory::new();
        let path = directory.state_path();
        let state = JsonAccountState::new(path).expect("open state");
        let original = account(
            "00000000-0000-4000-8000-000000000001",
            "person@example.test",
        );
        commit(&state, original.clone()).expect("commit account");
        baseline(&state, &original, "100", 3);
        let detected = message(original.id(), "message-1");

        let committed = tauri::async_runtime::block_on(state.commit_incremental(
            original.id(),
            "100",
            "101".into(),
            "2026-08-10T12:01:00Z".into(),
            vec![detected.clone(), detected.clone()],
            unread_ids(4),
        ))
        .expect("incremental commit");
        assert_eq!(committed.account.pending_count(), 1);
        assert_eq!(committed.account.inbox_unread_count(), Some(4));
        assert_eq!(committed.inserted_messages, vec![detected.clone()]);
        let snapshot = state.mail_snapshot().expect("snapshot");
        assert_eq!(snapshot.messages, vec![detected.clone()]);

        let replay = tauri::async_runtime::block_on(state.commit_incremental(
            original.id(),
            "101",
            "102".into(),
            "2026-08-10T12:02:00Z".into(),
            vec![detected],
            unread_ids(4),
        ))
        .expect("duplicate replay commit");
        assert!(replay.inserted_messages.is_empty());
        assert_eq!(replay.account.pending_count(), 1);

        assert!(tauri::async_runtime::block_on(state.commit_incremental(
            original.id(),
            "101",
            "103".into(),
            "2026-08-10T12:03:00Z".into(),
            vec![message(original.id(), "message-2")],
            unread_ids(5),
        ))
        .is_err());
        let unchanged = state.mail_snapshot().expect("unchanged snapshot");
        assert_eq!(unchanged.messages.len(), 1);
        assert_eq!(unchanged.accounts[0].last_history_id(), Some("102"));
        assert_eq!(unchanged.accounts[0].pending_count(), 1);
        assert_eq!(unchanged.accounts[0].inbox_unread_count(), Some(4));
    }

    #[test]
    fn migrates_v2_to_v4_then_persists_the_new_baseline_across_restart() {
        assert_legacy_migration_round_trip(
            br#"{"version":2,"accounts":[{"id":"00000000-0000-4000-8000-000000000001","email":"person@example.test","pendingCount":0,"active":true}],"messages":[]}"#,
            "400",
            "401",
            12,
        );
    }

    #[test]
    fn migrates_v3_by_clearing_unverifiable_unread_and_cursor_state() {
        assert_legacy_migration_round_trip(
            br#"{"version":3,"accounts":[{"id":"00000000-0000-4000-8000-000000000001","email":"person@example.test","pendingCount":0,"inboxUnreadCount":12,"active":true,"lastHistoryId":"400","lastSyncAt":"2026-08-10T12:00:00Z"}],"messages":[]}"#,
            "500",
            "501",
            14,
        );
    }

    #[test]
    fn removal_is_account_scoped_and_stale_poll_cannot_resurrect_state() {
        let directory = TestDirectory::new();
        let state = JsonAccountState::new(directory.state_path()).expect("state");
        let removed = account(
            "00000000-0000-4000-8000-000000000001",
            "removed@example.test",
        );
        let retained = account(
            "00000000-0000-4000-8000-000000000002",
            "retained@example.test",
        );
        commit(&state, removed.clone()).expect("removed account");
        commit(&state, retained.clone()).expect("retained account");
        for account in [&removed, &retained] {
            baseline(&state, account, "100", 8);
            tauri::async_runtime::block_on(state.commit_incremental(
                account.id(),
                "100",
                "101".into(),
                "2026-08-10T12:01:00Z".into(),
                vec![message(account.id(), "message-1")],
                unread_ids(9),
            ))
            .expect("message");
        }

        let removed_snapshot = tauri::async_runtime::block_on(
            state.remove_account(Uuid::parse_str(removed.id()).expect("uuid")),
        )
        .expect("remove");
        assert_eq!(removed_snapshot.accounts.len(), 1);
        assert_eq!(removed_snapshot.accounts[0].id(), retained.id());
        assert_eq!(removed_snapshot.accounts[0].pending_count(), 1);
        assert_eq!(removed_snapshot.accounts[0].inbox_unread_count(), Some(9));
        assert_eq!(removed_snapshot.messages.len(), 1);
        assert_eq!(removed_snapshot.messages[0].account_id(), retained.id());

        assert!(tauri::async_runtime::block_on(state.commit_incremental(
            removed.id(),
            "101",
            "102".into(),
            "2026-08-10T12:02:00Z".into(),
            vec![message(removed.id(), "stale-message")],
            unread_ids(10),
        ))
        .is_err());
        assert!(
            tauri::async_runtime::block_on(state.commit_inbox_unread(removed.id(), 10),).is_err()
        );
        assert_eq!(state.mail_snapshot().expect("final"), removed_snapshot);
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
        assert!(contents.contains("\"version\":4"));
        assert!(contents.contains("\"messages\":[]"));
        assert!(contents.contains("\"unreadMessageIds\":[]"));
        assert!(!contents.to_ascii_lowercase().contains("token"));
    }

    #[test]
    fn persisted_budget_accommodates_maximum_bounded_unread_set() {
        let directory = TestDirectory::new();
        let path = directory.state_path();
        let state = JsonAccountState::new(path.clone()).expect("open state");
        let account = account(
            "00000000-0000-4000-8000-000000000001",
            "person@example.test",
        );
        commit(&state, account.clone()).expect("commit account");
        let suffix = "a".repeat(MAX_MESSAGE_ID_BYTES - 8);
        let ids = (0..MAX_UNREAD_MESSAGE_IDS)
            .map(|index| format!("{index:08x}{suffix}"))
            .collect::<Vec<_>>();

        tauri::async_runtime::block_on(state.commit_baseline(
            account.id(),
            None,
            account.email(),
            "101".into(),
            "2026-08-10T12:00:00Z".into(),
            ids,
        ))
        .expect("maximum unread baseline");

        let persisted_bytes = fs::metadata(&path).expect("metadata").len() as usize;
        assert!(persisted_bytes > 1024 * 1024);
        assert!(persisted_bytes <= MAX_STATE_BYTES);
        let reopened = JsonAccountState::new(path).expect("reopen maximum state");
        assert_eq!(
            reopened
                .unread_message_ids(account.id())
                .expect("persisted unread ids")
                .len(),
            MAX_UNREAD_MESSAGE_IDS
        );
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
