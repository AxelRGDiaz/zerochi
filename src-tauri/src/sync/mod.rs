//! Read-only Gmail synchronization orchestration.
//!
//! Baselines persist the bounded unread-id set and cursor without importing historical pending.

#![allow(dead_code)]

use crate::{
    accounts::{AccountStatePort, MailAccount, MailMessage, MailProvider, MailStateSnapshot},
    auth::{
        ClientIdPort, ClientSecretPort, RefreshTokenReadError, RefreshTokenReadPort,
        SensitiveString,
    },
    gmail::{
        AccessTokenRefreshPort, GmailHistoryEvent, GmailMessageMetadata, GmailReadError,
        GmailReadPort,
    },
};
use chrono::{SecondsFormat, Utc};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

pub(crate) type SyncFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub(crate) const HEALTHY_POLL_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SyncError;

impl fmt::Display for SyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Mail synchronization is temporarily unavailable")
    }
}

impl std::error::Error for SyncError {}

pub(crate) trait InitialSyncPort: Send + Sync {
    fn baseline<'a>(
        &'a self,
        account: MailAccount,
        access_token: &'a SensitiveString,
    ) -> SyncFuture<'a, Result<MailAccount, SyncError>>;
}

pub(crate) trait SyncClock: Send + Sync {
    fn now(&self) -> Result<String, SyncError>;
}

pub(crate) struct SystemSyncClock;

impl SyncClock for SystemSyncClock {
    fn now(&self) -> Result<String, SyncError> {
        Ok(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true))
    }
}

/// Per-account guarded baseline coordinator. A second operation for the same account fails
/// closed instead of issuing overlapping Gmail requests or racing cursor commits.
pub(crate) struct GmailSyncEngine {
    gmail: Arc<dyn GmailReadPort>,
    accounts: Arc<dyn AccountStatePort>,
    clock: Arc<dyn SyncClock>,
    active_accounts: Mutex<HashSet<String>>,
    incremental: Option<IncrementalPorts>,
    failures: Mutex<HashMap<String, u32>>,
}

struct IncrementalPorts {
    client_id: Arc<dyn ClientIdPort>,
    client_secret: Arc<dyn ClientSecretPort>,
    refresh_tokens: Arc<dyn RefreshTokenReadPort>,
    access_tokens: Arc<dyn AccessTokenRefreshPort>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PollErrorKind {
    Busy,
    AuthRequired,
    Transient,
    State,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PollError {
    kind: PollErrorKind,
    retry_after: Duration,
    committed_snapshot: Option<MailStateSnapshot>,
}

impl PollError {
    pub(crate) fn retry_after(&self) -> Duration {
        self.retry_after
    }

    pub(crate) fn action_required(&self) -> bool {
        self.kind == PollErrorKind::AuthRequired
    }

    pub(crate) fn committed_snapshot(&self) -> Option<&MailStateSnapshot> {
        self.committed_snapshot.as_ref()
    }

    fn with_committed_snapshot(mut self, snapshot: MailStateSnapshot) -> Self {
        self.committed_snapshot = Some(snapshot);
        self
    }
}

impl fmt::Debug for PollError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PollError")
            .field("kind", &self.kind)
            .field("retry_after", &self.retry_after)
            .field("has_committed_snapshot", &self.committed_snapshot.is_some())
            .finish()
    }
}

impl fmt::Display for PollError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Mail polling is temporarily unavailable")
    }
}

impl std::error::Error for PollError {}

pub(crate) struct PollOutcome {
    pub(crate) snapshot: MailStateSnapshot,
    pub(crate) new_messages: Vec<MailMessage>,
    pub(crate) next_poll_after: Duration,
}

pub(crate) trait IncrementalSyncPort: Send + Sync {
    fn eligible_account_ids(&self) -> Result<Vec<String>, PollError>;

    fn poll_account<'a>(
        &'a self,
        account_id: &'a str,
    ) -> SyncFuture<'a, Result<PollOutcome, PollError>>;
}

/// Sends each account to the engine wired with its provider's adapters. Both engines share one
/// account state, so either can enumerate eligibility.
pub(crate) struct ProviderRoutingSync {
    accounts: Arc<dyn AccountStatePort>,
    gmail: Arc<dyn IncrementalSyncPort>,
    outlook: Arc<dyn IncrementalSyncPort>,
}

impl ProviderRoutingSync {
    pub(crate) fn new(
        accounts: Arc<dyn AccountStatePort>,
        gmail: Arc<dyn IncrementalSyncPort>,
        outlook: Arc<dyn IncrementalSyncPort>,
    ) -> Self {
        Self {
            accounts,
            gmail,
            outlook,
        }
    }
}

impl IncrementalSyncPort for ProviderRoutingSync {
    fn eligible_account_ids(&self) -> Result<Vec<String>, PollError> {
        self.gmail.eligible_account_ids()
    }

    fn poll_account<'a>(
        &'a self,
        account_id: &'a str,
    ) -> SyncFuture<'a, Result<PollOutcome, PollError>> {
        let provider = self.accounts.snapshot().ok().and_then(|accounts| {
            accounts
                .iter()
                .find(|account| account.id() == account_id)
                .map(MailAccount::provider)
        });
        // A missing account falls through to the default engine, which reports it as state.
        match provider {
            Some(MailProvider::Outlook) => self.outlook.poll_account(account_id),
            Some(MailProvider::Gmail) | None => self.gmail.poll_account(account_id),
        }
    }
}

impl GmailSyncEngine {
    pub(crate) fn new(
        gmail: Arc<dyn GmailReadPort>,
        accounts: Arc<dyn AccountStatePort>,
        clock: Arc<dyn SyncClock>,
    ) -> Self {
        Self {
            gmail,
            accounts,
            clock,
            active_accounts: Mutex::new(HashSet::new()),
            incremental: None,
            failures: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn with_incremental(
        mut self,
        client_id: Arc<dyn ClientIdPort>,
        client_secret: Arc<dyn ClientSecretPort>,
        refresh_tokens: Arc<dyn RefreshTokenReadPort>,
        access_tokens: Arc<dyn AccessTokenRefreshPort>,
    ) -> Self {
        self.incremental = Some(IncrementalPorts {
            client_id,
            client_secret,
            refresh_tokens,
            access_tokens,
        });
        self
    }

    fn begin(&self, account_id: &str) -> Result<(), SyncError> {
        let mut active = self.active_accounts.lock().map_err(|_| SyncError)?;
        if !active.insert(account_id.to_owned()) {
            return Err(SyncError);
        }
        Ok(())
    }

    fn finish(&self, account_id: &str) {
        if let Ok(mut active) = self.active_accounts.lock() {
            active.remove(account_id);
        }
    }

    fn poll_failure(&self, account_id: &str, kind: PollErrorKind) -> PollError {
        let attempt = self
            .failures
            .lock()
            .ok()
            .map(|mut failures| {
                let attempt = failures.entry(account_id.to_owned()).or_insert(0);
                let current = *attempt;
                *attempt = attempt.saturating_add(1).min(10);
                current
            })
            .unwrap_or(0);
        let base = match kind {
            PollErrorKind::Busy => 2,
            PollErrorKind::AuthRequired => 300,
            PollErrorKind::Transient | PollErrorKind::State => {
                5_u64.saturating_mul(1_u64 << attempt.min(6)).min(300)
            }
        };
        let jitter = stable_jitter(account_id, attempt) % (base / 4 + 1);
        PollError {
            kind,
            retry_after: Duration::from_secs((base + jitter).min(300)),
            committed_snapshot: None,
        }
    }

    fn reset_failure(&self, account_id: &str) {
        if let Ok(mut failures) = self.failures.lock() {
            failures.remove(account_id);
        }
    }
}

fn stable_jitter(account_id: &str, attempt: u32) -> u64 {
    account_id
        .bytes()
        .fold(0xcbf29ce484222325_u64 ^ u64::from(attempt), |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        })
}

impl InitialSyncPort for GmailSyncEngine {
    fn baseline<'a>(
        &'a self,
        account: MailAccount,
        access_token: &'a SensitiveString,
    ) -> SyncFuture<'a, Result<MailAccount, SyncError>> {
        Box::pin(async move {
            let account_id = account.id().to_owned();
            self.begin(&account_id)?;
            let result = async {
                if account.pending_count() != 0 {
                    return Err(SyncError);
                }
                let profile = self
                    .gmail
                    .profile(access_token)
                    .await
                    .map_err(|_| SyncError)?;
                let unread_message_ids = self
                    .gmail
                    .unread_message_ids(access_token)
                    .await
                    .map_err(|_| SyncError)?;
                let synced_at = self.clock.now()?;
                self.accounts
                    .commit_baseline(
                        &account_id,
                        account.last_history_id(),
                        &profile.email,
                        profile.history_id,
                        synced_at,
                        unread_message_ids,
                    )
                    .await
                    .map_err(|_| SyncError)
            }
            .await;
            self.finish(&account_id);
            result
        })
    }
}

