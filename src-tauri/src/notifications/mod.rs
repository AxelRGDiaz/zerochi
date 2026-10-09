//! Privacy-preserving native new-mail notifications.
//!
//! The application coordinator supplies bounded, already-normalized presentation metadata and a
//! stable local account UUID and bounded message identifier. Activation correlation remains
//! process-local and never places an email address, URL, or provider identifier in the native
//! response identifier.

#![allow(dead_code)]

use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

const SUBJECT_FALLBACK: &str = "Sin asunto";
const SENDER_FALLBACK: &str = "Remitente desconocido";
const PRESENTATION_FIELD_MAX_BYTES: usize = 512;
const SOUND_FILE: &str = "notification-chime.wav";
const NATIVE_ID_PREFIX: &str = "zerochi.mail.";
const OPAQUE_ID_BYTES: usize = 32;
const RECENT_ID_LIMIT: usize = 4_096;
const ACTIVATION_MESSAGE_ID_MAX_BYTES: usize = 256;

/// Stable non-provider identifier used only for local suppression and activation.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct OpaqueNotificationId(String);

impl OpaqueNotificationId {
    /// Derives a one-way identifier from the normalized local message key.
    ///
    /// The raw account and provider message identifiers are not retained by the request or sent
    /// to UserNotifications.
    pub(crate) fn from_message_key(account_id: &str, message_id: &str) -> Self {
        let mut digest = Sha256::new();
        digest.update(b"zerochi.notification.v1\0");
        digest.update((account_id.len() as u64).to_be_bytes());
        digest.update(account_id.as_bytes());
        digest.update((message_id.len() as u64).to_be_bytes());
        digest.update(message_id.as_bytes());
        Self(hex_lower(&digest.finalize()))
    }

    fn native_identifier(&self) -> String {
        format!("{NATIVE_ID_PREFIX}{}", self.0)
    }

    fn is_native_identifier(value: &str) -> bool {
        value
            .strip_prefix(NATIVE_ID_PREFIX)
            .is_some_and(valid_hex_digest)
    }

    fn from_native_identifier(value: &str) -> Option<Self> {
        value
            .strip_prefix(NATIVE_ID_PREFIX)
            .filter(|digest| valid_hex_digest(digest))
            .map(|digest| Self(digest.to_owned()))
    }
}

impl fmt::Debug for OpaqueNotificationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OpaqueNotificationId([redacted])")
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn valid_hex_digest(value: &str) -> bool {
    value.len() == OPAQUE_ID_BYTES * 2
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NotificationError;

impl fmt::Display for NotificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Native notification is unavailable")
    }
}

impl std::error::Error for NotificationError {}

/// Process-local activation metadata. It is never serialized into the native notification.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct NotificationActivationTarget {
    account_id: Uuid,
    message_id: String,
}

impl NotificationActivationTarget {
    pub(crate) fn new(
        account_id: Uuid,
        message_id: impl Into<String>,
    ) -> Result<Self, NotificationError> {
        let message_id = message_id.into();
        if !valid_activation_message_id(&message_id) {
            return Err(NotificationError);
        }
        Ok(Self {
            account_id,
            message_id,
        })
    }

    pub(crate) fn account_id(&self) -> Uuid {
        self.account_id
    }

    pub(crate) fn message_id(&self) -> &str {
        &self.message_id
    }
}

impl fmt::Debug for NotificationActivationTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NotificationActivationTarget([redacted])")
    }
}

fn valid_activation_message_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= ACTIVATION_MESSAGE_ID_MAX_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[derive(Clone)]
pub(crate) struct NotificationRequest {
    id: OpaqueNotificationId,
    activation_target: NotificationActivationTarget,
    title: String,
    subtitle: String,
    body: String,
}

impl NotificationRequest {
    pub(crate) fn new(
        id: OpaqueNotificationId,
        activation_target: NotificationActivationTarget,
        account_email: impl Into<String>,
        subject: Option<&str>,
        sender: Option<&str>,
    ) -> Self {
        Self {
            id,
            activation_target,
            title: account_email.into(),
            subtitle: bounded_presentation_field(subject, SUBJECT_FALLBACK),
            body: bounded_presentation_field(sender, SENDER_FALLBACK),
        }
    }
}

impl fmt::Debug for NotificationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NotificationRequest([redacted])")
    }
}

