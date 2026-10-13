//! Stable React/Tauri command DTO boundary.

use crate::{
    accounts::{AccountStatePort, JsonAccountState, MailAccount, MailMessage, MailProvider},
    auth::{
        AddAccountError, AddAccountErrorTag, AddAccountPorts, AddAccountService,
        LoopbackCallbackPort, OsAccountId, OsRandom, ProcessClientId, ProcessClientSecret,
        ProcessMicrosoftClientId, PublicClientSecret, RemoveAccountError, RemoveAccountErrorTag,
        RemoveAccountPorts, RemoveAccountService,
    },
    gmail::GoogleHttpClient,
    microsoft::{MicrosoftHttpClient, RotatingRefreshTokens},
    navigation::{
        ExternalNavigationError, ExternalNavigationService, MacOsBrowserPort,
        MacOsExternalNavigationService,
    },
    notifications::{
        NotificationActivationTarget, NotificationRequest, NotificationService,
        OpaqueNotificationId,
    },
    platform,
    secure_store::SecureRefreshTokenStorePort,
    sync::{
        GmailSyncEngine, IncrementalSyncPort, ProviderRoutingSync, SystemSyncClock,
        HEALTHY_POLL_INTERVAL,
    },
    tray::{self, PetTrayState},
};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    time::Instant,
};
use uuid::Uuid;

const SNAPSHOT_CHANGED_EVENT: &str = "snapshot_changed";
const SYNC_STATUS_CHANGED_EVENT: &str = "sync_status_changed";

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SyncStatus {
    Healthy,
    Retrying,
    ActionRequired,
}

enum PollSchedulerEvent {
    Reconcile,
    PollNowAccount(String),
    PollNowAll,
    Pause,
    WakeAndCatchUp,
    Completed {
        account_id: String,
        next_poll_after: Duration,
    },
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SyncStatusChanged {
    account_id: String,
    status: SyncStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_seconds: Option<u64>,
}

impl SyncStatusChanged {
    fn healthy(account_id: &str) -> Self {
        Self {
            account_id: account_id.to_owned(),
            status: SyncStatus::Healthy,
            retry_after_seconds: None,
        }
    }

    fn failed(account_id: &str, retry_after: Duration, action_required: bool) -> Self {
        Self {
            account_id: account_id.to_owned(),
            status: if action_required {
                SyncStatus::ActionRequired
            } else {
                SyncStatus::Retrying
            },
            retry_after_seconds: (!action_required).then_some(retry_after.as_secs()),
        }
    }
}

pub(crate) struct AppState {
    add_account: AddAccountService,
    add_outlook_account: AddAccountService,
    remove_account: RemoveAccountService,
    accounts: Arc<dyn AccountStatePort>,
    external_navigation: Arc<dyn ExternalNavigationService>,
    incremental_sync: Arc<dyn IncrementalSyncPort>,
    notifications: Arc<dyn NotificationService>,
    poll_events: UnboundedSender<PollSchedulerEvent>,
    poll_events_rx: std::sync::Mutex<Option<UnboundedReceiver<PollSchedulerEvent>>>,
    snapshot_publish: std::sync::Mutex<()>,
}

impl AppState {
    pub(crate) fn new(
        accounts_path: PathBuf,
        notifications: Arc<dyn NotificationService>,
    ) -> Result<Self, AddAccountError> {
        let google = Arc::new(GoogleHttpClient::new()?);
        let accounts: Arc<dyn AccountStatePort> = Arc::new(
            JsonAccountState::new(accounts_path)
                .map_err(|_| AddAccountError::from_tag(AddAccountErrorTag::Internal))?,
        );
        let refresh_tokens = Arc::new(if cfg!(debug_assertions) {
            SecureRefreshTokenStorePort::unsigned_development()?
        } else {
            SecureRefreshTokenStorePort::signed_provisioned()?
        });
        let client_secret = Arc::new(ProcessClientSecret);
        let gmail_sync = Arc::new(
            GmailSyncEngine::new(google.clone(), accounts.clone(), Arc::new(SystemSyncClock))
                .with_incremental(
                    Arc::new(ProcessClientId),
                    client_secret.clone(),
                    refresh_tokens.clone(),
                    google.clone(),
                ),
        );
        let outlook_refresh_tokens = Arc::new(RotatingRefreshTokens::new(
            refresh_tokens.clone(),
            refresh_tokens.clone(),
            accounts.clone(),
        ));
        let microsoft =
            Arc::new(MicrosoftHttpClient::new()?.with_rotation(outlook_refresh_tokens.clone()));
        let outlook_sync = Arc::new(
            GmailSyncEngine::new(
                microsoft.clone(),
                accounts.clone(),
                Arc::new(SystemSyncClock),
            )
            .with_incremental(
                Arc::new(ProcessMicrosoftClientId),
                Arc::new(PublicClientSecret),
                outlook_refresh_tokens,
                microsoft.clone(),
            ),
        );
        let sync_engine = Arc::new(ProviderRoutingSync::new(
            accounts.clone(),
            gmail_sync,
            outlook_sync,
        ));
        let remove_account = RemoveAccountService::new(RemoveAccountPorts {
            refresh_tokens: refresh_tokens.clone(),
            refresh_token_store: refresh_tokens.clone(),
            revoker: google.clone(),
            accounts: accounts.clone(),
        });

        let (poll_events, poll_events_rx) = mpsc::unbounded_channel();
        Ok(Self {
            add_account: AddAccountService::new(AddAccountPorts {
                client_id: Arc::new(ProcessClientId),
                client_secret,
                random: Arc::new(OsRandom),
                browser: Arc::new(MacOsBrowserPort::new()),
                callback: Arc::new(LoopbackCallbackPort::default()),
                token_exchange: google.clone(),
                identity: google,
                refresh_tokens: refresh_tokens.clone(),
                accounts: accounts.clone(),
                account_ids: Arc::new(OsAccountId),
            }),
            add_outlook_account: AddAccountService::for_provider(
                AddAccountPorts {
                    client_id: Arc::new(ProcessMicrosoftClientId),
                    client_secret: Arc::new(PublicClientSecret),
                    random: Arc::new(OsRandom),
                    browser: Arc::new(MacOsBrowserPort::new()),
                    callback: Arc::new(LoopbackCallbackPort::localhost()),
                    token_exchange: microsoft.clone(),
                    identity: microsoft,
                    refresh_tokens,
                    accounts: accounts.clone(),
                    account_ids: Arc::new(OsAccountId),
                },
                MailProvider::Outlook,
            ),
            remove_account,
            accounts,
            external_navigation: Arc::new(MacOsExternalNavigationService::new()),
            incremental_sync: sync_engine,
            notifications,
            poll_events,
            poll_events_rx: std::sync::Mutex::new(Some(poll_events_rx)),
            snapshot_publish: std::sync::Mutex::new(()),
        })
    }

