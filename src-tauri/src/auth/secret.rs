use serde::{Deserialize, Deserializer};
use std::fmt;
use zeroize::Zeroize;

/// A transient OAuth string that redacts formatting and clears its allocation on drop.
pub(crate) struct SensitiveString(String);

impl SensitiveString {
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }

    pub(crate) fn expose_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'de> Deserialize<'de> for SensitiveString {
    fn deserialize<DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<Self, DeserializerType::Error>
    where
        DeserializerType: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::new)
    }
}

impl fmt::Debug for SensitiveString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SensitiveString([REDACTED])")
    }
}

impl Drop for SensitiveString {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_is_redacted() {
        let secret = SensitiveString::new("fixture-secret".to_owned());
        let rendered = format!("{secret:?}");

        assert_eq!(rendered, "SensitiveString([REDACTED])");
        assert!(!rendered.contains(secret.expose()));
    }
}
