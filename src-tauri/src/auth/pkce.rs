use super::{AddAccountError, AddAccountErrorTag, SensitiveString};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};
use std::fmt;
use zeroize::Zeroize;

const ENTROPY_BYTES: usize = 32;

pub(crate) trait RandomSource {
    fn fill(&self, destination: &mut [u8]) -> Result<(), AddAccountError>;
}

pub(crate) struct OsRandom;

impl RandomSource for OsRandom {
    fn fill(&self, destination: &mut [u8]) -> Result<(), AddAccountError> {
        getrandom::fill(destination)
            .map_err(|_| AddAccountError::from_tag(AddAccountErrorTag::Internal))
    }
}

pub(crate) struct Pkce {
    verifier: SensitiveString,
    challenge: SensitiveString,
    state: SensitiveString,
}

impl Pkce {
    pub(crate) fn verifier(&self) -> &str {
        self.verifier.expose()
    }

    pub(crate) fn challenge(&self) -> &str {
        self.challenge.expose()
    }

    pub(crate) fn state(&self) -> &str {
        self.state.expose()
    }

    pub(crate) fn into_parts(self) -> (SensitiveString, SensitiveString, SensitiveString) {
        (self.verifier, self.challenge, self.state)
    }
}

impl fmt::Debug for Pkce {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Pkce([REDACTED])")
    }
}

pub(crate) fn generate_pkce(random: &impl RandomSource) -> Result<Pkce, AddAccountError> {
    let mut verifier_entropy = [0_u8; ENTROPY_BYTES];
    let mut state_entropy = [0_u8; ENTROPY_BYTES];

    random.fill(&mut verifier_entropy)?;
    if let Err(error) = random.fill(&mut state_entropy) {
        verifier_entropy.zeroize();
        return Err(error);
    }

    let verifier = URL_SAFE_NO_PAD.encode(verifier_entropy);
    let state = URL_SAFE_NO_PAD.encode(state_entropy);
    verifier_entropy.zeroize();
    state_entropy.zeroize();
    let challenge = challenge_for_verifier(&verifier);

    Ok(Pkce {
        verifier: SensitiveString::new(verifier),
        challenge: SensitiveString::new(challenge),
        state: SensitiveString::new(state),
    })
}

fn challenge_for_verifier(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, sync::Mutex};

    struct SequenceRandom(Mutex<VecDeque<[u8; ENTROPY_BYTES]>>);

    impl SequenceRandom {
        fn new(values: impl IntoIterator<Item = [u8; ENTROPY_BYTES]>) -> Self {
            Self(Mutex::new(values.into_iter().collect()))
        }
    }

    impl RandomSource for SequenceRandom {
        fn fill(&self, destination: &mut [u8]) -> Result<(), AddAccountError> {
            let value = self
                .0
                .lock()
                .expect("test random lock")
                .pop_front()
                .ok_or_else(|| AddAccountError::from_tag(AddAccountErrorTag::Internal))?;
            destination.copy_from_slice(&value);
            Ok(())
        }
    }

    #[test]
    fn matches_the_rfc_7636_s256_test_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            challenge_for_verifier(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn generates_deterministic_independent_verifier_and_state() {
        let random = SequenceRandom::new([[0x11; ENTROPY_BYTES], [0x22; ENTROPY_BYTES]]);
        let pkce = generate_pkce(&random).expect("pkce generation");

        assert_eq!(
            pkce.verifier(),
            URL_SAFE_NO_PAD.encode([0x11; ENTROPY_BYTES])
        );
        assert_eq!(pkce.state(), URL_SAFE_NO_PAD.encode([0x22; ENTROPY_BYTES]));
        assert_ne!(pkce.verifier(), pkce.state());
        assert_eq!(pkce.verifier().len(), 43);
        assert_eq!(pkce.state().len(), 43);
        assert_eq!(pkce.challenge().len(), 43);
        assert!(pkce
            .verifier()
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')));
    }

    #[test]
    fn random_failure_is_sanitized() {
        let random = SequenceRandom::new([]);
        let error = generate_pkce(&random).unwrap_err();

        assert_eq!(error.tag(), AddAccountErrorTag::Internal);
        assert!(!format!("{error:?}").contains("random"));
    }

    #[test]
    fn debug_never_exposes_pkce_material() {
        let pkce = generate_pkce(&SequenceRandom::new([
            [0x33; ENTROPY_BYTES],
            [0x44; ENTROPY_BYTES],
        ]))
        .expect("pkce generation");
        let rendered = format!("{pkce:?}");

        assert_eq!(rendered, "Pkce([REDACTED])");
        assert!(!rendered.contains(pkce.verifier()));
        assert!(!rendered.contains(pkce.state()));
    }
}
