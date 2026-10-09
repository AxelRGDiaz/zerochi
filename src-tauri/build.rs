use std::{env, fs, path::PathBuf};

const CLIENT_ID_ENV: &str = "ZEROCHI_GOOGLE_CLIENT_ID";
const CLIENT_SECRET_ENV: &str = "ZEROCHI_GOOGLE_CLIENT_SECRET";
const CLIENT_ID_MASK_SEED: u8 = 0x6d;
const CLIENT_SECRET_MASK_SEED: u8 = 0xb7;

fn main() {
    // This marker is non-sensitive and may safely participate in Cargo invalidation.
    // Credential values deliberately must not use rerun-if-env-changed.
    println!("cargo:rerun-if-env-changed=ZEROCHI_FRESH_RELEASE_BUILD");
    let release = env::var_os("PROFILE").is_some_and(|profile| profile == "release");
    let (client_id, client_secret) = if release {
        assert_eq!(
            env::var("ZEROCHI_FRESH_RELEASE_BUILD").as_deref(),
            Ok("1"),
            "release builds must use npm run tauri:release"
        );
        let client_id = required(CLIENT_ID_ENV);
        let client_secret = required(CLIENT_SECRET_ENV);
        validate_client_id(&client_id);
        validate_client_secret(&client_secret);
        (
            mask(client_id.as_bytes(), CLIENT_ID_MASK_SEED),
            mask(client_secret.as_bytes(), CLIENT_SECRET_MASK_SEED),
        )
    } else {
        (Vec::new(), Vec::new())
    };

    let generated = format!(
        "const BUILD_CLIENT_ID_MASK_SEED: u8 = {CLIENT_ID_MASK_SEED};\n\
         const BUILD_CLIENT_SECRET_MASK_SEED: u8 = {CLIENT_SECRET_MASK_SEED};\n\
         const BUILD_CLIENT_ID_MASKED: &[u8] = &{client_id:?};\n\
         const BUILD_CLIENT_SECRET_MASKED: &[u8] = &{client_secret:?};\n"
    );
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR is required"))
        .join("oauth_build_config.rs");
    fs::write(output, generated).expect("OAuth build configuration could not be generated");

    // Credential changes require a clean production target. Deliberately do not emit
    // rerun-if-env-changed: Cargo persists those environment values in cache metadata.
    tauri_build::build();
}

fn required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} must be set for a release build"))
}

fn validate_client_id(value: &str) {
    let valid = (32..=255).contains(&value.len())
        && value.ends_with(".apps.googleusercontent.com")
        && value.is_ascii()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    assert!(valid, "ZEROCHI_GOOGLE_CLIENT_ID is invalid");
}

fn validate_client_secret(value: &str) {
    let valid = (8..=512).contains(&value.len())
        && value.is_ascii()
        && value.trim() == value
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    assert!(valid, "ZEROCHI_GOOGLE_CLIENT_SECRET is invalid");
}

fn mask(value: &[u8], seed: u8) -> Vec<u8> {
    value
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ seed.wrapping_add((index as u8).wrapping_mul(31)))
        .collect()
}
