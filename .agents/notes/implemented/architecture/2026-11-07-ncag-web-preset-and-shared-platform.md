# Agent Note: Expose NC-AG as a Web agent preset and share one orchestrator per database

Status: implemented

English | [中文](2026-11-07-ncag-web-preset-and-shared-platform.zh.md)

## Problem

The NC-AG platform was reachable only through the standalone `ncag` profile or an explicit `--patch` of `packages/bundle/ncag/cordis.patch.yml`. The shipped Web agent select listed only `standard`, `ptc`, `minimal` and `cordis`, so a Web user could not start the platform from the interface that ships it.

Mounting the platform per Session also exposed a scheduling defect: each mount constructed its own orchestrator over the same SQLite database, so two Sessions — or a preset beside the standalone bundle — could each observe the same `READY` task and dispatch it. The dashboard's documented `/trace` route additionally called a missing orchestrator method, failing the Host typecheck and leaving one control-plane operation unimplemented.

## Decision

### Add the `ncag` preset instead of a second platform composition

`packages/bundle/web-app/presets/ncag.patch.yml` declares the fifth shipped preset (`order: 5`). It reuses the standard toolset and adds one `cordis:group` with `isolate: { ncag: true }` holding the platform service and `tools-ncag`:

- A preset owns the services it mounts, so the platform must live in an isolate realm; without it the mount fails with `Preset services require isolate realms: ncag` and the service would outlive the Session.
- The preset reads the documented `NCAG_DATABASE_PATH`, `NCAG_WORKSPACE_ROOT`, `NCAG_MODEL_PRICING` and tuning variables, defaulting both paths under `$DSH_HOME/ncag`.
- `NCAG_MODEL_PRICING` defaults to `'[]'`. Prices are optional overrides; token accounting is always recorded, and a Settings-tab price entry enables USD accounting. The [follow-the-main-model decision](2026-11-08-ncag-settings-owned-models-and-limits.md) supersedes the earlier no-guess-prices requirement that made an empty table refuse the mount.

### Share one orchestrator per resolved database path

`@deepseek-ai/dsh-ncag-service` keeps a module-level map from resolved database path to a refcounted `SharedPlatform`. The first mount constructs the store and orchestrator; later mounts await the same `started` promise and increment the count. Releasing the last mount stops the orchestrator and closes the store. `:memory:` paths are keyed per mount so they stay private.

One platform per database is what makes duplicate dispatch impossible: the orchestrator's dispatch loop is the only writer of `IN_PROGRESS` transitions, and a second instance would race it.

### Record a run correlation and implement the trace

`audit()` accepts the run's correlation identifier and the task-run path passes its existing `correlation` into spawn, tool-call, handoff, security, review and integration records; actions outside a run mint their own. `NcagOrchestrator.trace(projectId, correlationId)` then reads the audit records, platform events, messages and tool calls recorded under that identifier and resolves the tasks and agents they reference. The dashboard `/trace` route calls it, so the operation the control plane documents is the operation that exists.

## Alternatives considered

- **Mount the platform on the Web host plane.** Rejected: every Web Session would start a platform and hold the database open even when no user selected the mode, and the service would outlive the Session that did.
- **Let each mount open its own orchestrator over the same database.** Rejected: two dispatch loops can both claim a `READY` task, and the second instance's restart recovery would rewrite another instance's live state.
- **Reach NC-AG only through the standalone profile.** Rejected: the agent select is where a user chooses how a Session works; requiring an application restart for a mode the Web bundle already ships hides the platform from the interface that carries it.

## Consequences

- Selecting **NC-AG mode** starts a platform scoped to the Session; deselecting it stops the last mount.
- The standalone bundle and a Session never dispatch the same task twice while both run.
- One identifier now groups a run's audit, events, messages and tool calls, which is the trace the dashboard renders; records written outside a run carry their own identifier and therefore a single-record trace.
