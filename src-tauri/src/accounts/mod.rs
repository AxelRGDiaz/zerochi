//! Normalized, non-secret account state used by the OAuth application service.

#![allow(dead_code)]

use serde::Serialize;
use std::{fmt, future::Future, pin::Pin, sync::Mutex};

pub(crate) type AccountFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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

/// M4 process-local state adapter. Durable state remains owned by the persistence milestone.
#[derive(Default)]
pub(crate) struct InMemoryAccountState {
    accounts: Mutex<Vec<MailAccount>>,
}

impl AccountStatePort for InMemoryAccountState {
    fn commit<'a>(
        &'a self,
        account: MailAccount,
    ) -> AccountFuture<'a, Result<MailAccount, AccountStateError>> {
        Box::pin(async move {
            let mut accounts = self.accounts.lock().map_err(|_| AccountStateError)?;
            if accounts.iter().any(|existing| existing.id == account.id) {
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

    #[test]
    fn normalized_account_matches_the_shared_contract_without_secret_fields() {
        let account = MailAccount::new(
            "00000000-0000-4000-8000-000000000001".into(),
            "person@example.test".into(),
            Some("Person".into()),
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
    fn debug_redacts_identity() {
        let account = MailAccount::new(
            "fixture-account-id".into(),
            "person@example.test".into(),
            Some("Fixture Person".into()),
        );
        let rendered = format!("{account:?}");

        assert!(!rendered.contains(account.id()));
        assert!(!rendered.contains(account.email()));
        assert!(!rendered.contains("Fixture Person"));
    }

    #[test]
    fn in_memory_state_commits_once_and_rejects_duplicate_ids() {
        let state = InMemoryAccountState::default();
        let account = MailAccount::new(
            "00000000-0000-4000-8000-000000000001".into(),
            "person@example.test".into(),
            None,
        );

        let committed =
            tauri::async_runtime::block_on(state.commit(account.clone())).expect("first commit");
        assert_eq!(committed, account);
        assert!(tauri::async_runtime::block_on(state.commit(account)).is_err());
    }
}
