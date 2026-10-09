use super::{
    generate_pkce, AccountIdPort, AddAccountAttemptGuardSet, AddAccountError, AddAccountErrorTag,
    BrowserPort, CallbackPort, ClientIdPort, ClientSecretPort, IdentityPort, LoopbackRedirect,
    RandomSource, RefreshTokenStorePort, TokenExchangePort, TokenExchangeRequest,
};
use crate::accounts::{AccountStatePort, MailAccount};
use std::sync::Arc;
use uuid::Uuid;
use zeroize::Zeroize;

pub(crate) struct AddAccountPorts {
    pub(crate) client_id: Arc<dyn ClientIdPort>,
    pub(crate) client_secret: Arc<dyn ClientSecretPort>,
    pub(crate) random: Arc<dyn RandomSource>,
    pub(crate) browser: Arc<dyn BrowserPort>,
    pub(crate) callback: Arc<dyn CallbackPort>,
    pub(crate) token_exchange: Arc<dyn TokenExchangePort>,
    pub(crate) identity: Arc<dyn IdentityPort>,
    pub(crate) refresh_tokens: Arc<dyn RefreshTokenStorePort>,
    pub(crate) accounts: Arc<dyn AccountStatePort>,
    pub(crate) account_ids: Arc<dyn AccountIdPort>,
}

pub(crate) struct AddAccountService {
    attempts: AddAccountAttemptGuardSet,
    ports: AddAccountPorts,
}

impl AddAccountService {
    pub(crate) fn new(ports: AddAccountPorts) -> Self {
        Self {
            attempts: AddAccountAttemptGuardSet::new(),
            ports,
        }
    }

    pub(crate) async fn add_account(&self) -> Result<MailAccount, AddAccountError> {
        let guard = self.attempts.begin()?;
        let cancellation = guard.cancellation();
        let client_id = self.ports.client_id.load()?;
        let client_secret = self.ports.client_secret.load()?;
        cancellation.ensure_active()?;

        let mut callback = self.ports.callback.bind()?;
        let redirect = LoopbackRedirect::parse(callback.redirect_uri())?;
        let pkce = generate_pkce(self.ports.random.as_ref())?;
        let request = super::AuthorizationRequest::new(&client_id, redirect, pkce)?;

        self.ports.browser.open(&request, &cancellation).await?;
        cancellation.ensure_active()?;
        let callback_result = callback
            .receive(request.expected_state(), &cancellation)
            .await?;
        cancellation.ensure_active()?;

        let tokens = self
            .ports
            .token_exchange
            .exchange(
                TokenExchangeRequest {
                    client_id: &client_id,
                    client_secret: &client_secret,
                    redirect_uri: request.redirect_uri(),
                    code: &callback_result.code,
                    verifier: request.verifier(),
                },
                &cancellation,
            )
            .await?;
        cancellation.ensure_active()?;

        let identity = self
            .ports
            .identity
            .identity(&tokens.access_token, &cancellation)
            .await?;
        cancellation.ensure_active()?;
        let account_id = self.ports.account_ids.generate()?;
        let account = MailAccount::new(
            account_id.hyphenated().to_string(),
            identity.email,
            identity.display_name,
        );
        let mut refresh_token = tokens
            .refresh_token
            .ok_or_else(|| AddAccountError::from_tag(AddAccountErrorTag::RefreshTokenMissing))?;

        let store_result = self
            .ports
            .refresh_tokens
            .set(account_id, refresh_token.expose_bytes())
            .await;
        refresh_token.zeroize_now();
        if store_result.is_err() {
            return Err(AddAccountError::from_tag(
                AddAccountErrorTag::SecureStoreFailed,
            ));
        }

        if let Err(cancelled) = cancellation.ensure_active() {
            self.delete_after_failure(account_id).await?;
            return Err(cancelled);
        }

        match self.ports.accounts.commit(account).await {
            Ok(account) => Ok(account),
            Err(_) => {
                self.delete_after_failure(account_id).await?;
                Err(AddAccountError::from_tag(AddAccountErrorTag::Internal))
            }
        }
    }

    pub(crate) fn cancel(&self) -> Result<(), AddAccountError> {
        self.attempts.cancel_active().map(|_| ())
    }

