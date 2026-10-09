use super::{AddAccountError, AddAccountErrorTag, ClientIdPort, ClientSecretPort};
#[cfg(debug_assertions)]
use std::fs;
use std::{ffi::OsString, fmt, path::Path};
use zeroize::{Zeroize, Zeroizing};

include!(concat!(env!("OUT_DIR"), "/oauth_build_config.rs"));

const CLIENT_ID_ENV: &str = "ZEROCHI_GOOGLE_CLIENT_ID";
const CLIENT_SECRET_ENV: &str = "ZEROCHI_GOOGLE_CLIENT_SECRET";
const CLIENT_ID_SUFFIX: &str = ".apps.googleusercontent.com";
const MIN_CLIENT_ID_BYTES: usize = 32;
const MAX_CLIENT_ID_BYTES: usize = 255;
const MAX_DOTENV_BYTES: u64 = 64 * 1024;

pub(crate) trait Environment {
    fn var_os(&self, name: &str) -> Option<OsString>;
}

pub(crate) struct ProcessEnvironment;

impl Environment for ProcessEnvironment {
    fn var_os(&self, name: &str) -> Option<OsString> {
        std::env::var_os(name)
    }
}

pub(crate) struct ProcessClientId;

impl ProcessClientId {
    fn load_from(
        environment: &impl Environment,
        current_dir: &Path,
    ) -> Result<ClientId, AddAccountError> {
        #[cfg(debug_assertions)]
        {
            ClientId::parse(load_debug_value(CLIENT_ID_ENV, environment, current_dir)?)
        }

        #[cfg(not(debug_assertions))]
        {
            let _ = (environment, current_dir);
            Err(config_invalid())
        }
    }
}

impl ClientIdPort for ProcessClientId {
    fn load(&self) -> Result<ClientId, AddAccountError> {
        #[cfg(not(debug_assertions))]
        {
            let value = decode_masked(BUILD_CLIENT_ID_MASKED, BUILD_CLIENT_ID_MASK_SEED)?;
            ClientId::parse(value)
        }
        #[cfg(debug_assertions)]
        {
            let current_dir = std::env::current_dir().map_err(|_| config_invalid())?;
            Self::load_from(&ProcessEnvironment, &current_dir)
        }
    }
}

pub(crate) struct ProcessClientSecret;

impl ProcessClientSecret {
    #[cfg(test)]
    fn load_from(
        environment: &impl Environment,
        current_dir: &Path,
    ) -> Result<ClientSecret, AddAccountError> {
        #[cfg(debug_assertions)]
        {
            ClientSecret::parse(load_debug_value(
                CLIENT_SECRET_ENV,
                environment,
                current_dir,
            )?)
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = (environment, current_dir);
            Err(config_invalid())
        }
    }
}

impl ClientSecretPort for ProcessClientSecret {
    fn load(&self) -> Result<ClientSecret, AddAccountError> {
        #[cfg(not(debug_assertions))]
        {
            let value = decode_masked(BUILD_CLIENT_SECRET_MASKED, BUILD_CLIENT_SECRET_MASK_SEED)?;
            ClientSecret::parse(value)
        }
        #[cfg(debug_assertions)]
        {
            let current_dir = std::env::current_dir().map_err(|_| config_invalid())?;
            ClientSecret::parse(load_debug_value(
                CLIENT_SECRET_ENV,
                &ProcessEnvironment,
                &current_dir,
            )?)
        }
    }
}

#[cfg(debug_assertions)]
fn load_debug_value(
    name: &str,
    environment: &impl Environment,
    current_dir: &Path,
) -> Result<String, AddAccountError> {
    if let Some(raw) = environment.var_os(name) {
        return raw.into_string().map_err(|_| config_invalid());
    }
    let path = debug_dotenv_path(current_dir)?;
    load_dotenv_value(&path, name)
}

#[cfg(debug_assertions)]
fn debug_dotenv_path(current_dir: &Path) -> Result<std::path::PathBuf, AddAccountError> {
    let local = current_dir.join(".env");
    match fs::symlink_metadata(&local) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(local),
        Ok(_) => Err(config_invalid()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if current_dir
                .file_name()
                .is_some_and(|name| name == "src-tauri")
            {
                let parent = current_dir
                    .parent()
                    .ok_or_else(config_invalid)?
                    .join(".env");
                let metadata = fs::symlink_metadata(&parent).map_err(|_| config_invalid())?;
                if metadata.is_file() && !metadata.file_type().is_symlink() {
                    return Ok(parent);
                }
            }
            Err(config_invalid())
        }
        Err(_) => Err(config_invalid()),
    }
}