    pub(crate) fn refresh_tray(&self, app: &AppHandle) {
        let Ok(accounts) = self.accounts.snapshot() else {
            return;
        };
        let _ = tray::update(app, tray_state_for_accounts(&accounts));
    }

    fn publish_snapshot(&self, app: &AppHandle, snapshot: AppSnapshot) {
        let _publish_guard = self
            .snapshot_publish
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let snapshot = self.snapshot().unwrap_or(snapshot);
        let _ = tray::update(app, tray_state_for_accounts(&snapshot.accounts));
        let _ = app.emit(SNAPSHOT_CHANGED_EVENT, snapshot);
    }

    fn snapshot(&self) -> Result<AppSnapshot, AddAccountError> {
        self.accounts
            .mail_snapshot()
            .map(|snapshot| AppSnapshot {
                accounts: snapshot.accounts,
                messages: snapshot.messages,
            })
            .map_err(|_| AddAccountError::from_tag(AddAccountErrorTag::Internal))
    }

    fn reconcile_polling(&self) {
        let _ = self.poll_events.send(PollSchedulerEvent::Reconcile);
    }

    fn poll_all_now(&self) -> Result<(), CheckMailNowCommandError> {
        request_all_polls_now(&self.poll_events)
    }
}

pub(crate) fn request_scheduler_lifecycle(
    app: &AppHandle,
    request: platform::SchedulerLifecycleRequest,
) {
    let event = match request {
        platform::SchedulerLifecycleRequest::Pause => PollSchedulerEvent::Pause,
        platform::SchedulerLifecycleRequest::WakeAndCatchUp => PollSchedulerEvent::WakeAndCatchUp,
    };
    let _ = app.state::<AppState>().poll_events.send(event);
}

fn request_all_polls_now(
    poll_events: &UnboundedSender<PollSchedulerEvent>,
) -> Result<(), CheckMailNowCommandError> {
    poll_events
        .send(PollSchedulerEvent::PollNowAll)
        .map_err(|_| CheckMailNowCommandError::internal())
}

fn tray_state_for_accounts(accounts: &[MailAccount]) -> PetTrayState {
    accounts.iter().filter(|account| account.is_active()).fold(
        PetTrayState {
            account_count: 0,
            pending_total: 0,
            inbox_unread_total: 0,
        },
        |mut state, account| {
            state.account_count = state.account_count.saturating_add(1);
            state.pending_total = state.pending_total.saturating_add(account.pending_count());
            state.inbox_unread_total = state
                .inbox_unread_total
                .saturating_add(account.inbox_unread_count().unwrap_or(0));
            state
        },
    )
}

fn complete_successful_poll(
    new_messages: &[MailMessage],
    accounts: &[MailAccount],
    notifications: &dyn NotificationService,
    publish_snapshot: impl FnOnce(),
    emit_healthy: impl FnOnce(),
) {
    publish_snapshot();
    for message in new_messages {
        let Some(account) = accounts
            .iter()
            .find(|account| account.id() == message.account_id())
        else {
            continue;
        };
        let Ok(account_id) = Uuid::parse_str(message.account_id()) else {
            continue;
        };
        let Ok(activation_target) = NotificationActivationTarget::new(account_id, message.id())
        else {
            continue;
        };
        let id = OpaqueNotificationId::from_message_key(message.account_id(), message.id());
        let _ = notifications.notify(NotificationRequest::new(
            id,
            activation_target,
            account.email(),
            Some(message.subject()),
            Some(message.sender()),
        ));
    }
    emit_healthy();
}

fn complete_failed_poll(
    committed_snapshot: Option<AppSnapshot>,
    publish_snapshot: impl FnOnce(AppSnapshot),
    emit_status: impl FnOnce(),
) {
    if let Some(snapshot) = committed_snapshot {
        publish_snapshot(snapshot);
    }
    emit_status();
}

fn due_account_ids(
    next_poll_by_account: &HashMap<String, Instant>,
    in_flight: &HashSet<String>,
    now: Instant,
) -> Vec<String> {
    next_poll_by_account
        .iter()
        .filter(|(account_id, due)| **due <= now && !in_flight.contains(*account_id))
        .map(|(account_id, _)| account_id.clone())
        .collect()
}

fn apply_scheduler_event(
    event: PollSchedulerEvent,
    next_poll_by_account: &mut HashMap<String, Instant>,
    in_flight: &mut HashSet<String>,
    poll_again_after_flight: &mut HashSet<String>,
    paused: &mut bool,
    now: Instant,
) {
    match event {
        PollSchedulerEvent::Reconcile => {}
        PollSchedulerEvent::PollNowAccount(account_id) => {
            if in_flight.contains(&account_id) {
                if next_poll_by_account.contains_key(&account_id) {
                    poll_again_after_flight.insert(account_id);
                }
            } else {
                if let Some(due) = next_poll_by_account.get_mut(&account_id) {
                    *due = now;
                }
            }
        }
        PollSchedulerEvent::PollNowAll => {
            for (account_id, due) in next_poll_by_account.iter_mut() {
                if in_flight.contains(account_id) {
                    poll_again_after_flight.insert(account_id.clone());
                } else {
                    *due = now;
                }
            }
        }
        PollSchedulerEvent::Pause => *paused = true,
        PollSchedulerEvent::WakeAndCatchUp => {
            *paused = false;
            for (account_id, due) in next_poll_by_account.iter_mut() {
                if in_flight.contains(account_id) {
                    poll_again_after_flight.insert(account_id.clone());
                } else {
                    *due = now;
                }
            }
        }
        PollSchedulerEvent::Completed {
            account_id,
            next_poll_after,
        } => {
            in_flight.remove(&account_id);
            let force_immediate = poll_again_after_flight.remove(&account_id);
            if let Some(due) = next_poll_by_account.get_mut(&account_id) {
                *due = if force_immediate {
                    now
                } else {
                    now + next_poll_after
                };
            }
        }
    }
}

fn drain_pending_scheduler_events(
    events: &mut UnboundedReceiver<PollSchedulerEvent>,
    next_poll_by_account: &mut HashMap<String, Instant>,
    in_flight: &mut HashSet<String>,
    poll_again_after_flight: &mut HashSet<String>,
    paused: &mut bool,
    now: Instant,
) {
    while let Ok(event) = events.try_recv() {
        apply_scheduler_event(
            event,
            next_poll_by_account,
            in_flight,
            poll_again_after_flight,
            paused,
            now,
        );
    }
}

async fn poll_and_publish(app: &AppHandle, account_id: &str) -> Duration {
    let sync = app.state::<AppState>().incremental_sync.clone();
    match sync.poll_account(account_id).await {
        Ok(outcome) => {
            let delay = outcome.next_poll_after;
            let notification_accounts = outcome.snapshot.accounts.clone();
            let snapshot = AppSnapshot {
                accounts: outcome.snapshot.accounts,
                messages: outcome.snapshot.messages,
            };
            let state = app.state::<AppState>();
            complete_successful_poll(
                &outcome.new_messages,
                &notification_accounts,
                state.notifications.as_ref(),
                || state.publish_snapshot(app, snapshot),
                || {
                    let _ = app.emit(
                        SYNC_STATUS_CHANGED_EVENT,
                        SyncStatusChanged::healthy(account_id),
                    );
                },
            );
            delay
        }
        Err(error) => {
            let retry_after = error.retry_after();
            let committed_snapshot = error.committed_snapshot().map(|snapshot| AppSnapshot {
                accounts: snapshot.accounts.clone(),
                messages: snapshot.messages.clone(),
            });
            let status =
                SyncStatusChanged::failed(account_id, retry_after, error.action_required());
            complete_failed_poll(
                committed_snapshot,
                |snapshot| app.state::<AppState>().publish_snapshot(app, snapshot),
                || {
                    let _ = app.emit(SYNC_STATUS_CHANGED_EVENT, status);
                },
            );
            retry_after
        }
    }
}

pub(crate) fn start_polling(app: AppHandle) {
    let Some(mut events) = app
        .state::<AppState>()
        .poll_events_rx
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
    else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let mut next_poll_by_account = HashMap::<String, Instant>::new();
        let mut in_flight = HashSet::<String>::new();
        let mut poll_again_after_flight = HashSet::<String>::new();
        let mut paused = false;
        loop {
            // A native sleep notification may arrive after the prior timeout but before this
            // iteration calculates due accounts. Apply queued lifecycle intent first so no new
            // Gmail request can start after macOS has announced suspension.
            drain_pending_scheduler_events(
                &mut events,
                &mut next_poll_by_account,
                &mut in_flight,
                &mut poll_again_after_flight,
                &mut paused,
                Instant::now(),
            );
            let sync = app.state::<AppState>().incremental_sync.clone();
            let account_ids = match sync.eligible_account_ids() {
                Ok(account_ids) => account_ids,
                Err(error) => {
                    let event = if paused {
                        events.recv().await
                    } else {
                        tokio::time::timeout(error.retry_after(), events.recv())
                            .await
                            .ok()
                            .flatten()
                    };
                    if let Some(event) = event {
                        apply_scheduler_event(
                            event,
                            &mut next_poll_by_account,
                            &mut in_flight,
                            &mut poll_again_after_flight,
                            &mut paused,
                            Instant::now(),
                        );
                    }
                    continue;
                }
            };

            next_poll_by_account.retain(|account_id, _| account_ids.contains(account_id));
            poll_again_after_flight.retain(|account_id| account_ids.contains(account_id));
            let now = Instant::now();
            for account_id in account_ids {
                next_poll_by_account.entry(account_id).or_insert(now);
            }

            let due_accounts = if paused {
                Vec::new()
            } else {
                due_account_ids(&next_poll_by_account, &in_flight, now)
            };
            for account_id in due_accounts {
                in_flight.insert(account_id.clone());
                let app = app.clone();
                let events = app.state::<AppState>().poll_events.clone();
                tauri::async_runtime::spawn(async move {
                    let delay = poll_and_publish(&app, &account_id).await;
                    let _ = events.send(PollSchedulerEvent::Completed {
                        account_id,
                        next_poll_after: delay,
                    });
                });
            }

            let event = if paused {
                events.recv().await
            } else {
                let delay = next_poll_by_account
                    .iter()
                    .filter(|(account_id, _)| !in_flight.contains(*account_id))
                    .map(|(_, due)| due)
                    .min()
                    .map(|due| due.saturating_duration_since(Instant::now()))
                    .unwrap_or(HEALTHY_POLL_INTERVAL);
                tokio::time::timeout(delay, events.recv())
                    .await
                    .ok()
                    .flatten()
            };
            if let Some(event) = event {
                apply_scheduler_event(
                    event,
                    &mut next_poll_by_account,
                    &mut in_flight,
                    &mut poll_again_after_flight,
                    &mut paused,
                    Instant::now(),
                );
            }
        }
    });
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSnapshot {
    accounts: Vec<MailAccount>,
    messages: Vec<MailMessage>,
}

