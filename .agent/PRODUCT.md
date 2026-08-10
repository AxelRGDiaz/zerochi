# Product

## Vision

Zerochi is a tiny macOS menu-bar companion that turns newly detected mail into a kind, local Inbox Zero loop: mail arrives, Zerochi accumulates envelopes, and opening one from Zerochi feeds the pet. The pet is always adorable and never dies, shames, threatens, or creates punitive streaks.

## Core definitions

- **New mail:** a message first observed after Zerochi establishes an account baseline. It is not the historical Gmail unread total.
- **Pending:** a newly detected message that has not been opened from Zerochi.
- **Consumed:** opening a message from Zerochi records `openedFromZerochiAt` locally and reduces pending. V1 never marks read, archives, deletes, or otherwise mutates Gmail.
- **Inbox Zero:** global pending is zero. `1 → 0` celebrates; `0 → 0` does not replay the event.

## V1

macOS, menu bar/popup, Gmail personal and Workspace, independent multi-account OAuth, read-only incremental sync and polling, local persistence, native notifications/navigation, global and per-account badges, secure credentials, autostart/lifecycle, pixel-art sprite sheets, dark/light UI. One global pet derives from the sum of active account pending counts.

## Out of scope

Backend, cloud database, Pub/Sub, Electron, AI, mail sending or mutation, Outlook/iCloud, App Store packaging work before product validation, and final character art in M0.

## Primary UX

The compact popup is pet-first: pet, friendly status/copy, pending total, small account list, add-account, settings, quit. Account/message clicks delegate to `ExternalNavigationService`; browser `/u/N/` positions are not stable identity.