    async fn delete_after_failure(&self, account_id: Uuid) -> Result<(), AddAccountError> {
        self.ports
            .refresh_tokens
            .delete(account_id)
            .await
            .map_err(|_| AddAccountError::from_tag(AddAccountErrorTag::SecureStoreFailed))
    }
}

pub(crate) struct OsAccountId;

impl AccountIdPort for OsAccountId {
    fn generate(&self) -> Result<Uuid, AddAccountError> {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes)
            .map_err(|_| AddAccountError::from_tag(AddAccountErrorTag::Internal))?;
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let id = Uuid::from_bytes(bytes);
        bytes.zeroize();
        Ok(id)
    }
}

trait ZeroizeSensitive {
    fn zeroize_now(&mut self);
}

impl ZeroizeSensitive for super::SensitiveString {
    fn zeroize_now(&mut self) {
        // Drop already scrubs; replacing forces scrubbing immediately after the store call.
        let old = std::mem::replace(self, super::SensitiveString::new(String::new()));
        drop(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{
        AuthorizationCallback, AuthorizationRequest, CallbackSession, Cancellation, ClientId,
        Environment, PortFuture, SensitiveString, TokenResponse,
    };
    use crate::gmail::GoogleIdentity;
    use std::{
        collections::VecDeque,
        ffi::OsString,
        sync::{Mutex, MutexGuard},
        time::Duration,
    };

    const ACCOUNT_ID: Uuid = Uuid::from_u128(0xaaaaaaaa_aaaa_4aaa_8aaa_aaaaaaaaaaaa);

    struct Config;
    impl ClientIdPort for Config {
        fn load(&self) -> Result<ClientId, AddAccountError> {
            struct Env;
            impl Environment for Env {
                fn var_os(&self, _name: &str) -> Option<OsString> {
                    Some(
                        "123456789012-abcdefghijklmnopqrstuvwxyz012345.apps.googleusercontent.com"
                            .into(),
                    )
                }
            }
            ClientId::load_from(&Env)
        }
    }

    struct Secret;
    impl ClientSecretPort for Secret {
        fn load(&self) -> Result<crate::auth::ClientSecret, AddAccountError> {
            struct Env;
            impl Environment for Env {
                fn var_os(&self, _name: &str) -> Option<OsString> {
                    Some("fixture-client-secret-value".into())
                }
            }
            crate::auth::ClientSecret::load_from(&Env)
        }
    }

    struct MissingSecret;
    impl ClientSecretPort for MissingSecret {
        fn load(&self) -> Result<crate::auth::ClientSecret, AddAccountError> {
            Err(AddAccountError::from_tag(AddAccountErrorTag::ConfigInvalid))
        }
    }

    struct Random(Mutex<VecDeque<[u8; 32]>>);
    impl RandomSource for Random {
        fn fill(&self, destination: &mut [u8]) -> Result<(), AddAccountError> {
            destination.copy_from_slice(
                &self
                    .0
                    .lock()
                    .map_err(|_| AddAccountError::from_tag(AddAccountErrorTag::Internal))?
                    .pop_front()
                    .ok_or_else(|| AddAccountError::from_tag(AddAccountErrorTag::Internal))?,
            );
            Ok(())
        }
    }

    #[derive(Default)]
    struct Events(Mutex<Vec<&'static str>>);
    impl Events {
        fn push(&self, event: &'static str) {
            self.0.lock().expect("events lock").push(event);
        }
        fn lock(&self) -> MutexGuard<'_, Vec<&'static str>> {
            self.0.lock().expect("events lock")
        }
    }

    struct Browser(Arc<Events>);
    impl BrowserPort for Browser {
        fn open<'a>(
            &'a self,
            request: &'a AuthorizationRequest,
            _cancellation: &'a Cancellation,
        ) -> PortFuture<'a, Result<(), AddAccountError>> {
            self.0.push("browser");
            assert!(request.url().starts_with("https://accounts.google.com/"));
            Box::pin(async { Ok(()) })
        }
    }

    struct Callback(Arc<Events>);
    struct Session(Arc<Events>);
    impl CallbackPort for Callback {
        fn bind(&self) -> Result<Box<dyn CallbackSession>, AddAccountError> {
            self.0.push("bind");
            Ok(Box::new(Session(Arc::clone(&self.0))))
        }
    }
    impl CallbackSession for Session {
        fn redirect_uri(&self) -> &str {
            "http://127.0.0.1:49152/oauth/callback"
        }
        fn receive<'a>(
            &'a mut self,
            _expected_state: &'a str,
            _cancellation: &'a Cancellation,
        ) -> PortFuture<'a, Result<AuthorizationCallback, AddAccountError>> {
            self.0.push("callback");
            Box::pin(async {
                Ok(AuthorizationCallback {
                    code: SensitiveString::new("fixture-code".into()),
                })
            })
        }
    }

    struct Exchange {
        events: Arc<Events>,
        refresh: bool,
    }
    impl TokenExchangePort for Exchange {
        fn exchange<'a>(
            &'a self,
            request: TokenExchangeRequest<'a>,
            _cancellation: &'a Cancellation,
        ) -> PortFuture<'a, Result<TokenResponse, AddAccountError>> {
            self.events.push("exchange");
            assert_eq!(request.code.expose(), "fixture-code");
            assert_eq!(
                request.client_secret.expose(),
                "fixture-client-secret-value"
            );
            let refresh = self.refresh;
            Box::pin(async move {
                Ok(TokenResponse {
                    access_token: SensitiveString::new("fixture-access".into()),
                    refresh_token: refresh.then(|| SensitiveString::new("fixture-refresh".into())),
                    expires_in: Duration::from_secs(3600),
                })
            })
        }
    }

    struct Identity(Arc<Events>);
    impl IdentityPort for Identity {
        fn identity<'a>(
            &'a self,
            access_token: &'a SensitiveString,
            _cancellation: &'a Cancellation,
        ) -> PortFuture<'a, Result<GoogleIdentity, AddAccountError>> {
            self.0.push("identity");
            assert_eq!(access_token.expose(), "fixture-access");
            Box::pin(async {
                Ok(GoogleIdentity {
                    email: "person@example.test".into(),
                    display_name: Some("Person".into()),
                })
            })
        }
    }

    struct Id;
    impl AccountIdPort for Id {
        fn generate(&self) -> Result<Uuid, AddAccountError> {
            Ok(ACCOUNT_ID)
        }
    }

    struct Store {
        events: Arc<Events>,
        fail_set: bool,
        fail_delete: bool,
    }
    impl RefreshTokenStorePort for Store {
        fn set<'a>(
            &'a self,
            account_id: Uuid,
            refresh_token: &'a [u8],
        ) -> PortFuture<'a, Result<(), AddAccountError>> {
            self.events.push("store");
            assert_eq!(account_id, ACCOUNT_ID);
            assert_eq!(refresh_token, b"fixture-refresh");
            let fail = self.fail_set;
            Box::pin(async move {
                if fail {
                    Err(AddAccountError::from_tag(AddAccountErrorTag::Internal))
                } else {
                    Ok(())
                }
            })
        }
        fn delete<'a>(&'a self, account_id: Uuid) -> PortFuture<'a, Result<(), AddAccountError>> {
            self.events.push("delete");
            assert_eq!(account_id, ACCOUNT_ID);
            let fail = self.fail_delete;
            Box::pin(async move {
                if fail {
                    Err(AddAccountError::from_tag(AddAccountErrorTag::Internal))
                } else {
                    Ok(())
                }
            })
        }
    }

    struct Accounts {
        events: Arc<Events>,
        fail: bool,
    }

    impl AccountStatePort for Accounts {
        fn snapshot(&self) -> Result<Vec<MailAccount>, crate::accounts::AccountStateError> {
            Ok(Vec::new())
        }

        fn commit<'a>(
            &'a self,
            account: MailAccount,
        ) -> crate::accounts::AccountFuture<
            'a,
            Result<MailAccount, crate::accounts::AccountStateError>,
        > {
            self.events.push("commit");
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    Err(crate::accounts::AccountStateError)
                } else {
                    Ok(account)
                }
            })
        }

        fn update_sync_cursor<'a>(
            &'a self,
            _account_id: &'a str,
            _history_id: String,
            _synced_at: String,
            _inbox_unread_count: u64,
        ) -> crate::accounts::AccountFuture<
            'a,
            Result<MailAccount, crate::accounts::AccountStateError>,
        > {
            Box::pin(async { Err(crate::accounts::AccountStateError) })
        }
    }

    fn service(
        events: Arc<Events>,
        refresh: bool,
        fail_store: bool,
        fail_commit: bool,
        fail_delete: bool,
    ) -> AddAccountService {
        AddAccountService::new(AddAccountPorts {
            client_id: Arc::new(Config),
            client_secret: Arc::new(Secret),
            random: Arc::new(Random(Mutex::new(VecDeque::from([[0x11; 32], [0x22; 32]])))),
            browser: Arc::new(Browser(Arc::clone(&events))),
            callback: Arc::new(Callback(Arc::clone(&events))),
            token_exchange: Arc::new(Exchange {
                events: Arc::clone(&events),
                refresh,
            }),
            identity: Arc::new(Identity(Arc::clone(&events))),
            refresh_tokens: Arc::new(Store {
                events: Arc::clone(&events),
                fail_set: fail_store,
                fail_delete,
            }),
            accounts: Arc::new(Accounts {
                events,
                fail: fail_commit,
            }),
            account_ids: Arc::new(Id),
        })
    }

    #[test]
    fn completes_in_secure_transactional_order_and_returns_only_normalized_account() {
        let events = Arc::new(Events::default());
        let service = service(Arc::clone(&events), true, false, false, false);
        let account = tauri::async_runtime::block_on(service.add_account()).expect("add account");

        assert_eq!(account.id(), ACCOUNT_ID.hyphenated().to_string());
        assert_eq!(account.email(), "person@example.test");
        assert_eq!(
            events.lock().as_slice(),
            ["bind", "browser", "callback", "exchange", "identity", "store", "commit"]
        );
        let serialized = serde_json::to_string(&account).expect("serialize account");
        assert!(!serialized.contains("fixture-access"));
        assert!(!serialized.contains("fixture-refresh"));
        assert!(!serialized.contains("fixture-code"));
        assert!(!serialized.contains("fixture-client-secret-value"));
    }

    #[test]
    fn missing_client_credential_fails_before_binding_or_opening_browser() {
        let events = Arc::new(Events::default());
        let mut service = service(Arc::clone(&events), true, false, false, false);
        service.ports.client_secret = Arc::new(MissingSecret);

        let error = tauri::async_runtime::block_on(service.add_account()).unwrap_err();

        assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
        assert!(events.lock().is_empty());
    }

    #[test]
    fn missing_refresh_token_persists_nothing() {
        let events = Arc::new(Events::default());
        let service = service(Arc::clone(&events), false, false, false, false);
        let error = tauri::async_runtime::block_on(service.add_account()).unwrap_err();

        assert_eq!(error.tag(), AddAccountErrorTag::RefreshTokenMissing);
        assert!(!events.lock().contains(&"store"));
        assert!(!events.lock().contains(&"commit"));
    }

    #[test]
    fn secure_store_failure_prevents_account_commit_and_is_sanitized() {
        let events = Arc::new(Events::default());
        let service = service(Arc::clone(&events), true, true, false, false);
        let error = tauri::async_runtime::block_on(service.add_account()).unwrap_err();

        assert_eq!(error.tag(), AddAccountErrorTag::SecureStoreFailed);
        assert!(!events.lock().contains(&"commit"));
        assert!(!format!("{error:?}").contains("fixture-refresh"));
    }

    #[test]
    fn account_commit_failure_compensates_with_secret_delete() {
        let events = Arc::new(Events::default());
        let service = service(Arc::clone(&events), true, false, true, false);
        let error = tauri::async_runtime::block_on(service.add_account()).unwrap_err();

        assert_eq!(error.tag(), AddAccountErrorTag::Internal);
        let events = events.lock();
        assert_eq!(&events[events.len() - 3..], ["store", "commit", "delete"]);
    }

    #[test]
    fn failed_compensating_delete_is_a_secure_store_failure() {
        let events = Arc::new(Events::default());
        let service = service(events, true, false, true, true);
        let error = tauri::async_runtime::block_on(service.add_account()).unwrap_err();
        assert_eq!(error.tag(), AddAccountErrorTag::SecureStoreFailed);
    }

    #[test]
    fn service_rejects_a_duplicate_attempt_and_cancel_is_idempotent() {
        let service = service(Arc::new(Events::default()), true, false, false, false);
        let active = service.attempts.begin().expect("active attempt");

        let error = tauri::async_runtime::block_on(service.add_account()).unwrap_err();
        assert_eq!(error.tag(), AddAccountErrorTag::AuthInProgress);
        service.cancel().expect("first cancellation");
        service.cancel().expect("repeated cancellation");
        assert!(active.cancellation().is_cancelled());
    }
}
