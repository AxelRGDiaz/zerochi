# Product

## Vision

Zerochi is a tiny macOS menu-bar companion that turns newly detected mail into a kind, local Inbox Zero loop: mail arrives, Zerochi accumulates envelopes, and opening one from Zerochi feeds the pet. The pet is always adorable and never dies, shames, threatens, or creates punitive streaks.

## Core definitions

- **New mail:** a message first observed after Zerochi establishes an account baseline. It is not the historical Gmail unread total.
- **Pending:** a newly detected message that has not been opened from Zerochi.
- **Consumed:** opening a message from Zerochi records `openedFromZerochiAt` locally and reduces pending. V1 never marks read, archives, deletes, or otherwise mutates Gmail.
- **Inbox Zero:** global pending is zero. `1 → 0` celebrates; `0 → 0` does not replay the event.

## V1

macOS, menu bar/popup, Gmail personal and Workspace, Outlook personal accounts (read-only Microsoft Graph), independent multi-account OAuth, read-only incremental sync and polling, local persistence, native notifications/navigation, global and per-account badges, secure credentials, autostart/lifecycle, pixel-art sprite sheets, dark/light UI. One global pet uses the sum of active-account Gmail unread counts for its ambient face, phrase, and visible total; Zerochi pending remains the local event/feeding state.

## Out of scope

Backend, cloud database, Pub/Sub, Electron, AI, mail sending or mutation, Outlook work/school accounts, iCloud, App Store packaging work before product validation, and final character art in M0.

## Primary UX

The compact popup is pet-first and visually emerges from the menu-bar icon as a rounded comic-style bubble: no native title bar/traffic-light controls, a transparent exterior, a top pointer aligned with the tray icon, and a contained shadow. Inside remain the pet, friendly status/copy, global Gmail INBOX unread total, small account list with matching per-account INBOX unread counts, add-account, settings, and quit. New-mail notifications identify the destination account first, then subject and sender, using only stored metadata and the user's macOS preview policy; activation attempts that Gmail conversation and falls back to the account when the native contextual handoff is rejected. Account/message clicks delegate to `ExternalNavigationService`; browser `/u/N/` positions are not stable identity. Settings provides a manual all-active-account mail check whose acknowledgement means scheduled, not completed.
