# @deepseek-ai/dsh-client-ui-settings-plugin-inventory

English | [中文](README.zh.md)

**Plugin list** tab for Web Settings with inline enable/disable and per-plugin summaries. The browser plugin registers one localized `settings.plugins.tab` contribution with id `all`; the Plugins section owns the navigation entry and tab chrome. It performs no Remote read during plugin activation. Selecting the tab for the first time mounts it and lazily calls `ctx.remote.pluginInventory.list()` through [`api-remotes`](../../api/remotes/README.md); toggling a card calls `pluginInventory/setEnabled` and refreshes the snapshot.

The tab renders a searchable two-column catalog of compact disclosure cards. Each collapsed card shows the short module name, the full specifier underneath, a small enablement tag, a colored root-fiber status dot for enabled entries, and an inline switch that toggles enablement without expanding. Expanding one card reveals a short summary of what the plugin does, what disabling it will do, an essential-plugin warning when applicable, the Loader-tree entry id, and the effective configuration and Cordis status (disabled entries omit the unmounted runtime state). The entry id remains the React key, disclosure identity, detail value, and an additional search target; it is never classified by string shape. Loading, empty, no-match, and generic failure states stay local to the mounted component, and a failed read or toggle can be retried without exposing transport details. Toggle errors are shown inline and do not close the card; an optimistic flip keeps the tag and switch responsive while the Remote settles. The registration uses `ctx.slots.inject()`, so it follows late tab declaration, redeclaration, locale changes, and teardown without importing the section owner.

## Model Experience

None, as this package only visualizes a Host-owned deployment snapshot in browser Settings and registers nothing model-facing.

#### KV Cache effect

None; this package neither assembles nor sends a provider request.

## Known Limitations and Deferred Work

- **One snapshot per Settings mount, retry, or successful toggle** — the tab does not subscribe to Loader changes or automatically refetch after reconnect; switching tabs preserves the current snapshot, while reopening Settings or toggling a plugin obtains a fresh one. A toggle is live for the current Host process; persistence across restarts requires a profile-patch write outside this UI.
- **No provenance or grouping** — local search does not add provenance, current-browser activation diagnosis, or grouping by source.