impl IncrementalSyncPort for GmailSyncEngine {
    fn eligible_account_ids(&self) -> Result<Vec<String>, PollError> {
        self.accounts
            .snapshot()
            .map_err(|_| self.poll_failure("state", PollErrorKind::State))
            .map(|accounts| {
                accounts
                    .into_iter()
                    .filter(|account| account.is_active())
                    .map(|account| account.id().to_owned())
                    .collect()
            })
    }

    fn poll_account<'a>(
        &'a self,
        account_id: &'a str,
    ) -> SyncFuture<'a, Result<PollOutcome, PollError>> {
        Box::pin(async move {
            let ports = self
                .incremental
                .as_ref()
                .ok_or_else(|| self.poll_failure(account_id, PollErrorKind::State))?;
            self.begin(account_id)
                .map_err(|_| self.poll_failure(account_id, PollErrorKind::Busy))?;
            let result = async {
                let account = self
                    .accounts
                    .snapshot()
                    .map_err(|_| self.poll_failure(account_id, PollErrorKind::State))?
                    .into_iter()
                    .find(|account| account.id() == account_id && account.is_active())
                    .ok_or_else(|| self.poll_failure(account_id, PollErrorKind::State))?;
                let expected_cursor = account.last_history_id().map(str::to_owned);
                let account_uuid = uuid::Uuid::parse_str(account_id)
                    .map_err(|_| self.poll_failure(account_id, PollErrorKind::State))?;
                let refresh_token = ports
                    .refresh_tokens
                    .get(account_uuid)
                    .await
                    .map_err(|error| match error {
                        RefreshTokenReadError::AccessDeniedOrLocked => {
                            self.poll_failure(account_id, PollErrorKind::AuthRequired)
                        }
                        RefreshTokenReadError::Unavailable => {
                            self.poll_failure(account_id, PollErrorKind::Transient)
                        }
                    })?
                    .ok_or_else(|| self.poll_failure(account_id, PollErrorKind::AuthRequired))?;
                let client_id = ports
                    .client_id
                    .load()
                    .map_err(|_| self.poll_failure(account_id, PollErrorKind::State))?;
                let client_secret = ports
                    .client_secret
                    .load()
                    .map_err(|_| self.poll_failure(account_id, PollErrorKind::State))?;
                let access_token = ports
                    .access_tokens
                    .refresh_access_token(&client_id, &client_secret, refresh_token.as_slice())
                    .await
                    .map_err(|error| self.map_gmail_error(account_id, error))?;
                let detected_at = self
                    .clock
                    .now()
                    .map_err(|_| self.poll_failure(account_id, PollErrorKind::State))?;
                let Some(expected_cursor) = expected_cursor else {
                    return self
                        .full_resync(account_id, None, &access_token, detected_at)
                        .await;
                };
                let changes = match self
                    .gmail
                    .changes_since(&access_token, &expected_cursor)
                    .await
                {
                    Ok(changes) => changes,
                    Err(GmailReadError::HistoryExpired) => {
                        return self
                            .full_resync(
                                account_id,
                                Some(&expected_cursor),
                                &access_token,
                                detected_at,
                            )
                            .await;
                    }
                    Err(error) => return Err(self.map_gmail_error(account_id, error)),
                };
                // History is the source of newly-added candidates and the next cursor, but it is
                // not authoritative for current label membership: one history record can report
                // several kinds of change for the same message without ordering those arrays.
                // Reconcile the full bounded INBOX + UNREAD set on every successful poll so local
                // state cannot drift after read, re-unread, archive, trash, or delete operations.
                let mut unread: HashSet<String> = self
                    .gmail
                    .unread_message_ids(&access_token)
                    .await
                    .map_err(|error| self.map_gmail_error(account_id, error))?
                    .into_iter()
                    .collect();
                let existing_messages: HashSet<String> = self
                    .accounts
                    .mail_snapshot()
                    .map_err(|_| self.poll_failure(account_id, PollErrorKind::State))?
                    .messages
                    .into_iter()
                    .filter(|message| message.account_id() == account_id)
                    .map(|message| message.id().to_owned())
                    .collect();
                let added = message_added_candidates(changes.events);
                let mut messages = Vec::new();
                for (id, expected_thread_id) in added {
                    if !unread.contains(&id) || existing_messages.contains(&id) {
                        continue;
                    }
                    let Some(message) = self
                        .gmail
                        .message_metadata(&access_token, &id)
                        .await
                        .map_err(|error| self.map_gmail_error(account_id, error))?
                    else {
                        unread.remove(&id);
                        continue;
                    };
                    if !message.is_inbox_unread {
                        unread.remove(&id);
                        continue;
                    }
                    if message.thread_id != expected_thread_id {
                        return Err(self.poll_failure(account_id, PollErrorKind::State));
                    }
                    messages.push(
                        normalize_message(account_id, &detected_at, message)
                            .map_err(|_| self.poll_failure(account_id, PollErrorKind::State))?,
                    );
                }
                let mut unread_message_ids: Vec<_> = unread.into_iter().collect();
                unread_message_ids.sort_unstable();
                let committed = self
                    .accounts
                    .commit_incremental(
                        account_id,
                        &expected_cursor,
                        changes.cursor,
                        detected_at,
                        messages,
                        unread_message_ids,
                    )
                    .await
                    .map_err(|_| self.poll_failure(account_id, PollErrorKind::State))?;
                let snapshot = self
                    .accounts
                    .mail_snapshot()
                    .map_err(|_| self.poll_failure(account_id, PollErrorKind::State))?;
                self.reset_failure(account_id);
                Ok(PollOutcome {
                    snapshot,
                    new_messages: committed.inserted_messages,
                    next_poll_after: HEALTHY_POLL_INTERVAL,
                })
            }
            .await;
            let result = result.map_err(|error| match self.accounts.mail_snapshot() {
                Ok(snapshot) => error.with_committed_snapshot(snapshot),
                Err(_) => error,
            });
            self.finish(account_id);
            result
        })
    }
}