#[cfg(debug_assertions)]
fn load_dotenv_value(path: &Path, name: &str) -> Result<String, AddAccountError> {
    let metadata = fs::metadata(path).map_err(|_| config_invalid())?;
    if !metadata.is_file() || metadata.len() > MAX_DOTENV_BYTES {
        return Err(config_invalid());
    }
    let mut found = None;
    for entry in dotenvy::from_path_iter(path).map_err(|_| config_invalid())? {
        let (key, value) = entry.map_err(|_| config_invalid())?;
        if key == name {
            if found.is_some() {
                return Err(config_invalid());
            }
            found = Some(value);
        }
    }
    found.ok_or_else(config_invalid)
}

fn decode_masked(masked: &[u8], seed: u8) -> Result<String, AddAccountError> {
    if masked.is_empty() {
        return Err(config_invalid());
    }
    let decoded = Zeroizing::new(
        masked
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ seed.wrapping_add((index as u8).wrapping_mul(31)))
            .collect::<Vec<_>>(),
    );
    std::str::from_utf8(decoded.as_slice())
        .map(str::to_owned)
        .map_err(|_| config_invalid())
}

pub(crate) struct ClientSecret(String);

impl ClientSecret {
    #[cfg(test)]
    pub(crate) fn load_from(environment: &impl Environment) -> Result<Self, AddAccountError> {
        environment
            .var_os(CLIENT_SECRET_ENV)
            .ok_or_else(config_invalid)
            .and_then(Self::load_from_raw)
    }

    fn load_from_raw(raw: OsString) -> Result<Self, AddAccountError> {
        let value = raw.into_string().map_err(|_| config_invalid())?;
        Self::parse(value)
    }

    fn parse(value: String) -> Result<Self, AddAccountError> {
        let valid = (8..=512).contains(&value.len())
            && value.is_ascii()
            && value.trim() == value
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
        if valid {
            Ok(Self(value))
        } else {
            Err(config_invalid())
        }
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ClientSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ClientSecret([REDACTED])")
    }
}

impl Zeroize for ClientSecret {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl Drop for ClientSecret {
    fn drop(&mut self) {
        self.zeroize();
    }
}

pub(crate) struct ClientId(String);

impl ClientId {
    pub(crate) fn load() -> Result<Self, AddAccountError> {
        ProcessClientId.load()
    }

    pub(crate) fn load_from(environment: &impl Environment) -> Result<Self, AddAccountError> {
        let raw = environment
            .var_os(CLIENT_ID_ENV)
            .ok_or_else(config_invalid)?;
        Self::load_from_raw(raw)
    }

    fn load_from_raw(raw: OsString) -> Result<Self, AddAccountError> {
        let value = raw.into_string().map_err(|_| config_invalid())?;
        Self::parse(value)
    }

