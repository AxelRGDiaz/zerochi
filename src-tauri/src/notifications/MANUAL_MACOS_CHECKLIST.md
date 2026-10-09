# Contextual notification manual acceptance (macOS)

Use only disposable test accounts and messages. Never capture real tokens, provider payloads, or
private mail bodies in evidence.

## Presentation and privacy

- [ ] With notification permission granted, receive one newly committed incremental message and
      confirm Notification Center shows the destination account email as title, the subject as
      subtitle, and the sender as body.
- [ ] Repeat with empty Subject and From metadata; confirm `Sin asunto` and
      `Remitente desconocido`.
- [ ] Repeat with long non-ASCII subject/sender values; confirm the notification remains valid and
      no replacement character or split Unicode character appears.
- [ ] Check banner, Notification Center, and lock-screen preview behavior with macOS previews set
      to Always, When Unlocked, and Never. Confirm Zerochi follows the system policy and adds no
      message body or snippet.
- [ ] Inspect Console while delivering and activating a notification. Confirm no account email,
      subject, sender, URL, provider message id, local account UUID, or native diagnostic is logged.
- [ ] Confirm the custom `notification-chime` plays once for an accepted notification and is not
      replayed by activation.

## Activation and account lifecycle

- [ ] Click a contextual notification for an existing account. Confirm the matching Gmail
      conversation opens, using the account identity hint and no `/u/N/` positional identity.
- [ ] Repeat after closing that conversation but leaving a Gmail tab open. Confirm macOS/default
      browser behavior is accepted (reuse or a new tab); Zerochi neither enumerates nor controls
      browser tabs.
- [ ] Force only the contextual deep-link handoff to fail. Confirm Zerochi makes one sanitized
      fallback handoff to the matching Gmail account inbox and does not retry the deep link.
- [ ] Deliver notifications for two accounts, then activate each one. Confirm each activation opens
      its matching Gmail account rather than the most recently notified account.
- [ ] Remove an account after delivery but before activating its notification. Confirm activation is
      silently ignored: no browser handoff, state mutation, notification replay, or user-visible raw
      error.
- [ ] Quit/relaunch Zerochi and activate an old Notification Center entry. Confirm the stale
      process-local correlation is ignored safely and does not guess an account.
- [ ] Deliver two notifications whose native opaque identifier is intentionally duplicated in a
      test build but whose internal message targets differ. Confirm the second delivery is
      suppressed and cannot retarget activation away from the first message.
- [ ] Force synchronous native submission rejection, then retry the same message. Confirm the retry
      can be delivered and its activation opens the intended conversation (no stale correlation).
- [ ] With the requested Google session absent, confirm the normal Gmail chooser/login behavior;
      Zerochi must not transfer OAuth tokens to the browser.

## Delivery invariants

- [ ] Confirm baseline, history recovery, unread-only changes, startup hydration, failed commit,
      permission denial, and duplicate/overlapping polls produce no duplicate contextual delivery.
- [ ] Deny notification permission and confirm sync, unread, pending, tray, and later checks remain
      healthy; no sensitive native error appears in the UI or logs.