impl GmailSyncEngine {
    async fn full_resync(
        &self,
        account_id: &str,
        expected_cursor: Option<&str>,
        access_token: &SensitiveString,
        synced_at: String,
    ) -> Result<PollOutcome, PollError> {
        let profile = self
            .gmail
            .profile(access_token)
            .await
            .map_err(|error| self.map_gmail_error(account_id, error))?;
        let unread_message_ids = self
            .gmail
            .unread_message_ids(access_token)
            .await
            .map_err(|error| self.map_gmail_error(account_id, error))?;
        self.accounts
            .commit_baseline(
                account_id,
                expected_cursor,
                &profile.email,
                profile.history_id,
                synced_at,
                unread_message_ids,
            )
            .await
            .map_err(|_| self.poll_failure(account_id, PollErrorKind::State))?;
        let snapshot = self
            .accounts
            .mail_snapshot()
            .map_err(|_| self.poll_failure(account_id, PollErrorKind::State))?;
        self.reset_failure(account_id);
        Ok(PollOutcome {
            snapshot,
            new_messages: Vec::new(),
            next_poll_after: HEALTHY_POLL_INTERVAL,
        })
    }

    fn map_gmail_error(&self, account_id: &str, error: GmailReadError) -> PollError {
        match error {
            GmailReadError::AuthRequired => {
                self.poll_failure(account_id, PollErrorKind::AuthRequired)
            }
            GmailReadError::Unavailable => self.poll_failure(account_id, PollErrorKind::Transient),
            GmailReadError::HistoryExpired | GmailReadError::InvalidResponse => {
                self.poll_failure(account_id, PollErrorKind::State)
            }
        }
    }
}

fn normalize_message(
    account_id: &str,
    detected_at: &str,
    message: GmailMessageMetadata,
) -> Result<MailMessage, crate::accounts::AccountStateError> {
    MailMessage::detected(
        message.id,
        message.thread_id,
        account_id.to_owned(),
        message.sender,
        message.subject,
        message.received_at,
        detected_at.to_owned(),
    )
}

