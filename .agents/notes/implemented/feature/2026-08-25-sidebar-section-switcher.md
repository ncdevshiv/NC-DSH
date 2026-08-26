# Agent Note: Sidebar section switcher with Home/Code/Work/Team

Status: implemented

English | [中文](2026-08-25-sidebar-section-switcher.zh.md)


## Problem

The web GUI sidebar had exactly one browsing surface: the workspace/session browser filled the shell's single `sidebar.workspaces` hole. There was no place for the other things a multi-agent harness surfaces — what every agent is doing right now (goals, running sessions, interactions waiting on a person), or the agents themselves as a roster (agent presets to launch, live subagent members to inspect). Adding those as stacked panels inside the workspace browser would have buried the session list; adding them as separate full screens would have left the sidebar unable to answer "where am I?".

## Decision

The sidebar shell (`ui-sidebar`) owns a **section switcher**: a pill tab strip (Home / Code / Work / Team) above New Session when wide, the same four sections as 36px icon controls on the rail when collapsed (a rail pick also expands the column, because the section area is hidden while collapsed). The section area is the keyed slot `sidebar.section` (`kind: 'keyed'`, literal key union `SidebarSectionKey = 'home' | 'code' | 'work' | 'team'`), declared by the shell's `sidebar` entry; the shell dispatches the active key at its render site and passes no owner props — each section's business data and actions arrive through its own inject.

One client plugin package registers one key:

- `ui-home-section` → `home`: an inbox-style overview (summary chips, a New Session quick action, the newest non-blank sessions with state dots).
- `ui-workspace` → `code`: the existing `WorkspaceBrowser`, repointed from the retired `sidebar.workspaces` single slot. The browser simplified to always-wide: the shell hides the section area while collapsed, so the browser's rail-icon branch, the `wide`/`expandSidebar` owner share, and the `searchOnExpand` machinery were removed with their tests. Its directory-flow hole renamed to `sidebar.section.directoryFlow` (both directory-picker packages repointed).
- `ui-work-section` → `work`: a live board — "Needs you" (sessions blocked on a user interaction) leads, then "Running", then materialized `goal` projections with phase chips.
- `ui-team-section` → `team`: live members first (the session list's `origin: 'subagent'` rows, running first, opened through the runtime's subagent-address routing), then the agent-preset roster as startable teammates ("Start session" creates a blank session and mounts the preset via `agentPresets.select`, which refuses a started session — so the adoption lands on the fresh blank one the connect returned).

The active section is shell-local state booting on `code` (the historical surface). All four panes stay mounted, display-toggled like the layout columns, so a section keeps its local state across visits and across a collapse. A switch animates the incoming pane only: a directional slide + fade (200ms) retriggered by alternating two identical keyframe names (`section-in-a`/`section-in-b` — restarting a same-name animation is a no-op), aimed by the `--section-slide-from` custom property the shell sets from the tabs' index distance. Reduced-motion mode disables the animation.

Concept provenance: the roster → channel → watch-activity loop follows CopilotKit/OpenBot's coworkers and channels; agents-as-first-class-members and the audit-friendly framing follow block/buzz. Both are concept donors only — everything rides existing DSH services (session-list projections, `agentPresets`/`subagent` RPCs, the session log), no new backend process, database, or protocol.

## Alternatives considered

**Extend `ui-workspace` with tabs around the browser.** Lost because one UI feature is one plugin package (`packages/client/AGENTS.md` directory regime): Home/Work/Team share no data or store with the workspace browser, and a fifth section would grow a package that owns none of it.

**A `chain` slot with per-section selectors.** Lost because the tab set is closed and shell-owned, not elective: a chain elects one entry by declining others, which fits takeover routing (the composer), not a fixed set of tabs the user switches between.

**Keeping `wide`/`expandSidebar` on the section owner share.** Lost because no section renders in rail mode anymore (the area is hidden), so the share had no consumer; speculative API is rejected at package boundaries.

**A host-side cross-session work enumeration RPC before any UI.** Deferred, not rejected: needs-you and running are complete from the list projection today, while goals/jobs/workflow mirrors are per-opened-session; the board ships with the honest sparse goals section and the README records the host-RPC follow-up that slots in without component changes.

## Consequences

The sidebar gained a stable top-level navigation vocabulary; every future surface (per-member audit views, an inbox-style home feed, cross-session work enumeration) lands as a registration or a section-local change, not another shell change. The shell's rail contract changed shape: the region's rail icons are gone, replaced by the section rail, and `sidebar.workspaces` no longer exists (pre-release stance — renamed with every reference updated together, including the extension slot catalog, which is generated by `bun run gen-client-catalog`). The dependency override `use-sync-external-store: ^1.6.0` landed with this change out of necessity: the pinned 1.2.0 declares React ≤18 peers while the workspace runs React 19, so bun installed a nested React copy and every jsdom suite that mounts the renderer-bound hooks failed (~620 tests at clean HEAD).

## Testing

Per-package component specs cover each section's derivation and action arms; `ui-sidebar` specs cover the switcher (tab dispatch, rail pick + expand, static cold collapse), the keyed declaration, and the stylesheet contracts (rail-in targets, section-enter keyframes, reduced-motion). Sidebar DOM snapshots re-recorded. The assembled-output change is covered by `DSH_SNAPSHOT=replay test:web` before merging.