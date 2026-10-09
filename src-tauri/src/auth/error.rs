use serde::Serialize;
use std::fmt;

/// Exact Rust mirror of the Lead-approved `AddAccountErrorTag` union.
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AddAccountErrorTag {
    AuthInProgress,
    BrowserOpenFailed,
    CallbackInvalid,
    Cancelled,
    ConfigInvalid,
    IdentityInvalid,
    Internal,
    NetworkUnavailable,
    ProviderDenied,
    RefreshTokenMissing,
    SecureStoreFailed,
    Timeout,
    TokenExchangeFailed,
    WorkspaceRestricted,
}

impl fmt::Debug for AddAccountErrorTag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AddAccountErrorTag {
    pub(crate) const ALL: [Self; 14] = [
        Self::AuthInProgress,
        Self::BrowserOpenFailed,
        Self::CallbackInvalid,
        Self::Cancelled,
        Self::ConfigInvalid,
        Self::IdentityInvalid,
        Self::Internal,
        Self::NetworkUnavailable,
        Self::ProviderDenied,
        Self::RefreshTokenMissing,
        Self::SecureStoreFailed,
        Self::Timeout,
        Self::TokenExchangeFailed,
        Self::WorkspaceRestricted,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AuthInProgress => "auth_in_progress",
            Self::BrowserOpenFailed => "browser_open_failed",
            Self::CallbackInvalid => "callback_invalid",
            Self::Cancelled => "cancelled",
            Self::ConfigInvalid => "config_invalid",
            Self::IdentityInvalid => "identity_invalid",
            Self::Internal => "internal",
            Self::NetworkUnavailable => "network_unavailable",
            Self::ProviderDenied => "provider_denied",
            Self::RefreshTokenMissing => "refresh_token_missing",
            Self::SecureStoreFailed => "secure_store_failed",
            Self::Timeout => "timeout",
            Self::TokenExchangeFailed => "token_exchange_failed",
            Self::WorkspaceRestricted => "workspace_restricted",
        }
    }

    const fn presentation(self) -> (&'static str, bool, bool) {
        match self {
            Self::AuthInProgress => (
                "An account authorization is already in progress.",
                false,
                false,
            ),
            Self::BrowserOpenFailed => (
                "Zerochi could not open the authorization page.",
                true,
                false,
            ),
            Self::CallbackInvalid => (
                "The authorization response could not be verified.",
                false,
                false,
            ),
            Self::Cancelled => ("Account authorization was cancelled.", false, false),
            Self::ConfigInvalid => (
                "Set a valid ZEROCHI_GOOGLE_CLIENT_ID and restart Zerochi.",
                false,
                true,
            ),
            Self::IdentityInvalid => (
                "Google did not return an eligible verified account.",
                false,
                true,
            ),
            Self::Internal => (
                "Zerochi could not complete account authorization.",
                false,
                true,
            ),
            Self::NetworkUnavailable => {
                ("Account authorization could not reach Google.", true, false)
            }
            Self::ProviderDenied => ("Google did not approve account authorization.", false, true),
            Self::RefreshTokenMissing => (
                "Google did not provide durable account access.",
                false,
                true,
            ),
            Self::SecureStoreFailed => (
                "Zerochi could not store the account credential securely.",
                false,
                true,
            ),
            Self::Timeout => ("Account authorization timed out.", true, false),
            Self::TokenExchangeFailed => (
                "Google could not complete account authorization. Restart authorization and try again.",
                true,
                false,
            ),
            Self::WorkspaceRestricted => (
                "Your Workspace policy does not allow this authorization.",
                false,
                true,
            ),
        }
    }
}

/// Sanitized command-compatible error. It cannot retain an adapter/provider source.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AddAccountError {
    tag: AddAccountErrorTag,
    message: &'static str,
    retryable: bool,
    action_required: bool,
}

impl AddAccountError {
    pub(crate) const fn from_tag(tag: AddAccountErrorTag) -> Self {
        let (message, retryable, action_required) = tag.presentation();
        Self {
            tag,
            message,
            retryable,
            action_required,
        }
    }

    pub(crate) const fn tag(&self) -> AddAccountErrorTag {
        self.tag
    }
}

impl fmt::Debug for AddAccountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AddAccountError")
            .field("tag", &self.tag)
            .field("message", &self.message)
            .field("retryable", &self.retryable)
            .field("action_required", &self.action_required)
            .finish()
    }
}

impl fmt::Display for AddAccountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for AddAccountError {}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_SECRETS: [&str; 10] = [
        "fixture-access-token",
        "fixture-refresh-token",
        "fixture-auth-code",
        "fixture-pkce-verifier",
        "fixture-oauth-state",
        "someone@example.test",
        "https://localhost/callback?code=fixture-auth-code",
        "Bearer fixture-access-token",
        "/Users/someone/credentials.json",
        "provider diagnostic body",
    ];

    #[test]
    fn every_contract_tag_serializes_to_the_approved_sanitized_shape() {
        let approved_tags = [
            "auth_in_progress",
            "browser_open_failed",
            "callback_invalid",
            "cancelled",
            "config_invalid",
            "identity_invalid",
            "internal",
            "network_unavailable",
            "provider_denied",
            "refresh_token_missing",
            "secure_store_failed",
            "timeout",
            "token_exchange_failed",
            "workspace_restricted",
        ];

        assert_eq!(
            AddAccountErrorTag::ALL.map(AddAccountErrorTag::as_str),
            approved_tags
        );

        for tag in AddAccountErrorTag::ALL {
            let error = AddAccountError::from_tag(tag);
            let value = serde_json::to_value(&error).expect("error should serialize");

            assert_eq!(value.as_object().map(|object| object.len()), Some(4));
            assert_eq!(value["tag"], tag.as_str());
            assert!(value["message"]
                .as_str()
                .is_some_and(|message| !message.is_empty()));
            assert!(value["retryable"].is_boolean());
            assert!(value["actionRequired"].is_boolean());
        }
    }

    #[test]
    fn token_exchange_failure_is_retryable_with_fixed_restart_copy() {
        let error = AddAccountError::from_tag(AddAccountErrorTag::TokenExchangeFailed);
        let value = serde_json::to_value(&error).expect("serialize error");

        assert_eq!(value["tag"], "token_exchange_failed");
        assert_eq!(value["retryable"], true);
        assert_eq!(value["actionRequired"], false);
        assert_eq!(
            value["message"],
            "Google could not complete account authorization. Restart authorization and try again."
        );
        for secret in FIXTURE_SECRETS {
            assert!(!format!("{error:?} {error}").contains(secret));
        }
    }

    #[test]
    fn display_debug_and_serialization_cannot_contain_secret_inputs() {
        for tag in AddAccountErrorTag::ALL {
            let error = AddAccountError::from_tag(tag);
            let rendered = format!(
                "{}\n{:?}\n{}",
                error,
                error,
                serde_json::to_string(&error).expect("error should serialize")
            );

            for secret in FIXTURE_SECRETS {
                assert!(!rendered.contains(secret), "leaked fixture for {tag:?}");
            }
        }
    }
}
