# @deepseek-ai/dsh-client-ui-team-section

English | [中文](README.zh.md)

Sidebar Team section: the agents you work with, rendered into the keyed `sidebar.section` slot under the `team` key — **live members first**. Members are the session list's subagent rows (who exists right now, running or inactive, newest first, running on top); clicking one opens its conversation through the runtime's subagent address routing. Below them the roster: the deployment's agent presets rendered as startable teammates — name, trust badge (System/User), description, and the broken state surfaced honestly instead of a dead Start button. "Start session" creates a blank session and mounts that preset on it (`agentPresets.select` refuses a started session, so the adoption lands on the fresh blank one the connect returned).

Roster data rides one snapshot store fed by the `agentPresets.list` wire face, refreshed on `connection/reset` (preset authoring writes files, so nothing else on the wire announces it). Member data is a pure derivation over the standard `useSessions` hook — no second subscription, no plugin store beyond the roster snapshot.

The `/client` exports are the plugin body (`apply`/`inject`) plus the contract types only; the section component stays package-internal behind the slot registration.

## Model Experience

None, as the Team section renders the roster and member list; nothing here reaches a model request.

#### KV Cache effect

None; this package neither assembles nor sends a provider request.

## Known Limitations and Deferred Work

- **Member activity is the list projection only** — per-member transcript drilldown and audit views (the OpenBot Activity-tab concept) are deferred; a member row opens the existing conversation surface.
- **No team groupings yet** — the Buzz `AgentTeam` concept (named groups of presets with shared instructions) is deferred; the roster renders the flat preset list.
- **The roster refreshes on reconnect only** — a preset authored while this browser stays connected appears after the next reload or reconnect; the settings surfaces own authoring-time refresh for their own views.