    fn parse(value: String) -> Result<Self, AddAccountError> {
        let valid_length = (MIN_CLIENT_ID_BYTES..=MAX_CLIENT_ID_BYTES).contains(&value.len());
        let prefix = value.strip_suffix(CLIENT_ID_SUFFIX);
        let valid_prefix = prefix.is_some_and(|prefix| {
            !prefix.is_empty()
                && prefix
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        });

        if value.is_ascii()
            && value.trim() == value
            && valid_length
            && valid_prefix
            && !value.bytes().any(|byte| byte.is_ascii_control())
        {
            Ok(Self(value))
        } else {
            Err(config_invalid())
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ClientId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ClientId([REDACTED])")
    }
}

fn config_invalid() -> AddAccountError {
    AddAccountError::from_tag(AddAccountErrorTag::ConfigInvalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        ffi::OsStr,
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    const VALID_CLIENT_ID: &str =
        "123456789012-abcdefghijklmnopqrstuvwxyz012345.apps.googleusercontent.com";

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn decodes_the_build_mask_formula_into_an_owned_validated_value() {
        let masked = [
            36, 8, 248, 235, 203, 175, 153, 54, 87, 56, 11, 252, 155, 163, 149, 127, 71, 52,
        ];

        let decoded = decode_masked(&masked, 0x42).expect("decode synthetic build value");

        assert_eq!(decoded, "fixture-mask-value");
        let secret = ClientSecret::parse(decoded).expect("validate decoded secret");
        assert_eq!(format!("{secret:?}"), "ClientSecret([REDACTED])");
        assert!(!format!("{secret:?}").contains(secret.expose()));
    }

    #[test]
    fn empty_or_non_utf8_build_arrays_fail_closed_without_byte_diagnostics() {
        for masked in [&[][..], &[0xff][..]] {
            let error = decode_masked(masked, 0).unwrap_err();
            assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
            assert!(!format!("{error:?} {error}").contains("255"));
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_generated_build_arrays_are_empty_and_ignored() {
        assert!(BUILD_CLIENT_ID_MASKED.is_empty());
        assert!(BUILD_CLIENT_SECRET_MASKED.is_empty());
        assert_ne!(BUILD_CLIENT_ID_MASK_SEED, BUILD_CLIENT_SECRET_MASK_SEED);
    }

    #[test]
    fn auth_config_source_contains_no_value_bearing_credential_macro() {
        let source = include_str!("config.rs");
        let optional_macro = ["option_", "env!(\"ZEROCHI_GOOGLE_CLIENT"].concat();
        let required_macro = ["env", "!(\"ZEROCHI_GOOGLE_CLIENT"].concat();
        assert!(!source.contains(&optional_macro));
        assert!(!source.contains(&required_macro));
        let iterator_loader = ["dotenvy::", "from_path_iter"].concat();
        assert_eq!(source.matches(&iterator_loader).count(), 1);
        assert!(source.contains("#[cfg(debug_assertions)]\nfn load_dotenv_value"));
        let mutating_loader = ["dotenvy::", "dotenv"].concat();
        assert!(!source.contains(&mutating_loader));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn release_debug_loaders_ignore_process_and_dotenv_sources() {
        let directory = TestDirectory::new();
        directory.write_dotenv(&format!(
            "ZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\nZEROCHI_GOOGLE_CLIENT_SECRET=fixture-dotenv-secret-value\n"
        ));
        let environment =
            FakeEnvironment::with_pair(VALID_CLIENT_ID, "fixture-process-secret-value");

        assert!(ProcessClientId::load_from(&environment, directory.path()).is_err());
        assert!(ProcessClientSecret::load_from(&environment, directory.path()).is_err());
        assert!(environment.reads.into_inner().is_empty());
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "zerochi-auth-config-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create isolated test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write_dotenv(&self, contents: &str) {
            fs::write(self.0.join(".env"), contents).expect("write isolated dotenv fixture");
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove isolated test directory");
        }
    }

    #[derive(Default)]
    struct FakeEnvironment {
        values: HashMap<String, OsString>,
        reads: std::cell::RefCell<Vec<String>>,
    }

    impl FakeEnvironment {
        fn with_client_id(value: impl AsRef<OsStr>) -> Self {
            Self {
                values: HashMap::from([(CLIENT_ID_ENV.to_owned(), value.as_ref().to_owned())]),
                reads: Default::default(),
            }
        }

        fn with_client_secret(value: impl AsRef<OsStr>) -> Self {
            Self {
                values: HashMap::from([(CLIENT_SECRET_ENV.to_owned(), value.as_ref().to_owned())]),
                reads: Default::default(),
            }
        }

        fn with_pair(client_id: impl AsRef<OsStr>, client_secret: impl AsRef<OsStr>) -> Self {
            Self {
                values: HashMap::from([
                    (CLIENT_ID_ENV.to_owned(), client_id.as_ref().to_owned()),
                    (
                        CLIENT_SECRET_ENV.to_owned(),
                        client_secret.as_ref().to_owned(),
                    ),
                ]),
                reads: Default::default(),
            }
        }
    }

    impl Environment for FakeEnvironment {
        fn var_os(&self, name: &str) -> Option<OsString> {
            self.reads.borrow_mut().push(name.to_owned());
            self.values.get(name).cloned()
        }
    }

    #[test]
    fn loads_a_representative_desktop_client_id_from_the_rust_environment() {
        let environment = FakeEnvironment::with_client_id(VALID_CLIENT_ID);

        let client_id = ClientId::load_from(&environment).expect("valid client id");

        assert_eq!(client_id.as_str(), VALID_CLIENT_ID);
        assert_eq!(environment.reads.into_inner(), [CLIENT_ID_ENV]);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn process_environment_pair_has_priority_over_local_dotenv() {
        let directory = TestDirectory::new();
        directory.write_dotenv(
            "ZEROCHI_GOOGLE_CLIENT_ID=fixture-invalid\nZEROCHI_GOOGLE_CLIENT_SECRET=short\n",
        );
        let environment =
            FakeEnvironment::with_pair(VALID_CLIENT_ID, "fixture-process-secret-value");

        let client_id = ProcessClientId::load_from(&environment, directory.path())
            .expect("process client id wins");
        let client_secret = ProcessClientSecret::load_from(&environment, directory.path())
            .expect("process client secret wins");

        assert_eq!(client_id.as_str(), VALID_CLIENT_ID);
        assert_eq!(client_secret.expose(), "fixture-process-secret-value");
        assert_eq!(
            environment.reads.into_inner(),
            [CLIENT_ID_ENV, CLIENT_SECRET_ENV]
        );
    }

    #[test]
    fn masked_build_client_id_decodes_into_the_validated_owned_type() {
        let masked = VALID_CLIENT_ID
            .bytes()
            .enumerate()
            .map(|(index, byte)| byte ^ 0x31_u8.wrapping_add((index as u8).wrapping_mul(31)))
            .collect::<Vec<_>>();
        let client_id = ClientId::parse(decode_masked(&masked, 0x31).expect("decode client id"))
            .expect("validate client id");
        assert_eq!(client_id.as_str(), VALID_CLIENT_ID);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn client_secret_allows_runtime_only_in_debug_configuration() {
        let directory = TestDirectory::new();
        let runtime = FakeEnvironment::with_client_secret("fixture-runtime-secret-value");
        let secret = ProcessClientSecret::load_from(&runtime, directory.path())
            .expect("debug runtime secret");
        assert_eq!(secret.expose(), "fixture-runtime-secret-value");
        assert_eq!(runtime.reads.into_inner(), [CLIENT_SECRET_ENV]);
    }

    #[test]
    fn client_secret_is_redacted_and_zeroizable() {
        let environment = FakeEnvironment::with_client_secret("fixture-client-secret-value");
        let mut secret = ClientSecret::load_from(&environment).expect("secret");
        assert_eq!(format!("{secret:?}"), "ClientSecret([REDACTED])");
        assert!(!format!("{secret:?}").contains(secret.expose()));
        secret.zeroize();
        assert!(secret.expose().is_empty());
    }

    #[test]
    fn malformed_or_missing_client_secret_is_config_invalid_without_echo() {
        for raw in [None, Some("short"), Some("fixture secret with spaces")] {
            let directory = TestDirectory::new();
            let environment = raw
                .map(FakeEnvironment::with_client_secret)
                .unwrap_or_default();
            let error = ProcessClientSecret::load_from(&environment, directory.path()).unwrap_err();
            assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
            if let Some(raw) = raw {
                assert!(!format!("{error:?} {error}").contains(raw));
            }
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn invalid_process_environment_does_not_fall_back_to_local_dotenv() {
        let directory = TestDirectory::new();
        directory.write_dotenv(&format!(
            "ZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\nZEROCHI_GOOGLE_CLIENT_SECRET=fixture-dotenv-secret-value\n"
        ));
        let environment = FakeEnvironment::with_pair("fixture-invalid", "short");

        let error = ProcessClientId::load_from(&environment, directory.path()).unwrap_err();
        let secret_error =
            ProcessClientSecret::load_from(&environment, directory.path()).unwrap_err();

        assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
        assert_eq!(secret_error.tag(), AddAccountErrorTag::ConfigInvalid);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn loads_complete_pair_from_current_directory_dotenv_when_environment_is_absent() {
        let directory = TestDirectory::new();
        directory.write_dotenv(&format!(
            "ZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\nZEROCHI_GOOGLE_CLIENT_SECRET=fixture-dotenv-secret-value\n"
        ));

        let client_id = ProcessClientId::load_from(&FakeEnvironment::default(), directory.path())
            .expect("local dotenv client id");
        let client_secret =
            ProcessClientSecret::load_from(&FakeEnvironment::default(), directory.path())
                .expect("local dotenv client secret");

        assert_eq!(client_id.as_str(), VALID_CLIENT_ID);
        assert_eq!(client_secret.expose(), "fixture-dotenv-secret-value");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_loads_complete_pair_from_parent_only_when_current_directory_is_src_tauri() {
        let directory = TestDirectory::new();
        directory.write_dotenv(&format!(
            "ZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\nZEROCHI_GOOGLE_CLIENT_SECRET=fixture-parent-secret-value\n"
        ));
        let src_tauri = directory.path().join("src-tauri");
        fs::create_dir(&src_tauri).expect("create src-tauri fixture directory");

        let client_id = ProcessClientId::load_from(&FakeEnvironment::default(), &src_tauri)
            .expect("parent dotenv client id");
        let client_secret = ProcessClientSecret::load_from(&FakeEnvironment::default(), &src_tauri)
            .expect("parent dotenv client secret");

        assert_eq!(client_id.as_str(), VALID_CLIENT_ID);
        assert_eq!(client_secret.expose(), "fixture-parent-secret-value");
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_does_not_load_parent_dotenv_from_an_arbitrary_current_directory() {
        let directory = TestDirectory::new();
        directory.write_dotenv(&format!("ZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\n"));
        let arbitrary = directory.path().join("native");
        fs::create_dir(&arbitrary).expect("create arbitrary fixture directory");

        let error =
            ProcessClientId::load_from(&FakeEnvironment::default(), &arbitrary).unwrap_err();

        assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn invalid_local_dotenv_variants_do_not_fall_back_to_parent_dotenv() {
        for fixture in ["malformed", "not-a-file", "oversized", "non-utf8"] {
            let directory = TestDirectory::new();
            directory.write_dotenv(&format!("ZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\n"));
            let src_tauri = directory.path().join("src-tauri");
            fs::create_dir(&src_tauri).expect("create src-tauri fixture directory");
            let local_dotenv = src_tauri.join(".env");
            match fixture {
                "malformed" => {
                    fs::write(&local_dotenv, "ZEROCHI_GOOGLE_CLIENT_ID=fixture-invalid\n")
                        .expect("write malformed local dotenv fixture")
                }
                "not-a-file" => {
                    fs::create_dir(&local_dotenv).expect("create non-file local dotenv fixture")
                }
                "oversized" => fs::write(&local_dotenv, vec![b'a'; MAX_DOTENV_BYTES as usize + 1])
                    .expect("write oversized local dotenv fixture"),
                "non-utf8" => fs::write(&local_dotenv, [0xff, 0xfe])
                    .expect("write non-UTF-8 local dotenv fixture"),
                _ => unreachable!(),
            }

            let error =
                ProcessClientId::load_from(&FakeEnvironment::default(), &src_tauri).unwrap_err();

            assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid, "{fixture}");
        }
    }

    #[test]
    fn absent_environment_and_dotenv_return_config_invalid() {
        let directory = TestDirectory::new();

        let error =
            ProcessClientId::load_from(&FakeEnvironment::default(), directory.path()).unwrap_err();

        assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
    }

    #[test]
    fn missing_value_is_rejected() {
        let error = ClientId::load_from(&FakeEnvironment::default()).unwrap_err();
        assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
    }

    #[test]
    fn malformed_values_are_rejected_without_echoing_them() {
        let malformed = [
            "",
            "x.apps.googleusercontent.com",
            " 123456789012-abcdefghijklmnopqrstuvwxyz.apps.googleusercontent.com",
            "123456789012-abcdefghijklmnopqrstuvwxyz.apps.googleusercontent.com ",
            "123456789012-abcdefghijklmnopqrs\ntuvwxyz.apps.googleusercontent.com",
            "123456789012-abcdefghijklmnopqrs\ttuvwxyz.apps.googleusercontent.com",
            "https://123456789012-abcdefghijklmnopqrstuvwxyz.apps.googleusercontent.com",
            "{\"client_id\":\"123456789012-abcdefghijklmnopqrstuvwxyz.apps.googleusercontent.com\"}",
            "\"123456789012-abcdefghijklmnopqrstuvwxyz.apps.googleusercontent.com\"",
            "123456789012-abcdefghijklmnopqrstuvwxyz.example.com",
            "123456789012-abcdefghijklmnop/qrstuvwxyz.apps.googleusercontent.com",
            "123456789012-abcdefghijklmnop?qrstuvwxyz.apps.googleusercontent.com",
            "123456789012-abcdefghijklmnop#qrstuvwxyz.apps.googleusercontent.com",
        ];

        for value in malformed {
            let error = ClientId::load_from(&FakeEnvironment::with_client_id(value)).unwrap_err();
            let rendered = format!("{error:?} {error}");
            assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
            if !value.is_empty() {
                assert!(!rendered.contains(value));
            }
        }

        let overlong = format!("{}.apps.googleusercontent.com", "a".repeat(256));
        let error = ClientId::load_from(&FakeEnvironment::with_client_id(&overlong)).unwrap_err();
        assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
        assert!(!format!("{error:?} {error}").contains(&overlong));
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_value_is_rejected_without_panicking() {
        use std::os::unix::ffi::OsStringExt;

        let environment = FakeEnvironment::with_client_id(OsString::from_vec(vec![0xff, 0xfe]));
        let error = ClientId::load_from(&environment).unwrap_err();

        assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
    }

    #[test]
    fn does_not_fall_back_to_vite_or_client_secret_variables() {
        let environment = FakeEnvironment {
            values: HashMap::from([
                (
                    "VITE_ZEROCHI_GOOGLE_CLIENT_ID".to_owned(),
                    VALID_CLIENT_ID.into(),
                ),
                (
                    "ZEROCHI_GOOGLE_CLIENT_SECRET".to_owned(),
                    "fixture-secret".into(),
                ),
            ]),
            reads: Default::default(),
        };

        assert!(ClientId::load_from(&environment).is_err());
        assert_eq!(environment.reads.into_inner(), [CLIENT_ID_ENV]);
    }

    #[test]
    fn debug_does_not_expose_the_client_id() {
        let client_id = ClientId::parse(VALID_CLIENT_ID.to_owned()).expect("valid client id");
        let rendered = format!("{client_id:?}");

        assert_eq!(rendered, "ClientId([REDACTED])");
        assert!(!rendered.contains(VALID_CLIENT_ID));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn dotenvy_supports_quoted_export_syntax_for_the_complete_pair() {
        for contents in [
            format!(
                "ZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\nZEROCHI_GOOGLE_CLIENT_SECRET=fixture-plain-secret-value\n"
            ),
            format!(
                "export ZEROCHI_GOOGLE_CLIENT_ID=\"{VALID_CLIENT_ID}\"\nexport ZEROCHI_GOOGLE_CLIENT_SECRET='fixture-quoted-secret-value'\n"
            ),
        ] {
            let directory = TestDirectory::new();
            directory.write_dotenv(&contents);
            let client_id = ProcessClientId::load_from(&FakeEnvironment::default(), directory.path())
                .expect("dotenv client id");
            let client_secret =
                ProcessClientSecret::load_from(&FakeEnvironment::default(), directory.path())
                    .expect("dotenv client secret");
            assert_eq!(client_id.as_str(), VALID_CLIENT_ID);
            assert!(client_secret.expose().starts_with("fixture-"));
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn dotenvy_missing_duplicate_and_malformed_values_fail_closed_without_leakage() {
        for (contents, variable) in [
            (
                format!("ZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\n"),
                CLIENT_SECRET_ENV,
            ),
            (
                "ZEROCHI_GOOGLE_CLIENT_SECRET=fixture-secret-value\n".to_owned(),
                CLIENT_ID_ENV,
            ),
            (
                format!(
                    "ZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\nZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\nZEROCHI_GOOGLE_CLIENT_SECRET=fixture-secret-value\n"
                ),
                CLIENT_ID_ENV,
            ),
            (
                format!(
                    "ZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\nZEROCHI_GOOGLE_CLIENT_SECRET=fixture-secret-value\nZEROCHI_GOOGLE_CLIENT_SECRET=fixture-other-secret-value\n"
                ),
                CLIENT_SECRET_ENV,
            ),
            (
                format!(
                    "ZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\nZEROCHI_GOOGLE_CLIENT_SECRET='fixture-unterminated\n"
                ),
                CLIENT_SECRET_ENV,
            ),
            (
                "ZEROCHI_GOOGLE_CLIENT_ID=fixture-invalid\nZEROCHI_GOOGLE_CLIENT_SECRET=short\n"
                    .to_owned(),
                CLIENT_ID_ENV,
            ),
        ] {
            let directory = TestDirectory::new();
            directory.write_dotenv(&contents);
            let error = if variable == CLIENT_ID_ENV {
                ProcessClientId::load_from(&FakeEnvironment::default(), directory.path())
                    .unwrap_err()
            } else {
                ProcessClientSecret::load_from(&FakeEnvironment::default(), directory.path())
                    .unwrap_err()
            };
            assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
            let rendered = format!("{error:?} {error}");
            assert!(!rendered.contains("fixture-"));
            assert!(!rendered.contains(VALID_CLIENT_ID));
        }
    }
}
