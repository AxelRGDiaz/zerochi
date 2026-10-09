use super::{RefreshTokenReadPort, RefreshTokenStorePort};
use crate::{
    accounts::{AccountStatePort, MailStateSnapshot},
    gmail::TokenRevokePort,
};
use serde::Serialize;
use std::{fmt, sync::Arc};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RemoveAccountErrorTag {
    AccountNotFound,
    SecureStoreFailed,
    Internal,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct RemoveAccountError {
    tag: RemoveAccountErrorTag,
}

impl RemoveAccountError {
    fn new(tag: RemoveAccountErrorTag) -> Self {
        Self { tag }
    }

    pub(crate) fn tag(self) -> RemoveAccountErrorTag {
        self.tag
    }

    pub(crate) fn message(self) -> &'static str {
        match self.tag {
            RemoveAccountErrorTag::AccountNotFound => "The account is no longer connected.",
            RemoveAccountErrorTag::SecureStoreFailed => {
                "The account credential could not be removed."
            }
            RemoveAccountErrorTag::Internal => "The account could not be removed.",
        }
    }

    pub(crate) fn retryable(self) -> bool {
        false
    }

    pub(crate) fn action_required(self) -> bool {
        self.tag == RemoveAccountErrorTag::SecureStoreFailed
    }
}

impl fmt::Debug for RemoveAccountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoveAccountError")
            .field("tag", &self.tag)
            .finish()
    }
}

impl fmt::Display for RemoveAccountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for RemoveAccountError {}

pub(crate) struct RemoveAccountPorts {
    pub(crate) refresh_tokens: Arc<dyn RefreshTokenReadPort>,
    pub(crate) refresh_token_store: Arc<dyn RefreshTokenStorePort>,
    pub(crate) revoker: Arc<dyn TokenRevokePort>,
    pub(crate) accounts: Arc<dyn AccountStatePort>,
}

pub(crate) struct RemoveAccountService {
    ports: RemoveAccountPorts,
}

impl RemoveAccountService {
    pub(crate) fn new(ports: RemoveAccountPorts) -> Self {
        Self { ports }
    }

