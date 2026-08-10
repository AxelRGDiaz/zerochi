//! Stable React/Tauri command DTO boundary.

use crate::{
    accounts::{JsonAccountState, MailAccount},
    auth::{
        AddAccountError, AddAccountErrorTag, AddAccountPorts, AddAccountService,
        LoopbackCallbackPort, OsAccountId, OsRandom, ProcessClientId,
    },
    gmail::GoogleHttpClient,
    navigation::MacOsBrowserPort,
    secure_store::SecureRefreshTokenStorePort,
};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tauri::State;

pub(crate) struct AppState {
    add_account: AddAccountService,
}

impl AppState {
    pub(crate) fn new(accounts_path: PathBuf) -> Result<Self, AddAccountError> {
        let google = Arc::new(GoogleHttpClient::new()?);
        let accounts = JsonAccountState::new(accounts_path)
            .map_err(|_| AddAccountError::from_tag(AddAccountErrorTag::Internal))?;
        let refresh_tokens = if cfg!(debug_assertions) {
            SecureRefreshTokenStorePort::unsigned_development()?
        } else {
            SecureRefreshTokenStorePort::signed_provisioned()?
        };

        Ok(Self {
            add_account: AddAccountService::new(AddAccountPorts {
                client_id: Arc::new(ProcessClientId),
                random: Arc::new(OsRandom),
                browser: Arc::new(MacOsBrowserPort::new()),
                callback: Arc::new(LoopbackCallbackPort::default()),
                token_exchange: google.clone(),
                identity: google,
                refresh_tokens: Arc::new(refresh_tokens),
                accounts: Arc::new(accounts),
                account_ids: Arc::new(OsAccountId),
            }),
        })
    }
}

#[derive(Serialize)]
pub(crate) struct AddAccountResult {
    account: MailAccount,
}

#[tauri::command]
pub(crate) async fn add_account(
    state: State<'_, AppState>,
) -> Result<AddAccountResult, AddAccountError> {
    state
        .add_account
        .add_account()
        .await
        .map(|account| AddAccountResult { account })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_dto_has_only_the_approved_account_field() {
        let result = AddAccountResult {
            account: MailAccount::new(
                "00000000-0000-4000-8000-000000000001".into(),
                "person@example.test".into(),
                Some("Person".into()),
            ),
        };
        let value = serde_json::to_value(result).expect("serialize result");

        assert_eq!(value.as_object().map(|object| object.len()), Some(1));
        assert_eq!(value["account"]["email"], "person@example.test");
        assert!(value["account"].get("accessToken").is_none());
        assert!(value["account"].get("refreshToken").is_none());
    }
}