fn bounded_presentation_field(value: Option<&str>, fallback: &str) -> String {
    let value = value
        .filter(|candidate| !candidate.is_empty())
        .unwrap_or(fallback);
    if value.len() <= PRESENTATION_FIELD_MAX_BYTES {
        return value.to_owned();
    }

    let mut boundary = PRESENTATION_FIELD_MAX_BYTES;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value[..boundary].to_owned()
}

pub(crate) trait NotificationService: Send + Sync {
    /// Schedules best-effort native delivery without waiting for permission UI or OS delivery.
    fn notify(&self, request: NotificationRequest) -> Result<(), NotificationError>;
}

trait NotificationBackend: Send + Sync {
    fn submit(&self, notification: NativeNotification) -> Result<(), NotificationError>;
}

#[derive(Clone, PartialEq, Eq)]
struct NativeNotification {
    id: String,
    title: String,
    subtitle: String,
    body: String,
    sound_file: &'static str,
}

impl fmt::Debug for NativeNotification {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NativeNotification([redacted])")
    }
}

struct RecentNotifications {
    set: HashSet<OpaqueNotificationId>,
    order: VecDeque<OpaqueNotificationId>,
    activation_targets: HashMap<OpaqueNotificationId, NotificationActivationTarget>,
}

impl RecentNotifications {
    fn new() -> Self {
        Self {
            set: HashSet::new(),
            order: VecDeque::new(),
            activation_targets: HashMap::new(),
        }
    }

    fn insert(&mut self, id: OpaqueNotificationId, target: NotificationActivationTarget) -> bool {
        if !self.set.insert(id.clone()) {
            return false;
        }
        self.activation_targets.insert(id.clone(), target);
        self.order.push_back(id);
        if self.order.len() > RECENT_ID_LIMIT {
            if let Some(expired) = self.order.pop_front() {
                self.set.remove(&expired);
                self.activation_targets.remove(&expired);
            }
        }
        true
    }

    fn remove(&mut self, id: &OpaqueNotificationId) {
        if self.set.remove(id) {
            self.order.retain(|candidate| candidate != id);
            self.activation_targets.remove(id);
        }
    }

    fn activation_target(&self, native_identifier: &str) -> Option<NotificationActivationTarget> {
        let id = OpaqueNotificationId::from_native_identifier(native_identifier)?;
        self.activation_targets.get(&id).cloned()
    }
}

/// Testable adapter that fixes presentation and adds bounded defensive duplicate suppression.
pub(crate) struct NativeNotificationService {
    backend: Box<dyn NotificationBackend>,
    recent: Arc<Mutex<RecentNotifications>>,
    on_activate: Arc<dyn Fn(NotificationActivationTarget) + Send + Sync>,
}

impl NativeNotificationService {
    fn with_backend(backend: Box<dyn NotificationBackend>) -> Self {
        Self::with_backend_and_activation(backend, Arc::new(|_| {}))
    }

