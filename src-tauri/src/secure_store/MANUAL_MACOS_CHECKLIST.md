# M4/M13 macOS Keychain verification

Use synthetic credentials only. Never inspect, print, or copy a real refresh token.

## Unsigned development build

- Run a debug build and initialize `SecureStore` with `UnsignedDevelopment`.
- Add a synthetic account and confirm Keychain Access contains one generic-password item with service `com.zerochi.desktop.oauth.refresh-token` and the stable account UUID as its account name.
- Update the same account and confirm no duplicate item is created.
- Remove the account twice and confirm both deletions succeed and the item is absent.
- Lock the login keychain, then confirm reads and writes fail with only the sanitized locked-or-denied error.

## Signed/provisioned build

- Sign the app with a provisioning profile that permits Protected Data access and initialize `SecureStore` with `SignedProvisioned`.
- Repeat create, update, read, and delete checks; confirm the item is scoped to the app's protected access group and is not iCloud-synchronized.
- Deny Keychain access and confirm the operation fails closed without identifiers, paths, provider errors, or secret material in UI errors or logs.
- Run an unsigned release build with `SignedProvisioned` and confirm missing entitlements fail closed; there must be no fallback to the legacy login Keychain.
- Quit Zerochi after a read and use a memory/debugging workflow approved for synthetic credentials to confirm no long-lived plaintext token buffer remains.