    pub(crate) async fn remove_account(
        &self,
        account_id: Uuid,
    ) -> Result<MailStateSnapshot, RemoveAccountError> {
        let canonical_id = account_id.hyphenated().to_string();
        let exists = self
            .ports
            .accounts
            .snapshot()
            .map_err(|_| RemoveAccountError::new(RemoveAccountErrorTag::Internal))?
            .iter()
            .any(|account| account.id() == canonical_id);
        if !exists {
            return Err(RemoveAccountError::new(
                RemoveAccountErrorTag::AccountNotFound,
            ));
        }

        if let Ok(Some(refresh_token)) = self.ports.refresh_tokens.get(account_id).await {
            // Revocation is bounded and best-effort. Local unlink must remain available offline.
            let _ = self.ports.revoker.revoke(refresh_token.as_slice()).await;
        }

        self.ports
            .refresh_token_store
            .delete(account_id)
            .await
            .map_err(|_| RemoveAccountError::new(RemoveAccountErrorTag::SecureStoreFailed))?;

        match self.ports.accounts.remove_account(account_id).await {
            Ok(snapshot) => Ok(snapshot),
            Err(_) => {
                let missing_after_race = self.ports.accounts.snapshot().is_ok_and(|accounts| {
                    accounts.iter().all(|account| account.id() != canonical_id)
                });
                Err(RemoveAccountError::new(if missing_after_race {
                    RemoveAccountErrorTag::AccountNotFound
                } else {
                    RemoveAccountErrorTag::Internal
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accounts::{AccountFuture, AccountStateError, MailAccount},
        auth::{AddAccountError, AddAccountErrorTag, PortFuture, RefreshTokenReadError},
        gmail::{GmailFuture, TokenRevokeError},
    };
    use std::sync::{Mutex, MutexGuard};
    use zeroize::Zeroizing;

    const ACCOUNT_ID: Uuid = Uuid::from_u128(0x00000000_0000_4000_8000_000000000001);
    const OTHER_ID: Uuid = Uuid::from_u128(0x00000000_0000_4000_8000_000000000002);

    #[derive(Default)]
    struct Events(Mutex<Vec<&'static str>>);

    impl Events {
        fn push(&self, event: &'static str) {
            self.0.lock().expect("events").push(event);
        }

        fn lock(&self) -> MutexGuard<'_, Vec<&'static str>> {
            self.0.lock().expect("events")
        }
    }

    struct Reader {
        events: Arc<Events>,
        result: Result<Option<Vec<u8>>, RefreshTokenReadError>,
    }

    impl RefreshTokenReadPort for Reader {
        fn get<'a>(
            &'a self,
            account_id: Uuid,
        ) -> PortFuture<'a, Result<Option<Zeroizing<Vec<u8>>>, RefreshTokenReadError>> {
            assert_eq!(account_id, ACCOUNT_ID);
            self.events.push("read");
            let result = self.result.clone();
            Box::pin(async move { result.map(|token| token.map(Zeroizing::new)) })
        }
    }

    struct Store {
        events: Arc<Events>,
        fail: bool,
    }

    impl RefreshTokenStorePort for Store {
        fn set<'a>(
            &'a self,
            _account_id: Uuid,
            _refresh_token: &'a [u8],
        ) -> PortFuture<'a, Result<(), AddAccountError>> {
            Box::pin(async { Err(AddAccountError::from_tag(AddAccountErrorTag::Internal)) })
        }

        fn delete<'a>(&'a self, account_id: Uuid) -> PortFuture<'a, Result<(), AddAccountError>> {
            assert_eq!(account_id, ACCOUNT_ID);
            self.events.push("delete");
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    Err(AddAccountError::from_tag(AddAccountErrorTag::Internal))
                } else {
                    Ok(())
                }
            })
        }
    }

    struct Revoker {
        events: Arc<Events>,
        fail: bool,
    }

    impl TokenRevokePort for Revoker {
        fn revoke<'a>(
            &'a self,
            refresh_token: &'a [u8],
        ) -> GmailFuture<'a, Result<(), TokenRevokeError>> {
            assert_eq!(refresh_token, b"fixture-refresh-token");
            self.events.push("revoke");
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    Err(TokenRevokeError)
                } else {
                    Ok(())
                }
            })
        }
    }

    struct Accounts {
        events: Arc<Events>,
        accounts: Mutex<Vec<MailAccount>>,
    }

    impl AccountStatePort for Accounts {
        fn snapshot(&self) -> Result<Vec<MailAccount>, AccountStateError> {
            self.accounts
                .lock()
                .map(|accounts| accounts.clone())
                .map_err(|_| AccountStateError)
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
            account_id: Uuid,
        ) -> AccountFuture<'a, Result<MailStateSnapshot, AccountStateError>> {
            Box::pin(async move {
                self.events.push("remove");
                let id = account_id.hyphenated().to_string();
                let mut accounts = self.accounts.lock().map_err(|_| AccountStateError)?;
                let before = accounts.len();
                accounts.retain(|account| account.id() != id);
                if before == accounts.len() {
                    return Err(AccountStateError);
                }
                Ok(MailStateSnapshot {
                    accounts: accounts.clone(),
                    messages: Vec::new(),
                })
            })
        }
    }

    fn account(id: Uuid, email: &str) -> MailAccount {
        MailAccount::new(id.hyphenated().to_string(), email.into(), None)
    }

    fn service(
        events: Arc<Events>,
        token: Result<Option<Vec<u8>>, RefreshTokenReadError>,
        revoke_fail: bool,
        delete_fail: bool,
    ) -> (RemoveAccountService, Arc<Accounts>) {
        let accounts = Arc::new(Accounts {
            events: events.clone(),
            accounts: Mutex::new(vec![
                account(ACCOUNT_ID, "removed@example.test"),
                account(OTHER_ID, "retained@example.test"),
            ]),
        });
        (
            RemoveAccountService::new(RemoveAccountPorts {
                refresh_tokens: Arc::new(Reader {
                    events: events.clone(),
                    result: token,
                }),
                refresh_token_store: Arc::new(Store {
                    events: events.clone(),
                    fail: delete_fail,
                }),
                revoker: Arc::new(Revoker {
                    events,
                    fail: revoke_fail,
                }),
                accounts: accounts.clone(),
            }),
            accounts,
        )
    }

    #[test]
    fn revokes_then_requires_secret_delete_before_isolated_local_removal() {
        let events = Arc::new(Events::default());
        let (service, _) = service(
            events.clone(),
            Ok(Some(b"fixture-refresh-token".to_vec())),
            false,
            false,
        );

        let snapshot = tauri::async_runtime::block_on(service.remove_account(ACCOUNT_ID))
            .expect("remove account");

        assert_eq!(&*events.lock(), &["read", "revoke", "delete", "remove"]);
        assert_eq!(snapshot.accounts.len(), 1);
        assert_eq!(snapshot.accounts[0].id(), OTHER_ID.hyphenated().to_string());
    }

    #[test]
    fn offline_revoke_missing_or_unreadable_token_do_not_trap_local_unlink() {
        for (token, expected) in [
            (
                Ok(Some(b"fixture-refresh-token".to_vec())),
                vec!["read", "revoke", "delete", "remove"],
            ),
            (Ok(None), vec!["read", "delete", "remove"]),
            (
                Err(RefreshTokenReadError::Unavailable),
                vec!["read", "delete", "remove"],
            ),
        ] {
            let events = Arc::new(Events::default());
            let (service, _) = service(events.clone(), token, true, false);
            tauri::async_runtime::block_on(service.remove_account(ACCOUNT_ID))
                .expect("best effort unlink");
            assert_eq!(&*events.lock(), expected.as_slice());
        }
    }

    #[test]
    fn keychain_delete_failure_preserves_local_account_and_is_sanitized() {
        let events = Arc::new(Events::default());
        let (service, accounts) = service(events.clone(), Ok(None), false, true);

        let error = tauri::async_runtime::block_on(service.remove_account(ACCOUNT_ID)).unwrap_err();

        assert_eq!(error.tag(), RemoveAccountErrorTag::SecureStoreFailed);
        assert_eq!(&*events.lock(), &["read", "delete"]);
        assert_eq!(accounts.snapshot().expect("snapshot").len(), 2);
        assert!(!format!("{error:?} {error}").contains(ACCOUNT_ID.to_string().as_str()));
    }

    #[test]
    fn unknown_account_touches_no_credentials_and_returns_only_approved_metadata() {
        let events = Arc::new(Events::default());
        let (service, _) = service(events.clone(), Ok(None), false, false);

        let error =
            tauri::async_runtime::block_on(service.remove_account(Uuid::nil())).unwrap_err();

        assert_eq!(error.tag(), RemoveAccountErrorTag::AccountNotFound);
        assert!(!error.retryable());
        assert!(!error.action_required());
        assert!(events.lock().is_empty());
        assert_eq!(
            serde_json::to_string(&error.tag()).expect("tag"),
            "\"account_not_found\""
        );
    }
}
