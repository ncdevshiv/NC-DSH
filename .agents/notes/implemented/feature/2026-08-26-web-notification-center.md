# Agent Note: Web notification center

Status: implemented

## Problem

The host already owned two facts a Web human could not see: the notification registry (`dsh-notifications`) that host plugins publish system notices into, and the sidecar-update pipeline (`dsh-sidecar-updates`) that stages AI SDK releases. Neither had any browser surface, so a notice published by a plugin was invisible until it happened to enter a transcript, and a staged update sat silent until the next engine start.

Placement had no obvious answer. The frame declares exactly four child slots — `sidebar`, `conversation`, `details`, `shell.overlay` ([ui-layout](../../../../packages/client/ui-layout/README.md)) — and none is an app-level top bar. The only session-independent icon cluster in the shell is the sidebar foot's additive `sidebar.footer.action` list beside the settings seat, which the Cordis inventory panel already occupies additively.

## Decision

One new plugin package, `dsh-client-ui-notifications`, contributes one `sidebar.footer.action` entry (`notifications-bell`): a bell trigger whose dropdown panel holds the notice list and the AI SDK update card.

- **The wire contract is frozen ahead of the gateway.** The API gateway's `updates`/`notifications` domains land in parallel, so the package declares its narrow structural faces locally (`UpdatesFace`/`NotificationsFace` over `{ result: ok | err }` envelopes) and reads them off the connection handle. When `IApiClient` declares the domains, the local types are replaced by `Pick<IApiClient, 'updates' | 'notifications'>` verbatim.
- **Each domain owns its inline error line** (`sdkError`, `noticesError`). A success clears only its own line; a successful notice pull must not erase an update-domain failure or vice versa. A refused write skips its trailing re-read, because the write changed no server fact and a fresh read would clear the error it just surfaced.
- **Writes settle before render**: read/dismiss/install/skip refetch after settling, never optimistically. Status freshness rides the entry mount (immediate sync, 60-second poll, focus refetch); notices pull on open and after mutations.
- **Attention splits between two indicators**: the badge counts unread-and-not-dismissed notices; a separate dot marks an offered, non-ignored update only when nothing unread competes for it.
- **Install success copy is derived from the settled re-read**, not the request: `installedNow` shows "takes effect next start" plus the release-notes link, and retires when a later status reports another available version.

Verification: per-file 100% coverage on the package sources; component specs drive the store through the frozen fixtures (`tests/wire-fake.client.ts`), asserting badge/dot counts, open/close, install busy→success-copy swap, skip/check flows, and both error lines.

## Consequences

A Web human now sees system notices and staged SDK updates without reading transcripts or logs, and can act (read, dismiss, install, skip) from one panel whose every value is host-served. The sidebar foot becomes the home of app-global controls: a second additive occupant exists beside the Cordis panel, so future global surfaces should prefer this seat over new shell slots. The locally declared wire faces are deliberate debt with a mechanical retirement path; until then, gateway shape drift in these two domains is caught by integration rather than by this package's compiler.

## Alternatives considered

| Rejected | One-line reason |
|---|---|
| Session-header utilities slot | Session-scoped: the bell would vanish with no active session for facts that are app-global |
| New frame-level top-bar slot | Inventing a shell slot for one occupant; the footer action list is the existing additive seat |
| Importing view types from host packages now | Couples the client to artifacts the parallel backend has not landed; local structural types keep this package compiling and are drop-in replaceable |
| One shared error line | Cross-domain clearing made failures flicker away on unrelated successes |