fn message_added_candidates(events: Vec<GmailHistoryEvent>) -> Vec<(String, String)> {
    let mut added = Vec::new();
    let mut seen_added = HashSet::new();
    for event in events {
        if let GmailHistoryEvent::MessageAdded { id, thread_id, .. } = event {
            if seen_added.insert(id.clone()) {
                added.push((id, thread_id));
            }
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accounts::{AccountStatePort, InMemoryAccountState},
        auth::{
            AddAccountError, ClientId, ClientSecret, ClientSecretPort, Environment, PortFuture,
            RefreshTokenReadError, RefreshTokenReadPort,
        },
        gmail::{
            AccessTokenRefreshPort, GmailChanges, GmailFuture, GmailHistoryEvent,
            GmailMessageMetadata, GmailProfile, GmailReadError,
        },
    };
    use std::ffi::OsString;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    };
    use uuid::Uuid;
    use zeroize::Zeroizing;

    struct Gmail(Result<String, GmailReadError>);

    impl GmailReadPort for Gmail {
        fn profile<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
        ) -> GmailFuture<'a, Result<GmailProfile, GmailReadError>> {
            let result = self.0.clone();
            Box::pin(async move {
                result.map(|history_id| GmailProfile {
                    email: "person@example.test".into(),
                    history_id,
                })
            })
        }

        fn unread_message_ids<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
        ) -> GmailFuture<'a, Result<Vec<String>, GmailReadError>> {
            Box::pin(async { Ok((0..9).map(|index| format!("unread-{index}")).collect()) })
        }
    }

    struct PollGmail {
        initial: Result<String, GmailReadError>,
        changes: Result<GmailChanges, GmailReadError>,
        unread: Vec<String>,
    }

    struct MetadataRaceGmail {
        final_inbox: bool,
        final_unread: bool,
    }

    impl GmailReadPort for MetadataRaceGmail {
        fn profile<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
        ) -> GmailFuture<'a, Result<GmailProfile, GmailReadError>> {
            Box::pin(async {
                Ok(GmailProfile {
                    email: "person@example.test".into(),
                    history_id: "999".into(),
                })
            })
        }

        fn unread_message_ids<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
        ) -> GmailFuture<'a, Result<Vec<String>, GmailReadError>> {
            Box::pin(async { Ok(unread_ids(&["baseline-a", "baseline-b", "message-1"])) })
        }

        fn changes_since<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
            _cursor: &'a str,
        ) -> GmailFuture<'a, Result<GmailChanges, GmailReadError>> {
            Box::pin(async {
                Ok(GmailChanges {
                    cursor: "101".into(),
                    events: vec![GmailHistoryEvent::MessageAdded {
                        id: "message-1".into(),
                        thread_id: "thread-1".into(),
                        label_ids: vec!["INBOX".into(), "UNREAD".into()],
                    }],
                })
            })
        }

        fn message_metadata<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
            message_id: &'a str,
        ) -> GmailFuture<'a, Result<Option<GmailMessageMetadata>, GmailReadError>> {
            let id = message_id.to_owned();
            let is_inbox_unread = self.final_inbox && self.final_unread;
            Box::pin(async move {
                Ok(Some(GmailMessageMetadata {
                    id,
                    thread_id: "thread-1".into(),
                    sender: "Sender <sender@example.test>".into(),
                    subject: "Subject".into(),
                    received_at: "2026-04-12T11:59:00Z".into(),
                    is_inbox_unread,
                }))
            })
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FailureStage {
        Refresh,
        Profile,
        UnreadList,
        History,
        Metadata,
    }

    struct FailingGmail {
        stage: FailureStage,
        error: GmailReadError,
    }

    impl GmailReadPort for FailingGmail {
        fn profile<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
        ) -> GmailFuture<'a, Result<GmailProfile, GmailReadError>> {
            let result = if self.stage == FailureStage::Profile {
                Err(self.error)
            } else {
                Ok(GmailProfile {
                    email: "person@example.test".into(),
                    history_id: "999".into(),
                })
            };
            Box::pin(async move { result })
        }

        fn unread_message_ids<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
        ) -> GmailFuture<'a, Result<Vec<String>, GmailReadError>> {
            let result = if self.stage == FailureStage::UnreadList {
                Err(self.error)
            } else {
                Ok(if self.stage == FailureStage::Metadata {
                    vec!["message-1".into()]
                } else {
                    vec!["provider-unread".into()]
                })
            };
            Box::pin(async move { result })
        }

        fn changes_since<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
            _cursor: &'a str,
        ) -> GmailFuture<'a, Result<GmailChanges, GmailReadError>> {
            let result = if self.stage == FailureStage::History {
                Err(self.error)
            } else {
                Ok(GmailChanges {
                    cursor: "101".into(),
                    events: if self.stage == FailureStage::Metadata {
                        vec![GmailHistoryEvent::MessageAdded {
                            id: "message-1".into(),
                            thread_id: "thread-1".into(),
                            label_ids: vec!["UNREAD".into()],
                        }]
                    } else {
                        Vec::new()
                    },
                })
            };
            Box::pin(async move { result })
        }

        fn message_metadata<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
            _message_id: &'a str,
        ) -> GmailFuture<'a, Result<Option<GmailMessageMetadata>, GmailReadError>> {
            let error = self.error;
            Box::pin(async move { Err(error) })
        }
    }

    struct CountingUnreadGmail {
        reads: AtomicU64,
    }

    impl GmailReadPort for CountingUnreadGmail {
        fn profile<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
        ) -> GmailFuture<'a, Result<GmailProfile, GmailReadError>> {
            Box::pin(async {
                Ok(GmailProfile {
                    email: "person@example.test".into(),
                    history_id: "100".into(),
                })
            })
        }

        fn unread_message_ids<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
        ) -> GmailFuture<'a, Result<Vec<String>, GmailReadError>> {
            Box::pin(async { Ok(vec!["baseline-a".into(), "baseline-b".into()]) })
        }

        fn changes_since<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
            _cursor: &'a str,
        ) -> GmailFuture<'a, Result<GmailChanges, GmailReadError>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok(GmailChanges {
                    cursor: "100".into(),
                    events: Vec::new(),
                })
            })
        }
    }

    struct BlockingGmail {
        started: mpsc::Sender<()>,
        resume: Mutex<mpsc::Receiver<()>>,
    }

    impl GmailReadPort for BlockingGmail {
        fn profile<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
        ) -> GmailFuture<'a, Result<GmailProfile, GmailReadError>> {
            Box::pin(async {
                Ok(GmailProfile {
                    email: "person@example.test".into(),
                    history_id: "999".into(),
                })
            })
        }

        fn unread_message_ids<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
        ) -> GmailFuture<'a, Result<Vec<String>, GmailReadError>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn changes_since<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
            _cursor: &'a str,
        ) -> GmailFuture<'a, Result<GmailChanges, GmailReadError>> {
            Box::pin(async move {
                self.started.send(()).expect("signal poll");
                self.resume
                    .lock()
                    .map_err(|_| GmailReadError::Unavailable)?
                    .recv()
                    .map_err(|_| GmailReadError::Unavailable)?;
                Ok(GmailChanges {
                    cursor: "101".into(),
                    events: vec![GmailHistoryEvent::MessageAdded {
                        id: "racing-message".into(),
                        thread_id: "racing-thread".into(),
                        label_ids: vec!["UNREAD".into()],
                    }],
                })
            })
        }

        fn message_metadata<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
            message_id: &'a str,
        ) -> GmailFuture<'a, Result<Option<GmailMessageMetadata>, GmailReadError>> {
            let id = message_id.to_owned();
            Box::pin(async move {
                Ok(Some(GmailMessageMetadata {
                    id,
                    thread_id: "racing-thread".into(),
                    sender: "Sender <sender@example.test>".into(),
                    subject: "Subject".into(),
                    received_at: "2026-04-12T11:59:00Z".into(),
                    is_inbox_unread: true,
                }))
            })
        }
    }

    impl GmailReadPort for PollGmail {
        fn profile<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
        ) -> GmailFuture<'a, Result<GmailProfile, GmailReadError>> {
            let result = self.initial.clone();
            Box::pin(async move {
                result.map(|history_id| GmailProfile {
                    email: "person@example.test".into(),
                    history_id,
                })
            })
        }

        fn unread_message_ids<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
        ) -> GmailFuture<'a, Result<Vec<String>, GmailReadError>> {
            let unread = self.unread.clone();
            Box::pin(async move { Ok(unread) })
        }

        fn changes_since<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
            _cursor: &'a str,
        ) -> GmailFuture<'a, Result<GmailChanges, GmailReadError>> {
            let result = self.changes.clone();
            Box::pin(async move { result })
        }

        fn message_metadata<'a>(
            &'a self,
            _access_token: &'a SensitiveString,
            message_id: &'a str,
        ) -> GmailFuture<'a, Result<Option<GmailMessageMetadata>, GmailReadError>> {
            let id = message_id.to_owned();
            Box::pin(async move {
                Ok(Some(GmailMessageMetadata {
                    thread_id: id.replace("message", "thread"),
                    id,
                    sender: "Sender <sender@example.test>".into(),
                    subject: "Subject".into(),
                    received_at: "2026-04-12T11:59:00Z".into(),
                    is_inbox_unread: true,
                }))
            })
        }
    }

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

    struct ConfigSecret;
    impl ClientSecretPort for ConfigSecret {
        fn load(&self) -> Result<ClientSecret, AddAccountError> {
            struct Env;
            impl Environment for Env {
                fn var_os(&self, _name: &str) -> Option<OsString> {
                    Some("fixture-client-secret-value".into())
                }
            }
            ClientSecret::load_from(&Env)
        }
    }

    struct RefreshTokens;
    impl RefreshTokenReadPort for RefreshTokens {
        fn get<'a>(
            &'a self,
            _account_id: Uuid,
        ) -> PortFuture<'a, Result<Option<Zeroizing<Vec<u8>>>, RefreshTokenReadError>> {
            Box::pin(async { Ok(Some(Zeroizing::new(b"fixture-refresh".to_vec()))) })
        }
    }

    struct MissingTokens;
    impl RefreshTokenReadPort for MissingTokens {
        fn get<'a>(
            &'a self,
            _account_id: Uuid,
        ) -> PortFuture<'a, Result<Option<Zeroizing<Vec<u8>>>, RefreshTokenReadError>> {
            Box::pin(async { Ok(None) })
        }
    }

    struct LockedTokens;
    impl RefreshTokenReadPort for LockedTokens {
        fn get<'a>(
            &'a self,
            _account_id: Uuid,
        ) -> PortFuture<'a, Result<Option<Zeroizing<Vec<u8>>>, RefreshTokenReadError>> {
            Box::pin(async { Err(RefreshTokenReadError::AccessDeniedOrLocked) })
        }
    }

    struct AccessTokens(Result<(), GmailReadError>);
    impl AccessTokenRefreshPort for AccessTokens {
        fn refresh_access_token<'a>(
            &'a self,
            _client_id: &'a ClientId,
            client_secret: &'a ClientSecret,
            refresh_token: &'a [u8],
        ) -> GmailFuture<'a, Result<SensitiveString, GmailReadError>> {
            assert_eq!(refresh_token, b"fixture-refresh");
            assert_eq!(client_secret.expose(), "fixture-client-secret-value");
            let result = self.0;
            Box::pin(async move { result.map(|_| SensitiveString::new("fixture-access".into())) })
        }
    }

    struct Clock;
    impl SyncClock for Clock {
        fn now(&self) -> Result<String, SyncError> {
            Ok("2026-04-12T12:00:00Z".into())
        }
    }

    fn account() -> MailAccount {
        MailAccount::new(
            "00000000-0000-4000-8000-000000000001".into(),
            "person@example.test".into(),
            Some("Person".into()),
        )
    }

    #[test]
    fn baseline_commits_only_cursor_and_time_and_keeps_historical_pending_zero() {
        let accounts = Arc::new(InMemoryAccountState::default());
        let original = account();
        tauri::async_runtime::block_on(accounts.commit(original.clone())).expect("commit account");
        let engine = GmailSyncEngine::new(
            Arc::new(Gmail(Ok("123456789".into()))),
            accounts.clone(),
            Arc::new(Clock),
        );
        let token = SensitiveString::new("fixture-access".into());

        let updated =
            tauri::async_runtime::block_on(engine.baseline(original, &token)).expect("baseline");
        let value = serde_json::to_value(updated).expect("serialize account");
        assert_eq!(value["pendingCount"], 0);
        assert_eq!(value["inboxUnreadCount"], 9);
        assert_eq!(value["lastHistoryId"], "123456789");
        assert_eq!(value["lastSyncAt"], "2026-04-12T12:00:00Z");
        assert_eq!(accounts.snapshot().expect("snapshot").len(), 1);
    }

    #[test]
    fn baseline_profile_failure_preserves_the_uninitialized_retryable_account() {
        let accounts = Arc::new(InMemoryAccountState::default());
        let original = account();
        tauri::async_runtime::block_on(accounts.commit(original.clone())).expect("commit account");
        let engine = GmailSyncEngine::new(
            Arc::new(Gmail(Err(GmailReadError::Unavailable))),
            accounts.clone(),
            Arc::new(Clock),
        );
        let token = SensitiveString::new("fixture-access".into());

        assert!(tauri::async_runtime::block_on(engine.baseline(original, &token)).is_err());
        let value = serde_json::to_value(&accounts.snapshot().expect("snapshot")[0])
            .expect("serialize account");
        assert_eq!(value["pendingCount"], 0);
        assert!(value.get("inboxUnreadCount").is_none());
        assert!(value.get("lastHistoryId").is_none());
    }

    #[test]
    fn per_account_guard_rejects_an_overlapping_baseline() {
        let accounts = Arc::new(InMemoryAccountState::default());
        let original = account();
        tauri::async_runtime::block_on(accounts.commit(original.clone())).expect("commit account");
        let engine = GmailSyncEngine::new(
            Arc::new(Gmail(Ok("123456789".into()))),
            accounts,
            Arc::new(Clock),
        );
        engine.begin(original.id()).expect("first guard");
        let token = SensitiveString::new("fixture-access".into());
        assert!(tauri::async_runtime::block_on(engine.baseline(original.clone(), &token)).is_err());
        engine.finish(original.id());
    }

    #[test]
    fn production_clock_format_is_utc_rfc3339() {
        let timestamp = SystemSyncClock.now().expect("timestamp");
        assert!(timestamp.ends_with('Z'));
        assert!(chrono::DateTime::parse_from_rfc3339(&timestamp).is_ok());
    }

    fn incremental_engine(
        accounts: Arc<InMemoryAccountState>,
        gmail: PollGmail,
        access_result: Result<(), GmailReadError>,
    ) -> GmailSyncEngine {
        GmailSyncEngine::new(gmail_arc(gmail), accounts, Arc::new(Clock)).with_incremental(
            Arc::new(Config),
            Arc::new(ConfigSecret),
            Arc::new(RefreshTokens),
            Arc::new(AccessTokens(access_result)),
        )
    }

    fn gmail_arc(gmail: PollGmail) -> Arc<dyn GmailReadPort> {
        Arc::new(gmail)
    }

    fn unread_ids(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    fn baselined_accounts() -> (Arc<InMemoryAccountState>, MailAccount) {
        let accounts = Arc::new(InMemoryAccountState::default());
        let original = account();
        tauri::async_runtime::block_on(accounts.commit(original.clone())).expect("commit account");
        let baselined = tauri::async_runtime::block_on(accounts.commit_baseline(
            original.id(),
            None,
            original.email(),
            "100".into(),
            "2026-04-12T12:00:00Z".into(),
            vec!["baseline-a".into(), "baseline-b".into()],
        ))
        .expect("baseline");
        (accounts, baselined)
    }

    #[test]
    fn one_shot_poll_refreshes_fetches_and_atomically_exposes_pending_message() {
        let (accounts, baselined) = baselined_accounts();
        let engine = incremental_engine(
            accounts,
            PollGmail {
                initial: Ok("999".into()),
                unread: unread_ids(&["baseline-a", "baseline-b", "message-1"]),
                changes: Ok(GmailChanges {
                    cursor: "101".into(),
                    events: vec![
                        GmailHistoryEvent::MessageAdded {
                            id: "message-1".into(),
                            thread_id: "thread-1".into(),
                            label_ids: vec!["UNREAD".into()],
                        },
                        GmailHistoryEvent::MessageAdded {
                            id: "message-1".into(),
                            thread_id: "thread-1".into(),
                            label_ids: vec!["UNREAD".into()],
                        },
                    ],
                }),
            },
            Ok(()),
        );

        assert_eq!(
            engine.eligible_account_ids().expect("eligible"),
            vec![baselined.id().to_owned()]
        );
        let outcome =
            tauri::async_runtime::block_on(engine.poll_account(baselined.id())).expect("poll");
        assert_eq!(outcome.next_poll_after, HEALTHY_POLL_INTERVAL);
        assert_eq!(outcome.next_poll_after, Duration::from_secs(30));
        assert_eq!(outcome.snapshot.messages.len(), 1);
        assert_eq!(outcome.snapshot.accounts[0].pending_count(), 1);
        assert_eq!(outcome.snapshot.accounts[0].inbox_unread_count(), Some(3));
        assert_eq!(outcome.snapshot.accounts[0].last_history_id(), Some("101"));
        assert_eq!(outcome.new_messages.len(), 1);
        assert_eq!(outcome.new_messages[0].id(), "message-1");

        let replay = tauri::async_runtime::block_on(engine.poll_account(baselined.id()))
            .expect("duplicate replay poll");
        assert!(replay.new_messages.is_empty());
        assert_eq!(replay.snapshot.messages.len(), 1);
    }

    #[test]
    fn history_candidates_are_deduplicated_without_deriving_unread_membership() {
        let added = message_added_candidates(vec![
            GmailHistoryEvent::LabelsRemoved {
                id: "read-now".into(),
                label_ids: vec!["UNREAD".into()],
            },
            GmailHistoryEvent::LabelsAdded {
                id: "old-message".into(),
                label_ids: vec!["UNREAD".into()],
            },
            GmailHistoryEvent::LabelsAdded {
                id: "old-message".into(),
                label_ids: vec!["UNREAD".into()],
            },
            GmailHistoryEvent::MessageAdded {
                id: "new-read".into(),
                thread_id: "new-read-thread".into(),
                label_ids: vec!["UNREAD".into()],
            },
            GmailHistoryEvent::LabelsRemoved {
                id: "new-read".into(),
                label_ids: vec!["UNREAD".into()],
            },
            GmailHistoryEvent::MessageAdded {
                id: "new-unread".into(),
                thread_id: "new-unread-thread".into(),
                label_ids: vec!["UNREAD".into()],
            },
            GmailHistoryEvent::MessageAdded {
                id: "new-unread".into(),
                thread_id: "new-unread-thread".into(),
                label_ids: vec!["UNREAD".into()],
            },
            GmailHistoryEvent::MessageDeleted {
                id: "deleted".into(),
            },
        ]);

        assert_eq!(
            added,
            [
                ("new-read".into(), "new-read-thread".into()),
                ("new-unread".into(), "new-unread-thread".into())
            ]
        );
    }

    #[test]
    fn every_poll_reconciles_inbox_unread_authoritatively_for_mailbox_changes() {
        struct Scenario {
            name: &'static str,
            event: GmailHistoryEvent,
            authoritative_ids: Vec<String>,
            expected_count: u64,
        }

        for scenario in [
            Scenario {
                name: "read",
                event: GmailHistoryEvent::LabelsRemoved {
                    id: "baseline-a".into(),
                    label_ids: vec!["UNREAD".into()],
                },
                authoritative_ids: unread_ids(&["baseline-b"]),
                expected_count: 1,
            },
            Scenario {
                name: "mark-unread-again",
                event: GmailHistoryEvent::LabelsAdded {
                    id: "older-message".into(),
                    label_ids: vec!["UNREAD".into()],
                },
                authoritative_ids: unread_ids(&["baseline-a", "baseline-b", "older-message"]),
                expected_count: 3,
            },
            Scenario {
                name: "archive-unread",
                event: GmailHistoryEvent::LabelsRemoved {
                    id: "baseline-a".into(),
                    label_ids: vec!["INBOX".into()],
                },
                authoritative_ids: unread_ids(&["baseline-b"]),
                expected_count: 1,
            },
            Scenario {
                name: "trash",
                event: GmailHistoryEvent::LabelsRemoved {
                    id: "baseline-a".into(),
                    label_ids: vec!["INBOX".into(), "UNREAD".into()],
                },
                authoritative_ids: unread_ids(&["baseline-b"]),
                expected_count: 1,
            },
            Scenario {
                name: "delete",
                event: GmailHistoryEvent::MessageDeleted {
                    id: "baseline-a".into(),
                },
                authoritative_ids: unread_ids(&["baseline-b"]),
                expected_count: 1,
            },
        ] {
            let (accounts, baselined) = baselined_accounts();
            let engine = incremental_engine(
                accounts.clone(),
                PollGmail {
                    initial: Ok("999".into()),
                    unread: scenario.authoritative_ids.clone(),
                    changes: Ok(GmailChanges {
                        cursor: "101".into(),
                        events: vec![scenario.event],
                    }),
                },
                Ok(()),
            );

            let outcome = tauri::async_runtime::block_on(engine.poll_account(baselined.id()))
                .unwrap_or_else(|_| panic!("{} poll", scenario.name));

            assert_eq!(
                outcome.snapshot.accounts[0].inbox_unread_count(),
                Some(scenario.expected_count),
                "{}",
                scenario.name
            );
            assert_eq!(
                accounts
                    .unread_message_ids(baselined.id())
                    .expect("durable ids"),
                scenario.authoritative_ids,
                "{}",
                scenario.name
            );
            assert!(outcome.new_messages.is_empty(), "{}", scenario.name);
            assert_eq!(outcome.snapshot.accounts[0].pending_count(), 0);
            assert_eq!(outcome.snapshot.accounts[0].last_history_id(), Some("101"));
        }
    }

    #[test]
    fn message_added_that_finishes_read_creates_no_pending_or_notification() {
        let (accounts, baselined) = baselined_accounts();
        let engine = incremental_engine(
            accounts,
            PollGmail {
                initial: Ok("999".into()),
                unread: unread_ids(&["baseline-a", "baseline-b"]),
                changes: Ok(GmailChanges {
                    cursor: "101".into(),
                    events: vec![
                        GmailHistoryEvent::MessageAdded {
                            id: "message-1".into(),
                            thread_id: "thread-1".into(),
                            label_ids: vec!["UNREAD".into()],
                        },
                        GmailHistoryEvent::LabelsRemoved {
                            id: "message-1".into(),
                            label_ids: vec!["UNREAD".into()],
                        },
                    ],
                }),
            },
            Ok(()),
        );

        let outcome =
            tauri::async_runtime::block_on(engine.poll_account(baselined.id())).expect("poll");
        assert!(outcome.new_messages.is_empty());
        assert!(outcome.snapshot.messages.is_empty());
        assert_eq!(outcome.snapshot.accounts[0].pending_count(), 0);
        assert_eq!(outcome.snapshot.accounts[0].inbox_unread_count(), Some(2));
    }

    #[test]
    fn metadata_recheck_closes_archive_and_read_races_after_authoritative_list() {
        for (name, final_inbox, final_unread) in [("archived", false, true), ("read", true, false)]
        {
            let (accounts, baselined) = baselined_accounts();
            let engine = GmailSyncEngine::new(
                Arc::new(MetadataRaceGmail {
                    final_inbox,
                    final_unread,
                }),
                accounts.clone(),
                Arc::new(Clock),
            )
            .with_incremental(
                Arc::new(Config),
                Arc::new(ConfigSecret),
                Arc::new(RefreshTokens),
                Arc::new(AccessTokens(Ok(()))),
            );

            let outcome = tauri::async_runtime::block_on(engine.poll_account(baselined.id()))
                .unwrap_or_else(|_| panic!("{name} race poll"));

            assert!(outcome.new_messages.is_empty(), "{name}");
            assert!(outcome.snapshot.messages.is_empty(), "{name}");
            assert_eq!(outcome.snapshot.accounts[0].pending_count(), 0, "{name}");
            assert_eq!(
                outcome.snapshot.accounts[0].inbox_unread_count(),
                Some(2),
                "{name}"
            );
            assert_eq!(
                accounts
                    .unread_message_ids(baselined.id())
                    .expect("durable unread"),
                unread_ids(&["baseline-a", "baseline-b"]),
                "{name}"
            );
            assert_eq!(outcome.snapshot.accounts[0].last_history_id(), Some("101"));
        }
    }

    #[test]
    fn multi_account_poll_publishes_three_unread_globally_without_touching_pending() {
        let (accounts, baselined) = baselined_accounts();
        let other = MailAccount::new(
            "00000000-0000-4000-8000-000000000002".into(),
            "other@example.test".into(),
            Some("Other".into()),
        );
        tauri::async_runtime::block_on(accounts.commit(other.clone())).expect("commit other");
        tauri::async_runtime::block_on(accounts.update_sync_cursor(
            other.id(),
            "200".into(),
            "2026-04-12T12:00:00Z".into(),
            0,
        ))
        .expect("baseline other");
        let engine = incremental_engine(
            accounts.clone(),
            PollGmail {
                initial: Ok("999".into()),
                changes: Ok(GmailChanges {
                    cursor: "101".into(),
                    events: vec![GmailHistoryEvent::LabelsAdded {
                        id: "older-unread".into(),
                        label_ids: vec!["UNREAD".into()],
                    }],
                }),
                unread: unread_ids(&["baseline-a", "baseline-b", "older-unread"]),
            },
            Ok(()),
        );

        let outcome = tauri::async_runtime::block_on(engine.poll_account(baselined.id()))
            .expect("poll account with three unread");

        assert_eq!(outcome.snapshot.accounts.len(), 2);
        assert_eq!(
            outcome
                .snapshot
                .accounts
                .iter()
                .find(|account| account.id() == baselined.id())
                .and_then(MailAccount::inbox_unread_count),
            Some(3)
        );
        assert_eq!(
            outcome
                .snapshot
                .accounts
                .iter()
                .map(|account| account.inbox_unread_count().unwrap_or(0))
                .sum::<u64>(),
            3
        );
        assert_eq!(
            outcome
                .snapshot
                .accounts
                .iter()
                .map(MailAccount::pending_count)
                .sum::<u64>(),
            0
        );
        assert_eq!(
            accounts.mail_snapshot().expect("persisted"),
            outcome.snapshot
        );
        assert!(outcome.new_messages.is_empty());
    }

    #[test]
    fn every_no_delta_poll_preserves_the_persisted_unread_set() {
        let (accounts, baselined) = baselined_accounts();
        let gmail = Arc::new(CountingUnreadGmail {
            reads: AtomicU64::new(0),
        });
        let engine = GmailSyncEngine::new(gmail.clone(), accounts.clone(), Arc::new(Clock))
            .with_incremental(
                Arc::new(Config),
                Arc::new(ConfigSecret),
                Arc::new(RefreshTokens),
                Arc::new(AccessTokens(Ok(()))),
            );

        let first = tauri::async_runtime::block_on(engine.poll_account(baselined.id()))
            .expect("first no-delta poll");
        let second = tauri::async_runtime::block_on(engine.poll_account(baselined.id()))
            .expect("second no-delta poll");

        assert_eq!(gmail.reads.load(Ordering::SeqCst), 2);
        assert_eq!(first.snapshot.accounts[0].inbox_unread_count(), Some(2));
        assert_eq!(second.snapshot.accounts[0].inbox_unread_count(), Some(2));
        assert_eq!(second.snapshot.accounts[0].last_history_id(), Some("100"));
        assert_eq!(second.snapshot.accounts[0].pending_count(), 0);
        assert_eq!(
            accounts.mail_snapshot().expect("persisted"),
            second.snapshot
        );
    }

    #[test]
    fn history_failure_carries_the_last_committed_snapshot_without_mutation() {
        let (accounts, baselined) = baselined_accounts();
        let engine = incremental_engine(
            accounts.clone(),
            PollGmail {
                initial: Ok("999".into()),
                unread: unread_ids(&["provider-unread"]),
                changes: Err(GmailReadError::Unavailable),
            },
            Ok(()),
        );

        let error = tauri::async_runtime::block_on(engine.poll_account(baselined.id()))
            .err()
            .expect("history failure");
        let committed = error
            .committed_snapshot()
            .expect("committed unread snapshot");

        assert_eq!(committed.accounts[0].inbox_unread_count(), Some(2));
        assert_eq!(committed.accounts[0].last_history_id(), Some("100"));
        assert!(committed.messages.is_empty());
        assert_eq!(accounts.mail_snapshot().expect("persisted"), *committed);
        assert!(error.retry_after() <= Duration::from_secs(300));
        let rendered = format!("{error:?} {error}");
        assert!(!rendered.contains(baselined.id()));
        assert!(!rendered.contains("13"));
    }

    #[test]
    fn authoritative_list_failure_does_not_advance_history_or_mutate_unread() {
        let (accounts, baselined) = baselined_accounts();
        let before = accounts.mail_snapshot().expect("before");
        let engine = GmailSyncEngine::new(
            Arc::new(FailingGmail {
                stage: FailureStage::UnreadList,
                error: GmailReadError::Unavailable,
            }),
            accounts.clone(),
            Arc::new(Clock),
        )
        .with_incremental(
            Arc::new(Config),
            Arc::new(ConfigSecret),
            Arc::new(RefreshTokens),
            Arc::new(AccessTokens(Ok(()))),
        );

        let error = tauri::async_runtime::block_on(engine.poll_account(baselined.id()))
            .err()
            .expect("authoritative list failure");

        assert!(!error.action_required());
        assert_eq!(error.committed_snapshot(), Some(&before));
        assert_eq!(accounts.mail_snapshot().expect("after"), before);
        assert_eq!(
            accounts
                .unread_message_ids(baselined.id())
                .expect("durable unread"),
            unread_ids(&["baseline-a", "baseline-b"])
        );
    }

    #[test]
    fn auth_and_retryable_provider_failures_preserve_durable_state() {
        for stage in [
            FailureStage::Refresh,
            FailureStage::Profile,
            FailureStage::UnreadList,
            FailureStage::History,
            FailureStage::Metadata,
        ] {
            for (provider_error, action_required) in [
                (GmailReadError::AuthRequired, true),
                (GmailReadError::Unavailable, false),
            ] {
                let (accounts, tested_account) = match stage {
                    FailureStage::Profile | FailureStage::UnreadList => {
                        let accounts = Arc::new(InMemoryAccountState::default());
                        let account = account();
                        tauri::async_runtime::block_on(accounts.commit(account.clone()))
                            .expect("commit uninitialized account");
                        (accounts, account)
                    }
                    _ => baselined_accounts(),
                };
                let before = accounts.mail_snapshot().expect("snapshot before failure");
                let refresh_result = if stage == FailureStage::Refresh {
                    Err(provider_error)
                } else {
                    Ok(())
                };
                let engine = GmailSyncEngine::new(
                    Arc::new(FailingGmail {
                        stage,
                        error: provider_error,
                    }),
                    accounts.clone(),
                    Arc::new(Clock),
                )
                .with_incremental(
                    Arc::new(Config),
                    Arc::new(ConfigSecret),
                    Arc::new(RefreshTokens),
                    Arc::new(AccessTokens(refresh_result)),
                );

                let error =
                    tauri::async_runtime::block_on(engine.poll_account(tested_account.id()))
                        .err()
                        .expect("poll failure");

                assert_eq!(error.action_required(), action_required, "{stage:?}");
                assert!(error.retry_after() <= Duration::from_secs(300));
                assert_eq!(error.committed_snapshot(), Some(&before));
                assert_eq!(
                    accounts.mail_snapshot().expect("state after failure"),
                    before
                );
                let rendered = format!("{error:?} {error}");
                for forbidden in [
                    tested_account.id(),
                    tested_account.email(),
                    "fixture-refresh",
                    "fixture-access",
                ] {
                    assert!(!rendered.contains(forbidden), "{stage:?}");
                }
            }
        }
    }

    #[test]
    fn expired_history_recovers_cursor_without_importing_historical_pending() {
        let (accounts, baselined) = baselined_accounts();
        let engine = incremental_engine(
            accounts,
            PollGmail {
                initial: Ok("999".into()),
                unread: (0..7).map(|index| format!("unread-{index}")).collect(),
                changes: Err(GmailReadError::HistoryExpired),
            },
            Ok(()),
        );

        let outcome = tauri::async_runtime::block_on(engine.poll_account(baselined.id()))
            .expect("recovery poll");
        assert!(outcome.snapshot.messages.is_empty());
        assert_eq!(outcome.snapshot.accounts[0].pending_count(), 0);
        assert_eq!(outcome.snapshot.accounts[0].inbox_unread_count(), Some(7));
        assert_eq!(outcome.snapshot.accounts[0].last_history_id(), Some("999"));
        assert!(outcome.new_messages.is_empty());
    }

    #[test]
    fn transient_poll_failures_return_bounded_increasing_backoff_without_secret_data() {
        let (accounts, baselined) = baselined_accounts();
        let engine = incremental_engine(
            accounts,
            PollGmail {
                initial: Ok("999".into()),
                unread: unread_ids(&["provider-unread"]),
                changes: Err(GmailReadError::Unavailable),
            },
            Err(GmailReadError::Unavailable),
        );
        let first = tauri::async_runtime::block_on(engine.poll_account(baselined.id()))
            .err()
            .expect("first failure");
        let second = tauri::async_runtime::block_on(engine.poll_account(baselined.id()))
            .err()
            .expect("second failure");
        assert!(second.retry_after() >= first.retry_after());
        assert!(second.retry_after() <= Duration::from_secs(300));
        assert!(!first.action_required());
        assert!(!format!("{first:?} {first}").contains("fixture-refresh"));
    }

    #[test]
    fn scheduler_retries_a_failed_post_oauth_baseline_without_historical_pending() {
        let accounts = Arc::new(InMemoryAccountState::default());
        let original = account();
        tauri::async_runtime::block_on(accounts.commit(original.clone())).expect("commit account");
        let engine = incremental_engine(
            accounts,
            PollGmail {
                initial: Ok("500".into()),
                unread: (0..7).map(|index| format!("unread-{index}")).collect(),
                changes: Err(GmailReadError::InvalidResponse),
            },
            Ok(()),
        );

        assert_eq!(
            engine.eligible_account_ids().expect("eligible"),
            vec![original.id().to_owned()]
        );
        let outcome = tauri::async_runtime::block_on(engine.poll_account(original.id()))
            .expect("baseline retry");
        assert!(outcome.snapshot.messages.is_empty());
        assert_eq!(outcome.snapshot.accounts[0].pending_count(), 0);
        assert_eq!(outcome.snapshot.accounts[0].inbox_unread_count(), Some(7));
        assert_eq!(outcome.snapshot.accounts[0].last_history_id(), Some("500"));
        assert!(outcome.new_messages.is_empty());
    }

    #[test]
    fn missing_refresh_token_is_action_required_and_contains_no_identity() {
        let (accounts, baselined) = baselined_accounts();
        let engine = GmailSyncEngine::new(
            Arc::new(PollGmail {
                initial: Ok("999".into()),
                unread: unread_ids(&["provider-unread"]),
                changes: Err(GmailReadError::Unavailable),
            }),
            accounts,
            Arc::new(Clock),
        )
        .with_incremental(
            Arc::new(Config),
            Arc::new(ConfigSecret),
            Arc::new(MissingTokens),
            Arc::new(AccessTokens(Ok(()))),
        );
        let error = tauri::async_runtime::block_on(engine.poll_account(baselined.id()))
            .err()
            .expect("missing credential");
        assert!(error.action_required());
        assert!(!format!("{error:?} {error}").contains(baselined.id()));

        let (accounts, baselined) = baselined_accounts();
        let locked = GmailSyncEngine::new(
            Arc::new(PollGmail {
                initial: Ok("999".into()),
                unread: unread_ids(&["provider-unread"]),
                changes: Err(GmailReadError::Unavailable),
            }),
            accounts,
            Arc::new(Clock),
        )
        .with_incremental(
            Arc::new(Config),
            Arc::new(ConfigSecret),
            Arc::new(LockedTokens),
            Arc::new(AccessTokens(Ok(()))),
        );
        let locked_error = tauri::async_runtime::block_on(locked.poll_account(baselined.id()))
            .err()
            .expect("locked credential");
        assert!(locked_error.action_required());
    }

    #[test]
    fn backoff_resets_after_success_and_poll_overlap_is_rejected() {
        let (accounts, baselined) = baselined_accounts();
        let engine = incremental_engine(
            accounts,
            PollGmail {
                initial: Ok("999".into()),
                unread: unread_ids(&["baseline-a", "baseline-b"]),
                changes: Ok(GmailChanges {
                    cursor: "101".into(),
                    events: Vec::new(),
                }),
            },
            Ok(()),
        );
        let first = engine.poll_failure(baselined.id(), PollErrorKind::Transient);
        let second = engine.poll_failure(baselined.id(), PollErrorKind::Transient);
        assert!(second.retry_after() >= first.retry_after());
        engine.reset_failure(baselined.id());
        let reset = engine.poll_failure(baselined.id(), PollErrorKind::Transient);
        assert_eq!(reset.retry_after(), first.retry_after());
        engine.reset_failure(baselined.id());

        engine.begin(baselined.id()).expect("occupy account");
        let busy = tauri::async_runtime::block_on(engine.poll_account(baselined.id()))
            .err()
            .expect("busy poll");
        assert_eq!(busy.kind, PollErrorKind::Busy);
        engine.finish(baselined.id());
    }

    #[test]
    fn account_removal_wins_over_a_poll_already_fetching_history() {
        let (accounts, baselined) = baselined_accounts();
        let (started_tx, started_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let engine = Arc::new(
            GmailSyncEngine::new(
                Arc::new(BlockingGmail {
                    started: started_tx,
                    resume: Mutex::new(resume_rx),
                }),
                accounts.clone(),
                Arc::new(Clock),
            )
            .with_incremental(
                Arc::new(Config),
                Arc::new(ConfigSecret),
                Arc::new(RefreshTokens),
                Arc::new(AccessTokens(Ok(()))),
            ),
        );
        let account_id = baselined.id().to_owned();
        let polling = {
            let engine = engine.clone();
            let account_id = account_id.clone();
            std::thread::spawn(move || {
                tauri::async_runtime::block_on(engine.poll_account(&account_id))
            })
        };
        started_rx.recv().expect("poll reached provider");
        tauri::async_runtime::block_on(
            accounts.remove_account(Uuid::parse_str(&account_id).expect("uuid")),
        )
        .expect("remove account");
        resume_tx.send(()).expect("resume poll");

        let error = polling
            .join()
            .expect("poll thread")
            .err()
            .expect("removed poll fails");
        let publishable = error
            .committed_snapshot()
            .expect("post-removal snapshot remains publishable");
        assert!(publishable.accounts.is_empty());
        assert!(publishable.messages.is_empty());
        let snapshot = accounts.mail_snapshot().expect("final snapshot");
        assert!(snapshot.accounts.is_empty());
        assert!(snapshot.messages.is_empty());
    }

    struct RoutingAccounts(Vec<MailAccount>);

    impl AccountStatePort for RoutingAccounts {
        fn snapshot(&self) -> Result<Vec<MailAccount>, crate::accounts::AccountStateError> {
            Ok(self.0.clone())
        }

        fn commit<'a>(
            &'a self,
            _account: MailAccount,
        ) -> crate::accounts::AccountFuture<
            'a,
            Result<MailAccount, crate::accounts::AccountStateError>,
        > {
            Box::pin(async { Err(crate::accounts::AccountStateError) })
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

    struct RecordingEngine {
        name: &'static str,
        polled: Arc<Mutex<Vec<(&'static str, String)>>>,
    }

    impl IncrementalSyncPort for RecordingEngine {
        fn eligible_account_ids(&self) -> Result<Vec<String>, PollError> {
            Ok(Vec::new())
        }

        fn poll_account<'a>(
            &'a self,
            account_id: &'a str,
        ) -> SyncFuture<'a, Result<PollOutcome, PollError>> {
            self.polled
                .lock()
                .expect("polled")
                .push((self.name, account_id.to_owned()));
            Box::pin(async {
                Err(PollError {
                    kind: PollErrorKind::Busy,
                    retry_after: Duration::from_secs(1),
                    committed_snapshot: None,
                })
            })
        }
    }

    #[test]
    fn routing_sends_each_account_only_to_its_provider_engine() {
        let gmail_id = "00000000-0000-4000-8000-000000000001";
        let outlook_id = "00000000-0000-4000-8000-000000000002";
        let missing_id = "00000000-0000-4000-8000-000000000003";
        let accounts = Arc::new(RoutingAccounts(vec![
            MailAccount::new(gmail_id.into(), "g@example.test".into(), None),
            MailAccount::with_provider(
                outlook_id.into(),
                "o@example.test".into(),
                None,
                MailProvider::Outlook,
            ),
        ]));
        let polled = Arc::new(Mutex::new(Vec::new()));
        let router = ProviderRoutingSync::new(
            accounts,
            Arc::new(RecordingEngine {
                name: "gmail",
                polled: polled.clone(),
            }),
            Arc::new(RecordingEngine {
                name: "outlook",
                polled: polled.clone(),
            }),
        );

        for account_id in [gmail_id, outlook_id, missing_id] {
            let _ = tauri::async_runtime::block_on(router.poll_account(account_id));
        }

        assert_eq!(
            &*polled.lock().expect("polled"),
            &[
                ("gmail", gmail_id.to_owned()),
                ("outlook", outlook_id.to_owned()),
                ("gmail", missing_id.to_owned()),
            ]
        );
    }
}
