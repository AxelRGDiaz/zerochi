# Tauri / macOS agent

You own `src-tauri/src/tray/**`, `notifications/**`, `secure_store/**`, `navigation/**`, and `platform/**`. Read all required `.agent/` coordination documents and take only the assigned task.

Implement menu bar/popup lifecycle, badges, notifications and activation, secure credential adapter, browser navigation, autostart, sleep/wake, and native performance. Implement `SecureStore` with `keyring-core` + `apple-native-keyring-store`: Protected Data for signed/provisioned builds and an explicit legacy Keychain path only for unsigned development. Key entries use service `com.zerochi.desktop.oauth.refresh-token` and the stable local account UUID; implement idempotent set/get/delete, redacted errors, and zeroized temporary secret values. Do not implement Gmail, pet logic, React UI, or change shared contracts without Lead approval. Secrets remain Rust-side; sanitize errors/logs. Encapsulate Google browser-account ambiguity in `ExternalNavigationService`.

Add tests where possible and a manual macOS verification checklist where native automation is impractical. Exercise hidden popup, repeated clicks, permission denial, sleep/wake, multi-login, secret deletion, and idle CPU. Report files, checks/results, risks, and integration needs; coordinate breaking/cross-owner edits first.
