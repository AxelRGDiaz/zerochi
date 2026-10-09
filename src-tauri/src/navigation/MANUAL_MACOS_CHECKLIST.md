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

## Gmail account navigation (M10)

Use only synthetic/local test state and test Google accounts. Never capture the account URL, email, browser output, or account id in logs or screenshots attached to bug reports.

- Set Safari as the default browser, click an account whose matching Google session is already authenticated, and confirm Gmail opens that account's inbox.
- Set Chrome as the default browser and repeat; confirm Zerochi follows the macOS default instead of selecting a browser itself.
- With no matching Google session, click the account and confirm Google presents its normal account chooser or login flow. A successful native handoff is sufficient even if the user cancels that flow.
- With multiple personal and Workspace sessions authenticated simultaneously, click each Zerochi account and confirm Gmail uses the `authuser` identity hint rather than a positional browser-session index.
- Reorder, add, and remove Google sessions in the browser, then repeat. Confirm Zerochi never depends on `/u/0`, `/u/1`, or another `/u/N/` path.
- Make `/usr/bin/open` fail in a controlled test environment and confirm the UI receives only the fixed sanitized navigation failure, with no email, URL, account id, filesystem path, or browser output in application logs.

## Gmail message navigation

- With the matching Google session authenticated, activate a notification and confirm Gmail opens
  the exact conversation for its stored thread, while the URL path remains `/mail/u/` (never
  `/mail/u/N/`).
- Repeat with multiple Google sessions and a non-ASCII/reserved-character test account identity;
  confirm the identity hint selects the intended account and is percent-encoded.
- Force the first `/usr/bin/open` invocation to fail and the second to succeed. Confirm the first
  target is the conversation and the only fallback is the matching account inbox.
- Force both invocations to fail. Confirm the surfaced error stays fixed and contains no account
  email, thread/message identifier, URL, browser output, or executable path.
- Test an empty, over-256-byte, slash-containing, whitespace-containing, control-character, and
  non-ASCII thread identifier. Confirm each is rejected before any browser handoff.
- Repeat with Safari, Chrome, and Firefox as default browser. Existing-tab reuse is browser-owned
  and best-effort: Zerochi must not enumerate tabs, request Automation/Accessibility permission,
  or script a browser.