    fn with_backend_and_activation(
        backend: Box<dyn NotificationBackend>,
        on_activate: Arc<dyn Fn(NotificationActivationTarget) + Send + Sync>,
    ) -> Self {
        Self {
            backend,
            recent: Arc::new(Mutex::new(RecentNotifications::new())),
            on_activate,
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn macos(
        app: tauri::AppHandle,
        on_activate: Arc<dyn Fn(NotificationActivationTarget) + Send + Sync>,
    ) -> Self {
        let recent = Arc::new(Mutex::new(RecentNotifications::new()));
        let activation_recent = recent.clone();
        let activation_callback = on_activate.clone();
        let native_activation = Arc::new(move |native_identifier: String| {
            activate_native_identifier(
                &activation_recent,
                &activation_callback,
                &native_identifier,
            );
        });
        Self {
            backend: Box::new(macos::MacOsNotificationBackend::new(app, native_activation)),
            recent,
            on_activate,
        }
    }

    fn activate(&self, native_identifier: &str) {
        activate_native_identifier(&self.recent, &self.on_activate, native_identifier);
    }
}

fn activate_native_identifier(
    recent: &Arc<Mutex<RecentNotifications>>,
    on_activate: &Arc<dyn Fn(NotificationActivationTarget) + Send + Sync>,
    native_identifier: &str,
) {
    let target = recent
        .lock()
        .ok()
        .and_then(|recent| recent.activation_target(native_identifier));
    if let Some(target) = target {
        on_activate(target);
    }
}

impl NotificationService for NativeNotificationService {
    fn notify(&self, request: NotificationRequest) -> Result<(), NotificationError> {
        let claimed = self
            .recent
            .lock()
            .map_err(|_| NotificationError)?
            .insert(request.id.clone(), request.activation_target.clone());
        if !claimed {
            return Ok(());
        }

        let notification = NativeNotification {
            id: request.id.native_identifier(),
            title: request.title,
            subtitle: request.subtitle,
            body: request.body,
            sound_file: SOUND_FILE,
        };
        if let Err(error) = self.backend.submit(notification) {
            // A synchronous adapter rejection never reached macOS, so callers may retry it.
            if let Ok(mut recent) = self.recent.lock() {
                recent.remove(&request.id);
            }
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use block2::RcBlock;
    use objc2::{
        define_class, msg_send, rc::Retained, runtime::Bool, runtime::ProtocolObject, AnyThread,
        DefinedClass,
    };
    use objc2_foundation::{NSError, NSObject, NSObjectProtocol, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
        UNNotificationPresentationOptions, UNNotificationRequest as NativeRequest,
        UNNotificationResponse, UNNotificationSound, UNUserNotificationCenter,
        UNUserNotificationCenterDelegate,
    };
    use std::{cell::RefCell, path::Path};

    #[derive(Clone)]
    struct DelegateIvars {
        activate: Arc<dyn Fn(String) + Send + Sync>,
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements and the ivars are Send + Sync.
        #[unsafe(super = NSObject)]
        #[thread_kind = AnyThread]
        #[ivars = DelegateIvars]
        struct NotificationDelegate;

        // SAFETY: NSObjectProtocol has no additional safety requirements.
        unsafe impl NSObjectProtocol for NotificationDelegate {}

        // SAFETY: Every implemented selector matches the generated UserNotifications signature.
        unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            fn will_present(
                &self,
                _center: &UNUserNotificationCenter,
                _notification: &UNNotification,
                completion: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
            ) {
                completion.call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List
                    | UNNotificationPresentationOptions::Sound,));
            }

            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            fn did_activate(
                &self,
                _center: &UNUserNotificationCenter,
                response: &UNNotificationResponse,
                completion: &block2::DynBlock<dyn Fn()>,
            ) {
                let identifier = response.notification().request().identifier().to_string();
                if OpaqueNotificationId::is_native_identifier(&identifier) {
                    (self.ivars().activate)(identifier);
                }
                completion.call(());
            }
        }
    );

    impl NotificationDelegate {
        fn new(activate: Arc<dyn Fn(String) + Send + Sync>) -> Retained<Self> {
            let allocated = Self::alloc().set_ivars(DelegateIvars { activate });
            // SAFETY: NSObject's `init` selector has the declared signature.
            unsafe { msg_send![super(allocated), init] }
        }
    }

    thread_local! {
        // UNUserNotificationCenter.delegate is weak. Tauri installs this on the main thread and
        // the thread-local retains it for the process lifetime without making ObjC objects cross
        // Rust thread-safety boundaries.
        static DELEGATE: RefCell<Option<Retained<NotificationDelegate>>> = const { RefCell::new(None) };
    }

    pub(super) struct MacOsNotificationBackend {
        app: tauri::AppHandle,
        bundled: bool,
    }

    impl MacOsNotificationBackend {
        pub(super) fn new(
            app: tauri::AppHandle,
            activate: Arc<dyn Fn(String) + Send + Sync>,
        ) -> Self {
            let bundled = std::env::current_exe()
                .ok()
                .as_deref()
                .is_some_and(is_app_bundle_executable);
            if !bundled {
                // `UNUserNotificationCenter::currentNotificationCenter` raises an Objective-C
                // exception for Tauri's unbundled `target/debug/zerochi` dev executable. Do not
                // touch UserNotifications at all in that process shape.
                return Self {
                    app,
                    bundled: false,
                };
            }

            let center = UNUserNotificationCenter::currentNotificationCenter();
            let delegate = NotificationDelegate::new(activate);
            center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            DELEGATE.with(|slot| slot.replace(Some(delegate)));
            Self { app, bundled }
        }
    }

    pub(super) fn is_app_bundle_executable(path: &Path) -> bool {
        let Some(macos_dir) = path.parent() else {
            return false;
        };
        let Some(contents_dir) = macos_dir.parent() else {
            return false;
        };
        let Some(app_dir) = contents_dir.parent() else {
            return false;
        };

        macos_dir.file_name().is_some_and(|name| name == "MacOS")
            && contents_dir
                .file_name()
                .is_some_and(|name| name == "Contents")
            && app_dir
                .extension()
                .is_some_and(|extension| extension == "app")
            && app_dir.file_stem().is_some_and(|stem| !stem.is_empty())
    }

    impl NotificationBackend for MacOsNotificationBackend {
        fn submit(&self, notification: NativeNotification) -> Result<(), NotificationError> {
            if !self.bundled {
                return Ok(());
            }
            self.app
                .run_on_main_thread(move || deliver(notification))
                .map_err(|_| NotificationError)
        }
    }

    fn deliver(notification: NativeNotification) {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let authorization = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
            if !granted.as_bool() {
                return;
            }

            let content = UNMutableNotificationContent::new();
            content.setTitle(&NSString::from_str(&notification.title));
            content.setSubtitle(&NSString::from_str(&notification.subtitle));
            content.setBody(&NSString::from_str(&notification.body));
            let sound_name = NSString::from_str(notification.sound_file);
            let sound = UNNotificationSound::soundNamed(&sound_name);
            content.setSound(Some(&sound));

            let request = NativeRequest::requestWithIdentifier_content_trigger(
                &NSString::from_str(&notification.id),
                &content,
                None,
            );
            center.addNotificationRequest_withCompletionHandler(&request, None);
        });
        UNUserNotificationCenter::currentNotificationCenter()
            .requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
                &authorization,
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account_id() -> Uuid {
        Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").expect("valid UUID")
    }

