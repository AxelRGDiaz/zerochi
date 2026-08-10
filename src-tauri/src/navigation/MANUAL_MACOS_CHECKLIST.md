# Native OAuth adapter verification

Use only synthetic OAuth configuration and a test Google account.

- In an unsigned debug build, construct the token bridge through `unsigned_development`; confirm authorization opens the exact Google URL in the configured default browser and the login Keychain receives the refresh token.
- Change the default browser and repeat; confirm macOS honors the new default without Zerochi selecting a browser itself.
- Cancel before browser handoff and confirm no browser window opens.
- Make `/usr/bin/open` fail in a controlled test environment and confirm the UI receives only `browser_open_failed`, with no authorization URL in output or logs.
- Lock or deny access to the login Keychain and confirm add-account fails with only `secure_store_failed` and persists no account.
- In a signed/provisioned build, construct through `signed_provisioned`; confirm Protected Data is used and access succeeds while the device is unlocked.
- Run an unsigned release build and confirm Protected Data entitlement failure does not fall back to the legacy Keychain.
- Force account persistence failure after token storage during integration testing; invoke bridge deletion and confirm the Keychain item is removed before retrying.