#[derive(Serialize)]
pub(crate) struct AddAccountResult {
    account: MailAccount,
}

#[derive(Serialize)]
pub(crate) struct RemoveAccountResult {
    snapshot: AppSnapshot,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoveAccountCommandError {
    tag: RemoveAccountErrorTag,
    message: &'static str,
    retryable: bool,
    action_required: bool,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum OpenAccountErrorTag {
    AccountNotFound,
    Internal,
    NavigationFailed,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenAccountCommandError {
    tag: OpenAccountErrorTag,
    message: &'static str,
    retryable: bool,
    action_required: bool,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CheckMailNowErrorTag {
    Internal,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CheckMailNowCommandError {
    tag: CheckMailNowErrorTag,
    message: &'static str,
    retryable: bool,
    action_required: bool,
}

impl CheckMailNowCommandError {
    fn internal() -> Self {
        Self {
            tag: CheckMailNowErrorTag::Internal,
            message: "Mail check is unavailable.",
            retryable: true,
            action_required: false,
        }
    }
}

impl OpenAccountCommandError {
    fn account_not_found() -> Self {
        Self {
            tag: OpenAccountErrorTag::AccountNotFound,
            message: "The account is no longer connected.",
            retryable: false,
            action_required: false,
        }
    }

    fn internal() -> Self {
        Self {
            tag: OpenAccountErrorTag::Internal,
            message: "Account navigation is unavailable.",
            retryable: true,
            action_required: false,
        }
    }

    fn navigation_failed() -> Self {
        Self {
            tag: OpenAccountErrorTag::NavigationFailed,
            message: "Gmail could not be opened.",
            retryable: true,
            action_required: false,
        }
    }
}

#[cfg(test)]
fn open_persisted_account(
    accounts: &dyn AccountStatePort,
    external_navigation: &dyn ExternalNavigationService,
    account_id: &str,
) -> Result<(), OpenAccountCommandError> {
    let parsed_account_id =
        Uuid::parse_str(account_id).map_err(|_| OpenAccountCommandError::account_not_found())?;
    if parsed_account_id.hyphenated().to_string() != account_id {
        return Err(OpenAccountCommandError::account_not_found());
    }

    let accounts = accounts
        .snapshot()
        .map_err(|_| OpenAccountCommandError::internal())?;
    let account = accounts
        .iter()
        .find(|account| account.id() == account_id)
        .ok_or_else(OpenAccountCommandError::account_not_found)?;

    external_navigation
        .open_account(account.email())
        .map_err(|_| OpenAccountCommandError::navigation_failed())
}

fn open_persisted_account_and_poll(
    accounts: &dyn AccountStatePort,
    external_navigation: &dyn ExternalNavigationService,
    poll_events: &UnboundedSender<PollSchedulerEvent>,
    account_id: &str,
) -> Result<(), OpenAccountCommandError> {
    let parsed_account_id =
        Uuid::parse_str(account_id).map_err(|_| OpenAccountCommandError::account_not_found())?;
    if parsed_account_id.hyphenated().to_string() != account_id {
        return Err(OpenAccountCommandError::account_not_found());
    }

    let accounts = accounts
        .snapshot()
        .map_err(|_| OpenAccountCommandError::internal())?;
    let account = accounts
        .iter()
        .find(|account| account.id() == account_id)
        .ok_or_else(OpenAccountCommandError::account_not_found)?;

    // Polling is intentionally best-effort and independent from the browser handoff. A closed
    // scheduler must not change the established navigation result.
    let _ = poll_events.send(PollSchedulerEvent::PollNowAccount(account_id.to_owned()));
    open_provider_account(external_navigation, account)
        .map_err(|_| OpenAccountCommandError::navigation_failed())
}

fn open_provider_account(
    external_navigation: &dyn ExternalNavigationService,
    account: &MailAccount,
) -> Result<(), ExternalNavigationError> {
    match account.provider() {
        MailProvider::Gmail => external_navigation.open_account(account.email()),
        MailProvider::Outlook => external_navigation.open_outlook_account(account.email()),
    }
}

fn activate_persisted_notification_message(
    accounts: &dyn AccountStatePort,
    external_navigation: &dyn ExternalNavigationService,
    poll_events: &UnboundedSender<PollSchedulerEvent>,
    target: &NotificationActivationTarget,
) {
    let account_id = target.account_id().hyphenated().to_string();
    let Ok(snapshot) = accounts.mail_snapshot() else {
        return;
    };
    let Some(account) = snapshot
        .accounts
        .iter()
        .find(|account| account.id() == account_id)
    else {
        return;
    };
    let Some(message) = snapshot
        .messages
        .iter()
        .find(|message| message.account_id() == account_id && message.id() == target.message_id())
    else {
        return;
    };

    let _ = poll_events.send(PollSchedulerEvent::PollNowAccount(account_id));
    let _ = match account.provider() {
        MailProvider::Gmail => {
            external_navigation.open_message(account.email(), message.thread_id())
        }
        MailProvider::Outlook => external_navigation.open_outlook_account(account.email()),
    };
}

pub(crate) fn activate_notification_message(app: &AppHandle, target: NotificationActivationTarget) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    activate_persisted_notification_message(
        state.accounts.as_ref(),
        state.external_navigation.as_ref(),
        &state.poll_events,
        &target,
    );
}

impl RemoveAccountCommandError {
    fn account_not_found() -> Self {
        Self {
            tag: RemoveAccountErrorTag::AccountNotFound,
            message: "The account is no longer connected.",
            retryable: false,
            action_required: false,
        }
    }
}

impl From<RemoveAccountError> for RemoveAccountCommandError {
    fn from(error: RemoveAccountError) -> Self {
        Self {
            tag: error.tag(),
            message: error.message(),
            retryable: error.retryable(),
            action_required: error.action_required(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OAuthCompletionAction {
    RestoreWindow,
    PublishSnapshot,
    WakePolling,
}

fn oauth_completion_actions(succeeded: bool) -> &'static [OAuthCompletionAction] {
    const FAILURE: &[OAuthCompletionAction] = &[OAuthCompletionAction::RestoreWindow];
    const SUCCESS: &[OAuthCompletionAction] = &[
        OAuthCompletionAction::RestoreWindow,
        OAuthCompletionAction::PublishSnapshot,
        OAuthCompletionAction::WakePolling,
    ];
    if succeeded {
        SUCCESS
    } else {
        FAILURE
    }
}

/// `None` keeps the original zero-argument Gmail contract.
fn add_account_provider(provider: Option<&str>) -> Result<MailProvider, AddAccountError> {
    match provider {
        None | Some("gmail") => Ok(MailProvider::Gmail),
        Some("outlook") => Ok(MailProvider::Outlook),
        Some(_) => Err(AddAccountError::from_tag(AddAccountErrorTag::ConfigInvalid)),
    }
}

#[tauri::command]
pub(crate) async fn add_account(
    provider: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<AddAccountResult, AddAccountError> {
    let service = match add_account_provider(provider.as_deref())? {
        MailProvider::Gmail => &state.add_account,
        MailProvider::Outlook => &state.add_outlook_account,
    };
    let result = service
        .add_account()
        .await
        .map(|account| AddAccountResult { account });

    for action in oauth_completion_actions(result.is_ok()) {
        match action {
            OAuthCompletionAction::RestoreWindow => platform::restore_main_window(&app),
            OAuthCompletionAction::PublishSnapshot => {
                if let Ok(snapshot) = state.snapshot() {
                    state.publish_snapshot(&app, snapshot);
                }
            }
            OAuthCompletionAction::WakePolling => {
                state.reconcile_polling();
            }
        }
    }
    result
}

#[tauri::command]
pub(crate) fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, AddAccountError> {
    state.snapshot()
}

#[tauri::command]
pub(crate) fn open_account(
    account_id: String,
    state: State<'_, AppState>,
) -> Result<(), OpenAccountCommandError> {
    open_persisted_account_and_poll(
        state.accounts.as_ref(),
        state.external_navigation.as_ref(),
        &state.poll_events,
        &account_id,
    )
}

#[tauri::command]
pub(crate) fn check_mail_now(state: State<'_, AppState>) -> Result<(), CheckMailNowCommandError> {
    state.poll_all_now()
}

#[tauri::command]
pub(crate) async fn remove_account(
    account_id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<RemoveAccountResult, RemoveAccountCommandError> {
    let parsed_account_id =
        Uuid::parse_str(&account_id).map_err(|_| RemoveAccountCommandError::account_not_found())?;
    if parsed_account_id.hyphenated().to_string() != account_id {
        return Err(RemoveAccountCommandError::account_not_found());
    }
    let snapshot = state
        .remove_account
        .remove_account(parsed_account_id)
        .await
        .map_err(RemoveAccountCommandError::from)?;
    let snapshot = AppSnapshot {
        accounts: snapshot.accounts,
        messages: snapshot.messages,
    };
    state.publish_snapshot(&app, snapshot.clone());
    state.reconcile_polling();
    Ok(RemoveAccountResult { snapshot })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accounts::{AccountFuture, AccountStateError},
        navigation::ExternalNavigationError,
        notifications::NotificationError,
    };
    use std::sync::Mutex;

    struct SnapshotAccounts {
        accounts: Vec<MailAccount>,
        messages: Vec<MailMessage>,
        fail_snapshot: bool,
    }

    impl AccountStatePort for SnapshotAccounts {
        fn snapshot(&self) -> Result<Vec<MailAccount>, AccountStateError> {
            if self.fail_snapshot {
                Err(AccountStateError)
            } else {
                Ok(self.accounts.clone())
            }
        }

        fn mail_snapshot(&self) -> Result<crate::accounts::MailStateSnapshot, AccountStateError> {
            if self.fail_snapshot {
                Err(AccountStateError)
            } else {
                Ok(crate::accounts::MailStateSnapshot {
                    accounts: self.accounts.clone(),
                    messages: self.messages.clone(),
                })
            }
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
    }

    struct RecordingNavigation {
        emails: Mutex<Vec<String>>,
        threads: Mutex<Vec<String>>,
        fail: bool,
    }

    impl ExternalNavigationService for RecordingNavigation {
        fn open_account(&self, email: &str) -> Result<(), ExternalNavigationError> {
            self.emails
                .lock()
                .expect("navigation log")
                .push(email.into());
            if self.fail {
                Err(ExternalNavigationError)
            } else {
                Ok(())
            }
        }

        fn open_message(
            &self,
            email: &str,
            thread_id: &str,
        ) -> Result<(), ExternalNavigationError> {
            self.emails
                .lock()
                .expect("navigation log")
                .push(email.into());
            self.threads
                .lock()
                .expect("thread log")
                .push(thread_id.into());
            if self.fail {
                Err(ExternalNavigationError)
            } else {
                Ok(())
            }
        }
    }

    struct RecordingNotifications {
        events: Arc<Mutex<Vec<&'static str>>>,
        fail: bool,
    }

    impl NotificationService for RecordingNotifications {
        fn notify(&self, _request: NotificationRequest) -> Result<(), NotificationError> {
            self.events.lock().expect("event log").push("notify");
            if self.fail {
                Err(NotificationError)
            } else {
                Ok(())
            }
        }
    }

    fn notification_message(account_id: &str, id: &str) -> MailMessage {
        MailMessage::detected(
            id.into(),
            format!("thread-{id}"),
            account_id.into(),
            "Private Sender <sender@example.test>".into(),
            "Private Subject".into(),
            "2026-08-11T11:59:00Z".into(),
            "2026-08-11T12:00:00Z".into(),
        )
        .expect("valid message")
    }

    fn notification_account(account_id: &str) -> MailAccount {
        MailAccount::new(account_id.into(), "destination@example.test".into(), None)
    }

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

    #[test]
    fn remove_account_errors_are_exact_sanitized_command_shapes() {
        let not_found = serde_json::to_value(RemoveAccountCommandError::account_not_found())
            .expect("serialize remove error");
        assert_eq!(
            not_found,
            serde_json::json!({
                "tag": "account_not_found",
                "message": "The account is no longer connected.",
                "retryable": false,
                "actionRequired": false
            })
        );

        for tag in [
            RemoveAccountErrorTag::AccountNotFound,
            RemoveAccountErrorTag::SecureStoreFailed,
            RemoveAccountErrorTag::Internal,
        ] {
            let value = serde_json::to_value(tag).expect("serialize remove tag");
            assert!(matches!(
                value.as_str(),
                Some("account_not_found" | "secure_store_failed" | "internal")
            ));
        }
    }

    #[test]
    fn open_account_errors_are_exact_sanitized_command_shapes() {
        let cases = [
            (
                OpenAccountCommandError::account_not_found(),
                serde_json::json!({
                    "tag": "account_not_found",
                    "message": "The account is no longer connected.",
                    "retryable": false,
                    "actionRequired": false
                }),
            ),
            (
                OpenAccountCommandError::navigation_failed(),
                serde_json::json!({
                    "tag": "navigation_failed",
                    "message": "Gmail could not be opened.",
                    "retryable": true,
                    "actionRequired": false
                }),
            ),
            (
                OpenAccountCommandError::internal(),
                serde_json::json!({
                    "tag": "internal",
                    "message": "Account navigation is unavailable.",
                    "retryable": true,
                    "actionRequired": false
                }),
            ),
        ];

        for (error, expected) in cases {
            let value = serde_json::to_value(error).expect("serialize open-account error");
            assert_eq!(value, expected);
            let serialized = value.to_string().to_ascii_lowercase();
            for forbidden in [
                "person@example.test",
                "mail.google.com",
                "authuser",
                "account-id",
                "/usr/bin/open",
                "token",
            ] {
                assert!(!serialized.contains(forbidden));
            }
        }
    }

    #[test]
    fn open_account_resolves_only_the_persisted_identity_without_mutating_state() {
        let account_id = "00000000-0000-4000-8000-000000000001";
        let accounts = SnapshotAccounts {
            accounts: vec![MailAccount::new(
                account_id.into(),
                "person@example.test".into(),
                Some("Person".into()),
            )],
            messages: Vec::new(),
            fail_snapshot: false,
        };
        let before = serde_json::to_value(accounts.snapshot().expect("snapshot before"))
            .expect("serialize before");
        let navigation = RecordingNavigation {
            emails: Mutex::new(Vec::new()),
            threads: Mutex::new(Vec::new()),
            fail: false,
        };

        open_persisted_account(&accounts, &navigation, account_id).expect("navigation handoff");

        assert_eq!(
            navigation.emails.lock().expect("navigation log").as_slice(),
            ["person@example.test"]
        );
        let after = serde_json::to_value(accounts.snapshot().expect("snapshot after"))
            .expect("serialize after");
        assert_eq!(after, before);
    }

    #[test]
    fn open_account_rejects_noncanonical_or_missing_ids_before_navigation() {
        let account_id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let accounts = SnapshotAccounts {
            accounts: vec![MailAccount::new(
                account_id.into(),
                "person@example.test".into(),
                None,
            )],
            messages: Vec::new(),
            fail_snapshot: false,
        };
        let navigation = RecordingNavigation {
            emails: Mutex::new(Vec::new()),
            threads: Mutex::new(Vec::new()),
            fail: false,
        };

        for rejected in [
            "AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA",
            "not-a-uuid",
            "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        ] {
            assert_eq!(
                open_persisted_account(&accounts, &navigation, rejected),
                Err(OpenAccountCommandError::account_not_found())
            );
        }
        assert!(navigation.emails.lock().expect("navigation log").is_empty());
    }

    #[test]
    fn open_account_maps_storage_and_native_failures_without_identity_diagnostics() {
        let account_id = "00000000-0000-4000-8000-000000000001";
        let account = MailAccount::new(
            account_id.into(),
            "sensitive+person@example.test".into(),
            None,
        );
        let unavailable_accounts = SnapshotAccounts {
            accounts: Vec::new(),
            messages: Vec::new(),
            fail_snapshot: true,
        };
        let failing_navigation = RecordingNavigation {
            emails: Mutex::new(Vec::new()),
            threads: Mutex::new(Vec::new()),
            fail: true,
        };

        assert_eq!(
            open_persisted_account(&unavailable_accounts, &failing_navigation, account_id),
            Err(OpenAccountCommandError::internal())
        );

        let accounts = SnapshotAccounts {
            accounts: vec![account],
            messages: Vec::new(),
            fail_snapshot: false,
        };
        let error = open_persisted_account(&accounts, &failing_navigation, account_id)
            .expect_err("native handoff should fail");
        assert_eq!(error, OpenAccountCommandError::navigation_failed());
        let rendered = format!("{error:?}");
        assert!(!rendered.contains("sensitive+person@example.test"));
        assert!(!rendered.contains(account_id));
        assert!(!rendered.contains("mail.google.com"));
    }

    #[test]
    fn open_account_enqueues_its_resolved_account_even_when_navigation_fails() {
        let account_id = "00000000-0000-4000-8000-000000000001";
        let accounts = SnapshotAccounts {
            accounts: vec![MailAccount::new(
                account_id.into(),
                "private@example.test".into(),
                None,
            )],
            messages: Vec::new(),
            fail_snapshot: false,
        };
        let navigation = RecordingNavigation {
            emails: Mutex::new(Vec::new()),
            threads: Mutex::new(Vec::new()),
            fail: true,
        };
        let (poll_events, mut poll_events_rx) = mpsc::unbounded_channel();

        assert_eq!(
            open_persisted_account_and_poll(&accounts, &navigation, &poll_events, account_id,),
            Err(OpenAccountCommandError::navigation_failed())
        );
        assert!(matches!(
            poll_events_rx.try_recv(),
            Ok(PollSchedulerEvent::PollNowAccount(requested)) if requested == account_id
        ));
    }

    #[test]
    fn check_mail_now_error_is_fixed_minimal_and_redacted() {
        let value = serde_json::to_value(CheckMailNowCommandError::internal())
            .expect("serialize manual-check error");
        assert_eq!(
            value,
            serde_json::json!({
                "tag": "internal",
                "message": "Mail check is unavailable.",
                "retryable": true,
                "actionRequired": false
            })
        );
        let serialized = value.to_string().to_ascii_lowercase();
        for forbidden in ["account", "email", "provider", "token", "cursor", "url"] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn manual_check_acknowledges_enqueue_and_fails_closed_when_scheduler_is_gone() {
        let (poll_events, mut poll_events_rx) = mpsc::unbounded_channel();
        request_all_polls_now(&poll_events).expect("scheduler accepts manual check");
        assert!(matches!(
            poll_events_rx.try_recv(),
            Ok(PollSchedulerEvent::PollNowAll)
        ));

        drop(poll_events_rx);
        assert_eq!(
            request_all_polls_now(&poll_events),
            Err(CheckMailNowCommandError::internal())
        );
    }

    #[test]
    fn notification_activation_revalidates_message_and_keeps_navigation_best_effort() {
        let existing_id =
            Uuid::parse_str("00000000-0000-4000-8000-000000000001").expect("valid existing UUID");
        let removed_id =
            Uuid::parse_str("00000000-0000-4000-8000-000000000002").expect("valid removed UUID");
        let other_id =
            Uuid::parse_str("00000000-0000-4000-8000-000000000003").expect("valid other UUID");
        let accounts = SnapshotAccounts {
            accounts: vec![
                MailAccount::new(
                    existing_id.hyphenated().to_string(),
                    "private@example.test".into(),
                    None,
                ),
                MailAccount::new(
                    other_id.hyphenated().to_string(),
                    "other@example.test".into(),
                    None,
                ),
            ],
            messages: vec![
                notification_message(&existing_id.hyphenated().to_string(), "message-1"),
                notification_message(&other_id.hyphenated().to_string(), "message-other"),
            ],
            fail_snapshot: false,
        };
        let navigation = RecordingNavigation {
            emails: Mutex::new(Vec::new()),
            threads: Mutex::new(Vec::new()),
            fail: true,
        };
        let (poll_events, mut poll_events_rx) = mpsc::unbounded_channel();

        let removed_target = NotificationActivationTarget::new(removed_id, "message-1")
            .expect("valid removed target");
        activate_persisted_notification_message(
            &accounts,
            &navigation,
            &poll_events,
            &removed_target,
        );
        assert!(navigation.emails.lock().expect("navigation log").is_empty());
        assert!(poll_events_rx.try_recv().is_err());

        for message_id in ["missing", "message-other"] {
            let invalid_target = NotificationActivationTarget::new(existing_id, message_id)
                .expect("valid but unresolved target");
            activate_persisted_notification_message(
                &accounts,
                &navigation,
                &poll_events,
                &invalid_target,
            );
        }
        assert!(navigation.emails.lock().expect("navigation log").is_empty());
        assert!(poll_events_rx.try_recv().is_err());

        let unavailable_accounts = SnapshotAccounts {
            accounts: Vec::new(),
            messages: Vec::new(),
            fail_snapshot: true,
        };
        let existing_target = NotificationActivationTarget::new(existing_id, "message-1")
            .expect("valid existing target");
        activate_persisted_notification_message(
            &unavailable_accounts,
            &navigation,
            &poll_events,
            &existing_target,
        );
        assert!(navigation.emails.lock().expect("navigation log").is_empty());
        assert!(poll_events_rx.try_recv().is_err());

        activate_persisted_notification_message(
            &accounts,
            &navigation,
            &poll_events,
            &existing_target,
        );
        assert_eq!(
            navigation.emails.lock().expect("navigation log").as_slice(),
            ["private@example.test"]
        );
        assert_eq!(
            navigation.threads.lock().expect("thread log").as_slice(),
            ["thread-message-1"]
        );
        assert!(matches!(
            poll_events_rx.try_recv(),
            Ok(PollSchedulerEvent::PollNowAccount(requested))
                if requested == existing_id.hyphenated().to_string()
        ));
    }

    #[test]
    fn startup_snapshot_keeps_the_approved_accounts_and_messages_shape() {
        let account_id = "00000000-0000-4000-8000-000000000001";
        let snapshot = AppSnapshot {
            accounts: vec![MailAccount::new(
                account_id.into(),
                "person@example.test".into(),
                None,
            )],
            messages: vec![MailMessage::detected(
                "message-1".into(),
                "thread-1".into(),
                account_id.into(),
                "Sender <sender@example.test>".into(),
                "Subject".into(),
                "2026-04-12T11:59:00Z".into(),
                "2026-04-12T12:00:00Z".into(),
            )
            .expect("valid message")],
        };
        let value = serde_json::to_value(snapshot).expect("serialize snapshot");

        assert_eq!(value["accounts"].as_array().map(Vec::len), Some(1));
        assert_eq!(value["messages"].as_array().map(Vec::len), Some(1));
        assert_eq!(value["messages"][0]["accountId"], account_id);
        assert_eq!(value["messages"][0]["threadId"], "thread-1");
        assert!(value["accounts"][0].get("refreshToken").is_none());
        assert!(value["messages"][0].get("body").is_none());
    }

    #[test]
    fn tray_projection_sums_only_active_account_unread_counts() {
        let account = |id: &str, active: bool, pending: u64, unread: Option<u64>| {
            serde_json::from_value::<MailAccount>(serde_json::json!({
                "id": id,
                "email": format!("{id}@example.test"),
                "pendingCount": pending,
                "inboxUnreadCount": unread,
                "active": active
            }))
            .expect("valid account fixture")
        };
        let accounts = vec![
            account("active-a", true, 0, Some(3)),
            account("active-b", true, 2, None),
            account("inactive", false, 99, Some(40)),
        ];

        assert_eq!(
            tray_state_for_accounts(&accounts),
            PetTrayState {
                account_count: 2,
                pending_total: 2,
                inbox_unread_total: 3,
            }
        );
    }

    #[test]
    fn sync_status_projection_is_minimal_sanitized_and_account_scoped() {
        let account_id = "00000000-0000-4000-8000-000000000001";
        let retrying = serde_json::to_value(SyncStatusChanged::failed(
            account_id,
            Duration::from_secs(75),
            false,
        ))
        .expect("serialize retry status");
        assert_eq!(
            retrying,
            serde_json::json!({
                "accountId": account_id,
                "status": "retrying",
                "retryAfterSeconds": 75
            })
        );

        let action_required = serde_json::to_value(SyncStatusChanged::failed(
            account_id,
            Duration::from_secs(300),
            true,
        ))
        .expect("serialize action status");
        assert_eq!(
            action_required,
            serde_json::json!({
                "accountId": account_id,
                "status": "action_required"
            })
        );
        let serialized = action_required.to_string();
        for forbidden in ["email", "provider", "message", "token", "cursor", "url"] {
            assert!(!serialized.to_ascii_lowercase().contains(forbidden));
        }
    }

    #[test]
    fn successful_poll_projects_healthy_for_only_the_same_account() {
        let account_id = "00000000-0000-4000-8000-000000000001";
        let healthy = serde_json::to_value(SyncStatusChanged::healthy(account_id))
            .expect("serialize healthy status");

        assert_eq!(
            healthy,
            serde_json::json!({
                "accountId": account_id,
                "status": "healthy"
            })
        );
    }

    #[test]
    fn committed_messages_notify_between_snapshot_publication_and_healthy_status() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let notifications = RecordingNotifications {
            events: events.clone(),
            fail: false,
        };
        let messages = vec![
            notification_message("00000000-0000-4000-8000-000000000001", "message-1"),
            notification_message("00000000-0000-4000-8000-000000000001", "message-2"),
        ];
        let accounts = [notification_account("00000000-0000-4000-8000-000000000001")];

        complete_successful_poll(
            &messages,
            &accounts,
            &notifications,
            || events.lock().expect("event log").push("snapshot"),
            || events.lock().expect("event log").push("healthy"),
        );

        assert_eq!(
            *events.lock().expect("event log"),
            ["snapshot", "notify", "notify", "healthy"]
        );
    }

    #[test]
    fn notification_rejection_is_best_effort_and_does_not_block_healthy_status() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let notifications = RecordingNotifications {
            events: events.clone(),
            fail: true,
        };
        let messages = vec![notification_message(
            "00000000-0000-4000-8000-000000000001",
            "message-1",
        )];
        let accounts = [notification_account("00000000-0000-4000-8000-000000000001")];

        complete_successful_poll(
            &messages,
            &accounts,
            &notifications,
            || events.lock().expect("event log").push("snapshot"),
            || events.lock().expect("event log").push("healthy"),
        );

        assert_eq!(
            *events.lock().expect("event log"),
            ["snapshot", "notify", "healthy"]
        );
    }

    #[test]
    fn unread_only_poll_publishes_without_creating_a_notification() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let notifications = RecordingNotifications {
            events: events.clone(),
            fail: false,
        };

        complete_successful_poll(
            &[],
            &[],
            &notifications,
            || events.lock().expect("event log").push("snapshot"),
            || events.lock().expect("event log").push("healthy"),
        );

        assert_eq!(*events.lock().expect("event log"), ["snapshot", "healthy"]);
    }

    #[test]
    fn committed_unread_snapshot_is_published_before_retry_status() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let snapshot = AppSnapshot {
            accounts: Vec::new(),
            messages: Vec::new(),
        };

        complete_failed_poll(
            Some(snapshot),
            |_| events.lock().expect("event log").push("snapshot"),
            || events.lock().expect("event log").push("retrying"),
        );

        assert_eq!(*events.lock().expect("event log"), ["snapshot", "retrying"]);
    }

    #[test]
    fn an_in_flight_account_does_not_block_a_new_due_account() {
        let now = Instant::now();
        let next_poll_by_account = HashMap::from([
            ("slow".to_owned(), now),
            ("new".to_owned(), now),
            ("later".to_owned(), now + Duration::from_secs(30)),
        ]);
        let in_flight = HashSet::from(["slow".to_owned()]);

        assert_eq!(
            due_account_ids(&next_poll_by_account, &in_flight, now),
            ["new"]
        );
    }

    #[test]
    fn immediate_all_advances_only_eligible_idle_accounts_and_coalesces_in_flight() {
        let now = Instant::now();
        let later = now + Duration::from_secs(30);
        // The scheduler map is already reconciled from eligible_account_ids, so removed/inactive
        // accounts are deliberately absent and cannot be forced by this zero-argument event.
        let mut next_poll_by_account = HashMap::from([
            ("idle-a".to_owned(), later),
            ("idle-b".to_owned(), later),
            ("in-flight".to_owned(), later),
        ]);
        let mut in_flight = HashSet::from(["in-flight".to_owned()]);
        let mut poll_again_after_flight = HashSet::new();
        let mut paused = false;

        apply_scheduler_event(
            PollSchedulerEvent::PollNowAll,
            &mut next_poll_by_account,
            &mut in_flight,
            &mut poll_again_after_flight,
            &mut paused,
            now,
        );

        assert_eq!(next_poll_by_account["idle-a"], now);
        assert_eq!(next_poll_by_account["idle-b"], now);
        assert_eq!(next_poll_by_account["in-flight"], later);
        assert_eq!(
            poll_again_after_flight,
            HashSet::from(["in-flight".to_owned()])
        );
        assert!(!next_poll_by_account.contains_key("removed-or-inactive"));
    }

    #[test]
    fn immediate_requests_coalesce_to_one_trailing_poll_for_an_in_flight_account() {
        let now = Instant::now();
        let later = now + Duration::from_secs(30);
        let mut next_poll_by_account =
            HashMap::from([("idle".to_owned(), later), ("in-flight".to_owned(), later)]);
        let mut in_flight = HashSet::from(["in-flight".to_owned()]);
        let mut poll_again_after_flight = HashSet::new();
        let mut paused = false;

        for account_id in ["idle", "idle", "unknown", "in-flight"] {
            apply_scheduler_event(
                PollSchedulerEvent::PollNowAccount(account_id.to_owned()),
                &mut next_poll_by_account,
                &mut in_flight,
                &mut poll_again_after_flight,
                &mut paused,
                now,
            );
        }

        assert_eq!(next_poll_by_account["idle"], now);
        assert_eq!(next_poll_by_account["in-flight"], later);
        assert!(!next_poll_by_account.contains_key("unknown"));
        assert_eq!(
            poll_again_after_flight,
            HashSet::from(["in-flight".to_owned()])
        );

        apply_scheduler_event(
            PollSchedulerEvent::Completed {
                account_id: "in-flight".to_owned(),
                next_poll_after: Duration::from_secs(300),
            },
            &mut next_poll_by_account,
            &mut in_flight,
            &mut poll_again_after_flight,
            &mut paused,
            now,
        );

        assert_eq!(next_poll_by_account["in-flight"], now);
        assert!(poll_again_after_flight.is_empty());
    }

    #[test]
    fn completion_never_reinserts_an_account_removed_while_in_flight() {
        let now = Instant::now();
        let mut next_poll_by_account = HashMap::new();
        let mut in_flight = HashSet::from(["removed".to_owned()]);
        let mut poll_again_after_flight = HashSet::from(["removed".to_owned()]);
        let mut paused = false;

        apply_scheduler_event(
            PollSchedulerEvent::Completed {
                account_id: "removed".to_owned(),
                next_poll_after: Duration::from_secs(30),
            },
            &mut next_poll_by_account,
            &mut in_flight,
            &mut poll_again_after_flight,
            &mut paused,
            now,
        );

        assert!(!next_poll_by_account.contains_key("removed"));
        assert!(!in_flight.contains("removed"));
        assert!(!poll_again_after_flight.contains("removed"));
    }

    #[test]
    fn sleep_pauses_dispatch_and_wake_makes_idle_accounts_due_now() {
        let now = Instant::now();
        let later = now + Duration::from_secs(30);
        let mut next_poll_by_account =
            HashMap::from([("idle".to_owned(), later), ("in-flight".to_owned(), later)]);
        let mut in_flight = HashSet::from(["in-flight".to_owned()]);
        let mut poll_again_after_flight = HashSet::new();
        let mut paused = false;

        apply_scheduler_event(
            PollSchedulerEvent::Pause,
            &mut next_poll_by_account,
            &mut in_flight,
            &mut poll_again_after_flight,
            &mut paused,
            now,
        );
        assert!(paused);
        assert!(due_account_ids(&next_poll_by_account, &in_flight, now).is_empty());

        apply_scheduler_event(
            PollSchedulerEvent::WakeAndCatchUp,
            &mut next_poll_by_account,
            &mut in_flight,
            &mut poll_again_after_flight,
            &mut paused,
            now,
        );

        assert!(!paused);
        assert_eq!(next_poll_by_account["idle"], now);
        assert_eq!(
            poll_again_after_flight,
            HashSet::from(["in-flight".to_owned()])
        );
    }

    #[test]
    fn queued_sleep_is_drained_before_due_accounts_are_selected() {
        let now = Instant::now();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        sender.send(PollSchedulerEvent::Pause).expect("queue pause");
        let mut next_poll_by_account = HashMap::from([("due".to_owned(), now)]);
        let mut in_flight = HashSet::new();
        let mut poll_again_after_flight = HashSet::new();
        let mut paused = false;

        drain_pending_scheduler_events(
            &mut receiver,
            &mut next_poll_by_account,
            &mut in_flight,
            &mut poll_again_after_flight,
            &mut paused,
            now,
        );

        assert!(paused);
        let dispatch = if paused {
            Vec::new()
        } else {
            due_account_ids(&next_poll_by_account, &in_flight, now)
        };
        assert!(dispatch.is_empty());
    }

    #[test]
    fn oauth_completion_always_restores_and_success_publishes_before_polling() {
        assert_eq!(
            oauth_completion_actions(false),
            [OAuthCompletionAction::RestoreWindow]
        );
        assert_eq!(
            oauth_completion_actions(true),
            [
                OAuthCompletionAction::RestoreWindow,
                OAuthCompletionAction::PublishSnapshot,
                OAuthCompletionAction::WakePolling,
            ]
        );
    }

    #[test]
    fn add_account_provider_defaults_to_gmail_and_rejects_unknown_providers() {
        assert_eq!(add_account_provider(None).unwrap(), MailProvider::Gmail);
        assert_eq!(
            add_account_provider(Some("gmail")).unwrap(),
            MailProvider::Gmail
        );
        assert_eq!(
            add_account_provider(Some("outlook")).unwrap(),
            MailProvider::Outlook
        );
        for unknown in ["icloud", "Outlook", ""] {
            assert_eq!(
                add_account_provider(Some(unknown)).unwrap_err().tag(),
                AddAccountErrorTag::ConfigInvalid
            );
        }
    }
}