    fn activation_target(message_id: &str) -> NotificationActivationTarget {
        NotificationActivationTarget::new(account_id(), message_id).expect("activation target")
    }

    fn request(id: OpaqueNotificationId) -> NotificationRequest {
        NotificationRequest::new(
            id,
            activation_target("provider-message"),
            "cuenta@example.com",
            Some("Asunto importante"),
            Some("Remitente <sender@example.com>"),
        )
    }

    #[derive(Default)]
    struct RecordingBackend {
        notifications: Arc<Mutex<Vec<NativeNotification>>>,
        fail: bool,
    }

    impl NotificationBackend for RecordingBackend {
        fn submit(&self, notification: NativeNotification) -> Result<(), NotificationError> {
            if self.fail {
                return Err(NotificationError);
            }
            self.notifications
                .lock()
                .map_err(|_| NotificationError)?
                .push(notification);
            Ok(())
        }
    }

    #[test]
    fn stable_ids_are_opaque_and_domain_separated() {
        let first = OpaqueNotificationId::from_message_key("account-a", "message-1");
        let repeated = OpaqueNotificationId::from_message_key("account-a", "message-1");
        let other_account = OpaqueNotificationId::from_message_key("account-b", "message-1");

        assert_eq!(first, repeated);
        assert_ne!(first, other_account);
        assert!(valid_hex_digest(&first.0));
        assert!(!format!("{first:?}").contains("account-a"));
        assert!(!format!("{first:?}").contains("message-1"));
    }

    #[test]
    fn activation_target_accepts_only_bounded_provider_identifiers() {
        let maximum = "a".repeat(ACTIVATION_MESSAGE_ID_MAX_BYTES);
        let target = NotificationActivationTarget::new(account_id(), maximum.clone())
            .expect("maximum-length id");

        assert_eq!(target.account_id(), account_id());
        assert_eq!(target.message_id(), maximum);
        for invalid in [
            String::new(),
            "a".repeat(ACTIVATION_MESSAGE_ID_MAX_BYTES + 1),
            "contains/slash".to_owned(),
            "contains space".to_owned(),
            "contains\nnewline".to_owned(),
            "non-ascii-é".to_owned(),
        ] {
            assert_eq!(
                NotificationActivationTarget::new(account_id(), invalid),
                Err(NotificationError)
            );
        }
    }

    #[test]
    fn activation_target_debug_redacts_uuid_and_message_id() {
        let target = activation_target("sensitive-message-id");
        let rendered = format!("{target:?}");

        assert_eq!(rendered, "NotificationActivationTarget([redacted])");
        assert!(!rendered.contains("sensitive-message-id"));
        assert!(!rendered.contains(&account_id().to_string()));
    }

