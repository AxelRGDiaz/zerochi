use super::{AddAccountError, AddAccountErrorTag, ClientIdPort};
use std::{ffi::OsString, fmt, fs};

const CLIENT_ID_ENV: &str = "ZEROCHI_GOOGLE_CLIENT_ID";
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

impl ClientIdPort for ProcessClientId {
    fn load(&self) -> Result<ClientId, AddAccountError> {
        if ProcessEnvironment.var_os(CLIENT_ID_ENV).is_some() {
            return ClientId::load_from(&ProcessEnvironment);
        }

        let path = std::env::current_dir()
            .map_err(|_| config_invalid())?
            .join(".env");
        let metadata = fs::metadata(&path).map_err(|_| config_invalid())?;
        if !metadata.is_file() || metadata.len() > MAX_DOTENV_BYTES {
            return Err(config_invalid());
        }
        let contents = fs::read_to_string(path).map_err(|_| config_invalid())?;
        ClientId::load_from_dotenv(&contents)
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

    fn load_from_dotenv(contents: &str) -> Result<Self, AddAccountError> {
        let mut found = None;
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let Some((name, raw_value)) = line.split_once('=') else {
                continue;
            };
            if name.trim() != CLIENT_ID_ENV {
                continue;
            }
            if found.is_some() {
                return Err(config_invalid());
            }
            let raw_value = raw_value.trim();
            let value = match (raw_value.as_bytes().first(), raw_value.as_bytes().last()) {
                (Some(b'\''), Some(b'\'')) | (Some(b'"'), Some(b'"')) if raw_value.len() >= 2 => {
                    &raw_value[1..raw_value.len() - 1]
                }
                _ => raw_value,
            };
            found = Some(value.to_owned());
        }

        Self::parse(found.ok_or_else(config_invalid)?)
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
    use std::{collections::HashMap, ffi::OsStr};

    const VALID_CLIENT_ID: &str =
        "123456789012-abcdefghijklmnopqrstuvwxyz012345.apps.googleusercontent.com";

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

    #[test]
    fn loads_only_the_client_id_from_local_dotenv_syntax() {
        for contents in [
            format!("ZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\n"),
            format!("export ZEROCHI_GOOGLE_CLIENT_ID=\"{VALID_CLIENT_ID}\"\n"),
            format!(
                "VITE_ZEROCHI_GOOGLE_CLIENT_ID=ignored\nZEROCHI_GOOGLE_CLIENT_SECRET=ignored\nZEROCHI_GOOGLE_CLIENT_ID='{VALID_CLIENT_ID}'\n"
            ),
        ] {
            let client_id = ClientId::load_from_dotenv(&contents).expect("dotenv client id");
            assert_eq!(client_id.as_str(), VALID_CLIENT_ID);
        }
    }

    #[test]
    fn dotenv_missing_duplicate_or_malformed_client_id_fails_without_echoing_values() {
        for contents in [
            "VITE_ZEROCHI_GOOGLE_CLIENT_ID=fixture-only".to_owned(),
            format!(
                "ZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}\nZEROCHI_GOOGLE_CLIENT_ID={VALID_CLIENT_ID}"
            ),
            "ZEROCHI_GOOGLE_CLIENT_ID=fixture-invalid".to_owned(),
        ] {
            let error = ClientId::load_from_dotenv(&contents).unwrap_err();
            assert_eq!(error.tag(), AddAccountErrorTag::ConfigInvalid);
            assert!(!format!("{error:?} {error}").contains("fixture-invalid"));
            assert!(!format!("{error:?} {error}").contains(VALID_CLIENT_ID));
        }
    }
}
