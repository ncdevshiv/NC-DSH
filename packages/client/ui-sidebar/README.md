# @deepseek-ai/dsh-client-ui-sidebar

English | [中文](README.zh.md)

Sidebar shell plugin: the brand row, the section switcher, the New Session action, the layout-owned collapse control, the scroll-aware section-area seat, and the bottom-pinned Settings seat. The section area is the keyed `sidebar.section` slot — [ui-home-section](../ui-home-section/README.md) renders Home, [ui-workspace](../ui-workspace/README.md) the Code browser, and the Work and Team keys await their section packages. This package owns only the switcher chrome; it neither derives any section's rows nor owns their view preferences. Collapse into the layout-owned 56px rail remains presentation-local. Contract: the [slot system standard](../../../.agents/notes/implemented/architecture/2026-07-22-slot-type-chain-implementation.md).

The expanded brand row renders `sidebar.brand.mark` and `sidebar.brand.name` as independent single slots, while the collapsed rail renders the same mark slot. Without occupants, the shell uses the fish mark and a `DSH Local Build` label carrying the build's 7-character `DSH_CLIENT_COMMIT_HASH` badge. A deployment package can replace either value without replacing the New Session control or rail geometry; declaration-aware `slots.inject()` lets such a package activate before or after the sidebar.

New Session starts the runtime's page-local frontend Session Intent. The runtime targets the explicit Workspace used by a scoped action, otherwise the current Session's Workspace, otherwise the most recently active Workspace; when none exists it lands in the workspace-less chat session. The shell renders the action's outcome at the control: a busy label while the connect runs, and a rejected connect as an inline alert — under load a create can take a while, and before this feedback both looked like a dead button. Workspace-specific controls and the shared picker belong to ui-workspace.

The section switcher owns the region between the brand row and the foot. Wide, it is a pill tab strip (Home / Code / Work / Team) above New Session; collapsed, the same four sections are 36px icon controls on the shared rail entry path, and picking one also expands the column (the section area is hidden while collapsed, so a rail pick without expansion would show nothing). The active section is shell-local state booting on Code; the four section panes stay mounted beneath the switcher (display-toggled, the layout columns' pattern) so a section keeps its local state across visits and across a collapse.

`SidebarRootComponentProps` composes the layout owner share, the global `useSessions` and `useWorkspaces` hooks, the declared brand, the keyed `sidebar.section`, and `sidebar.settings` child slots, and injected `startSession` plus sidebar-toggle callbacks. There is no plugin store.

A section switch animates the incoming pane only: a directional slide + fade (200ms) retriggered by alternating two identical keyframe names — restarting a same-name animation is a no-op — and aimed by the `--section-slide-from` custom property the shell sets from the tabs' index distance. Reduced-motion mode disables the animation.

During a live collapse, the shell holds the expanded content at its current width while it fades out for 150ms. The upper controls—the shell toggle and New Session plus the section rail—then share one 150ms fade and 49px leftward translation into the 56px rail, ending with the layout's 300ms column slide; every 36px control box follows the same path to the rail's 10px left inset. The bottom-pinned `sidebar.settings` control shares the fade timing but has no horizontal translation. A page that starts collapsed renders the rail statically, and reduced-motion mode disables both transitions.

Scrollbars in the column are a pointer affordance: the shell rebinds ui-theme's [scrollbar indirection](../ui-theme/README.md) to `transparent` whenever the pointer is outside it, and keeps the thumb drawn for 2s after the pointer leaves, so a list nobody is pointing at carries no bar. The reservation that keeps rows from moving belongs to the scrolling region ([ui-workspace](../ui-workspace/README.md)), so revealing a thumb never reflows.

The foot is the `sidebar.settings` seat: the sidebar renders only the bottom-pinned layout slot and shares its column state (`wide`); ui-settings registers the trigger row and settings panel there.

The `/client` exports are the plugin body (`apply`/`inject`) plus the contract types only; SidebarRoot, the row components, and the tree derivation remain package-internal behind the slot registration.

## Model Experience

None, as the sidebar renders the browser session list; nothing here reaches a model request.

#### KV Cache effect

None; this package neither assembles nor sends a provider request.

## Known Limitations and Deferred Work

- **Session state-dot rendering is owned by [ui-workspace](../ui-workspace/README.md)** — no done/error notification sources are available.
- **Workspace browser behavior is composition-owned** — grouping, ordering, search, and row state belong to [ui-workspace](../ui-workspace/README.md), not this shell.
- **"New task completed" unread marking is local viewing state** — completion-time > last-seen never reaches the host.
