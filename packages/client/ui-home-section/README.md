# @deepseek-ai/dsh-client-ui-home-section

English | [中文](README.zh.md)

Sidebar Home section: an inbox-style overview rendered into the keyed `sidebar.section` slot under the `home` key. A summary strip (running sessions, total sessions, workspaces) sits above a New Session quick action and the recent-session inbox — non-blank sessions newest-first, each row carrying its state dot (running / needs-you / done), agent-preset label, and compact relative time; a row click opens that session. The section registers through `ctx.slots.inject('sidebar.section', …)` so activation order against the shell never matters, and the registration leaves with the caller's fiber.

All data rides the standard global hooks (`useSessions`, `useWorkspaces`); the only actions are the runtime's shared `startSession`/`open` verbs, injected from the apply closure. There is no plugin store: the inbox is a pure derivation over the session list snapshot, capped at the 30 newest non-blank rows.

The `/client` exports are the plugin body (`apply`/`inject`) plus the contract types only; the section component stays package-internal behind the slot registration.

## Model Experience

None, as the Home section renders the session list overview; nothing here reaches a model request.

#### KV Cache effect

None; this package neither assembles nor sends a provider request.

## Known Limitations and Deferred Work

- **The inbox is list-projection only** — no unread tracking exists yet; "done" rides the runtime's completed-while-away bit and clears when the session opens.
- **No cross-session goal/workflow aggregation** — the Work section (ui-work-section) owns that surface; Home intentionally stays a session inbox.
- **Relative time is computed at render** — the label does not tick while idle; it refreshes on the next list projection change.
