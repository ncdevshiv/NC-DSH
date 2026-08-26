# @deepseek-ai/dsh-client-ui-work-section

English | [中文](README.zh.md)

Sidebar Work section: a live board of what the agents are doing, rendered into the keyed `sidebar.section` slot under the `work` key. **Needs you** leads — the sessions blocked on a user interaction (approvals, questions), where the human is the bottleneck. **Running** lists every executing session with its state dot and recency. **Goals** renders each session whose `goal` projection value is materialized client-side with its phase chip (Active / Paused / Blocked) and the blocked reason; a row click opens that session.

All data is a pure derivation over the standard `useSessions` hook — needs-you and running are complete for the whole account because they are list-projection facts; goals are sparse by design until the cross-session enumeration lands (see below). No second subscription, no plugin store.

The `/client` exports are the plugin body (`apply`/`inject`) plus the contract types only; the section component stays package-internal behind the slot registration.

## Model Experience

None, as the Work section renders the work board; nothing here reaches a model request.

#### KV Cache effect

None; this package neither assembles nor sends a provider request.

## Known Limitations and Deferred Work

- **Goals are materialization-scoped** — a session's goal appears only after that session has been opened in this browser (the projection store fills from the history tail). The complete cross-session board needs a small host-side enumeration RPC in the api-proxy pattern (documented in the integration report); the UI is shaped so that RPC slots in without component changes.
- **Background jobs and workflow runs are not on the board yet** — their mirrors (`jobsBySession`, workflow-run log folds) are also per-opened-session today, so they would render the same sparse picture; they join with the same host-side enumeration.
- **Read-only board** — pause/resume exists on the wire (`goal.*`) but the section defers mutations to the session's own GoalBar surface.