    #[test]
    fn native_fields_map_account_subject_sender_and_sound() {
        let notifications = Arc::new(Mutex::new(Vec::new()));
        let service = NativeNotificationService::with_backend(Box::new(RecordingBackend {
            notifications: notifications.clone(),
            fail: false,
        }));

        service
            .notify(request(OpaqueNotificationId::from_message_key(
                "private-account",
                "provider-message",
            )))
            .expect("notification accepted");

        let recorded = notifications.lock().expect("recorded notification");
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].title, "cuenta@example.com");
        assert_eq!(recorded[0].subtitle, "Asunto importante");
        assert_eq!(recorded[0].body, "Remitente <sender@example.com>");
        assert_eq!(recorded[0].sound_file, "notification-chime.wav");
    }

    #[test]
    fn empty_or_missing_metadata_uses_fixed_fallbacks() {
        let notifications = Arc::new(Mutex::new(Vec::new()));
        let service = NativeNotificationService::with_backend(Box::new(RecordingBackend {
            notifications: notifications.clone(),
            fail: false,
        }));

        service
            .notify(NotificationRequest::new(
                OpaqueNotificationId::from_message_key("a", "missing"),
                activation_target("missing"),
                "cuenta@example.com",
                None,
                Some(""),
            ))
            .expect("notification accepted");

        let recorded = notifications.lock().expect("recorded notification");
        assert_eq!(recorded[0].subtitle, "Sin asunto");
        assert_eq!(recorded[0].body, "Remitente desconocido");
    }

    #[test]
    fn subject_and_sender_are_truncated_on_unicode_boundaries() {
        let subject = format!("{}é", "a".repeat(511));
        let sender = "🦀".repeat(129);
        let request = NotificationRequest::new(
            OpaqueNotificationId::from_message_key("a", "large"),
            activation_target("large"),
            "cuenta@example.com",
            Some(&subject),
            Some(&sender),
        );

        assert_eq!(request.subtitle.len(), 511);
        assert!(request.subtitle.is_char_boundary(request.subtitle.len()));
        assert_eq!(request.body.len(), 512);
        assert_eq!(request.body.chars().count(), 128);
    }

    #[test]
    fn debug_output_redacts_presentation_and_activation_metadata() {
        let request = NotificationRequest::new(
            OpaqueNotificationId::from_message_key("private-account", "provider-message"),
            activation_target("provider-message"),
            "private@example.com",
            Some("private subject"),
            Some("private sender"),
        );
        let request_debug = format!("{request:?}");
        assert_eq!(request_debug, "NotificationRequest([redacted])");
        assert!(!request_debug.contains("private"));

        let native = NativeNotification {
            id: request.id.native_identifier(),
            title: request.title,
            subtitle: request.subtitle,
            body: request.body,
            sound_file: SOUND_FILE,
        };
        assert_eq!(format!("{native:?}"), "NativeNotification([redacted])");
    }

    #[test]
    fn repeated_stable_id_is_suppressed() {
        let notifications = Arc::new(Mutex::new(Vec::new()));
        let service = NativeNotificationService::with_backend(Box::new(RecordingBackend {
            notifications: notifications.clone(),
            fail: false,
        }));
        let request = request(OpaqueNotificationId::from_message_key("a", "m"));

        service.notify(request.clone()).expect("first delivery");
        service.notify(request).expect("duplicate suppressed");

        assert_eq!(notifications.lock().expect("notifications").len(), 1);
    }

    #[test]
    fn duplicate_id_cannot_replace_its_original_activation_target() {
        let activations = Arc::new(Mutex::new(Vec::new()));
        let recorded_activations = activations.clone();
        let service = NativeNotificationService::with_backend_and_activation(
            Box::new(RecordingBackend::default()),
            Arc::new(move |id| {
                recorded_activations
                    .lock()
                    .expect("activation recorder")
                    .push(id);
            }),
        );
        let id = OpaqueNotificationId::from_message_key("a", "duplicate");
        let native_identifier = id.native_identifier();
        let other_account =
            Uuid::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb").expect("valid other UUID");

        service
            .notify(request(id.clone()))
            .expect("first notification accepted");
        service
            .notify(NotificationRequest::new(
                id,
                NotificationActivationTarget::new(other_account, "other-message")
                    .expect("other target"),
                "other@example.com",
                Some("other subject"),
                Some("other sender"),
            ))
            .expect("duplicate suppressed");
        service.activate(&native_identifier);

        assert_eq!(
            activations.lock().expect("activations").as_slice(),
            &[activation_target("provider-message")]
        );
    }

    #[test]
    fn activation_correlation_is_bounded_with_recent_ids() {
        let mut recent = RecentNotifications::new();
        let expired = OpaqueNotificationId::from_message_key("a", "expired");
        assert!(recent.insert(expired.clone(), activation_target("expired")));

        for index in 0..RECENT_ID_LIMIT {
            assert!(recent.insert(
                OpaqueNotificationId::from_message_key("a", &format!("message-{index}")),
                activation_target(&format!("message-{index}")),
            ));
        }

        assert_eq!(recent.set.len(), RECENT_ID_LIMIT);
        assert_eq!(recent.order.len(), RECENT_ID_LIMIT);
        assert_eq!(recent.activation_targets.len(), RECENT_ID_LIMIT);
        assert_eq!(recent.activation_target(&expired.native_identifier()), None);
    }

    #[test]
    fn synchronous_rejection_can_be_retried() {
        let id = OpaqueNotificationId::from_message_key("a", "m");
        let failing = NativeNotificationService::with_backend(Box::new(RecordingBackend {
            notifications: Arc::new(Mutex::new(Vec::new())),
            fail: true,
        }));

        assert_eq!(failing.notify(request(id.clone())), Err(NotificationError));
        assert_eq!(failing.notify(request(id)), Err(NotificationError));
    }

    #[test]
    fn activation_accepts_only_zerochi_opaque_ids() {
        let id = OpaqueNotificationId::from_message_key("a", "m").native_identifier();
        assert!(OpaqueNotificationId::is_native_identifier(&id));
        assert!(!OpaqueNotificationId::is_native_identifier(
            "provider-message-id"
        ));
        assert!(!OpaqueNotificationId::is_native_identifier(
            "zerochi.mail.not-hex"
        ));
    }

    #[test]
    fn activation_correlates_only_a_known_local_uuid_and_calls_once() {
        let activations = Arc::new(Mutex::new(Vec::new()));
        let recorded_activations = activations.clone();
        let service = NativeNotificationService::with_backend_and_activation(
            Box::new(RecordingBackend::default()),
            Arc::new(move |id| {
                recorded_activations
                    .lock()
                    .expect("activation recorder")
                    .push(id);
            }),
        );
        let id = OpaqueNotificationId::from_message_key("private-account", "provider-message");
        let native_identifier = id.native_identifier();
        service.notify(request(id)).expect("notification accepted");

        service.activate(&native_identifier);
        service.activate("zerochi.mail.not-hex");
        service.activate(
            &OpaqueNotificationId::from_message_key("other", "unknown").native_identifier(),
        );

        assert_eq!(
            activations.lock().expect("activations").as_slice(),
            &[activation_target("provider-message")]
        );
        assert!(!native_identifier.contains("private-account"));
        assert!(!native_identifier.contains("provider-message"));
        assert!(!native_identifier.contains("@"));
    }

    #[test]
    fn rejected_notification_does_not_leave_an_activation_target() {
        let activations = Arc::new(Mutex::new(Vec::new()));
        let recorded_activations = activations.clone();
        let service = NativeNotificationService::with_backend_and_activation(
            Box::new(RecordingBackend {
                notifications: Arc::new(Mutex::new(Vec::new())),
                fail: true,
            }),
            Arc::new(move |id| {
                recorded_activations
                    .lock()
                    .expect("activation recorder")
                    .push(id);
            }),
        );
        let id = OpaqueNotificationId::from_message_key("a", "rejected");
        let native_identifier = id.native_identifier();

        assert_eq!(service.notify(request(id)), Err(NotificationError));
        service.activate(&native_identifier);

        assert!(activations.lock().expect("activations").is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn user_notifications_are_guarded_by_real_app_bundle_executable_shape() {
        use std::path::Path;

        assert!(macos::is_app_bundle_executable(Path::new(
            "/Applications/Zerochi.app/Contents/MacOS/zerochi"
        )));
        assert!(macos::is_app_bundle_executable(Path::new(
            "/tmp/build/Zerochi.app/Contents/MacOS/zerochi"
        )));
        assert!(!macos::is_app_bundle_executable(Path::new(
            "/workspace/src-tauri/target/debug/zerochi"
        )));
        assert!(!macos::is_app_bundle_executable(Path::new(
            "/tmp/Zerochi.app/zerochi"
        )));
        assert!(!macos::is_app_bundle_executable(Path::new(
            "/tmp/Zerochi/Contents/MacOS/zerochi"
        )));
    }
}
