# @deepseek-ai/dsh-client-ui-notifications

English | [中文](README.zh.md)

Web notification-center owner: contributes one bell entry to `sidebar.footer.action` that opens the dropdown panel holding the system-notice list and the AI SDK update card. Both sections read the frozen `notifications` and `updates` wire faces off the connection handle, so this package issues no session RPC, holds no state beyond popover visibility, and treats the host as the single fact source.

The trigger badge counts notices that are neither read nor dismissed; a separate dot marks an offered-but-not-ignored AI SDK update whenever no unread notice is competing for attention. Opening the panel pulls the notice list fresh, and every mutation (read, dismiss) refetches after it settles — nothing renders optimistically. A row click marks the notice read; the ✕ control dismisses it. Dismissed rows never render; a host that lists none shows the empty-state copy.

The update card shows the installed and latest tags. While a newer release is offered and not ignored, the card offers Install (busy label while in flight), Skip this version (writes the ignore list, then re-reads the status), and Check now (explicit freshness check). After a successful install the card swaps to "Installed {tag} — takes effect next start" with the latest release's page as the release-notes link; the copy retires when a later status reports another available version. Each domain surfaces its own inline error line — local operation failures plus the status view's `lastError` — and no failure ever throws to the component.

Freshness of the status rides the entry's mount lifetime: one immediate sync, a poll every 60 seconds, and a refetch when the window regains focus. Notice data is pulled on open and after mutations only. Styling uses tokens only; copy goes through the package's own `notifications` locale namespace. The wire contract is frozen against shapes the API gateway lands in parallel, so this package declares its narrow structural face locally until `IApiClient` declares the two domains.

## Model Experience

None, as this package renders host-computed notice/update state for a human and touches no prompt, message, schema, stream, or tool result. Reads and writes go through the wire domains; the model never sees this panel or its actions.

#### KV Cache effect

None; the package never assembles or sends provider requests.

## Known Limitations and Deferred Work

- **The wire face is locally declared** — `UpdatesFace`/`NotificationsFace` mirror the frozen contract structurally. When the gateway lands `updates`/`notifications` on `IApiClient`, the local types are replaced by `Pick<IApiClient, 'updates' | 'notifications'>`; until then a gateway shape drift would not be caught by this package's compiler.
- **Notice rendering is generic** — every live notice renders as title/age/snippet regardless of `kind`, and `data` payloads are ignored. Richer per-kind cards should arrive with the first producer that needs one.
