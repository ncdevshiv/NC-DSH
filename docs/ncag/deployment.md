---
description: "How to install, configure and start NC-AG: required environment, model prices, credentials, the database, and the first project."
kind: "guide"
---

# NC-AG deployment

English | [中文](deployment.zh.md)

## Summary

NC-AG runs inside a `dsh` profile. It needs three things: a platform database path, a workspace directory, and a price for every model route it may use. Model credentials belong to the harness, so NC-AG never stores them. This page configures each of those, mounts the platform, verifies the boot, and creates the first project.

## Table of Contents

- [Requirements](#requirements)
- [Required configuration](#required-configuration)
- [Model routes and prices](#model-routes-and-prices)
- [Credentials](#credentials)
- [Mount the platform](#mount-the-platform)
- [Verify the boot](#verify-the-boot)
- [First project](#first-project)
- [Upgrades and schema migrations](#upgrades-and-schema-migrations)

-----

<a id="requirements"></a>
## Requirements

| Requirement | Why |
|---|---|
| Node 22.19+ or 24+ | the harness engine range; the platform uses `node:sqlite` |
| A registered harness workspace | a project without an explicit repository path is anchored at it |
| `git` on `PATH`, or `GIT_EXECUTABLE` | isolated worktrees, change sets and integration run real git |
| A shell (`powershell.exe` on Windows, `pwsh` elsewhere), or `NCAG_SHELL` | verification commands and the `shell` tool |
| A model provider with credentials | agents execute through the harness LLM service |

NC-AG never shells out for git through a shell string: it resolves the executable once (`GIT_EXECUTABLE`, otherwise `git`) and runs argv arrays, so a path with spaces works.

<a id="required-configuration"></a>
## Required configuration

| Variable | Required | Meaning |
|---|---|---|
| `NCAG_DATABASE_PATH` | no | Absolute path of the platform SQLite database; defaults under the harness home |
| `NCAG_WORKSPACE_ROOT` | no | Absolute directory holding agent worktrees; defaults under the harness home |
| `NCAG_MODEL_PRICING` | no | JSON price overrides; empty records tokens without USD |
| `NCAG_PREFERRED_PROVIDERS` | no | Comma-separated provider names in priority order |
| `NCAG_MAX_CONCURRENT_AGENTS` | no | Default live-agent ceiling per project (4) |
| `NCAG_MAX_STEPS` | no | Model steps per task before the loop stops (24) |
| `NCAG_MAX_OUTPUT_TOKENS` | no | Output ceiling per model call (4096) |
| `NCAG_SHELL` | no | Shell executable for commands and verification |
| `GIT_EXECUTABLE` | no | Git executable when `git` is not on `PATH` |
| `NCAG_PORT` | no | Control-plane port (`0` selects a free one) |
| `NCAG_HOST` | no | Control-plane bind address (`127.0.0.1`) |
| `NCAG_TOKEN` | no | Bearer token required on API calls; set it whenever the control plane is reachable from another machine |

A missing value now falls back to the harness home, so the shipped Web preset and bundle start with no environment. The platform never invents a route: an unpriced route still runs with token-only accounting, and a model route comes from the user's main model unless the settings tab names an override.

<a id="model-routes-and-prices"></a>
## Model routes and prices

`NCAG_MODEL_PRICING` is a JSON array of entries:

```json
[
  { "provider": "deepseek", "model": "deepseek-chat", "inputPerMillion": 0.27, "outputPerMillion": 1.1, "tier": "standard" },
  { "provider": "deepseek", "model": "deepseek-reasoner", "inputPerMillion": 0.55, "outputPerMillion": 1.6, "tier": "reasoning" }
]
```

`tier` is optional; without it the router infers a tier from the model id. Pricing is optional: a route without a price entry still runs and records its token counts, while its USD cost stays unknown. Price the routes you want the budget ceilings to enforce.

- A priced route contributes USD to the project ledger; an unpriced route contributes tokens only. Budget ceilings apply when you set one in the NC-AG settings tab.
- A profile asks for a tier, not a model. The router uses the user's main model for every tier until the settings tab names an override, preferring `NCAG_PREFERRED_PROVIDERS` when it must discover a route.

<a id="credentials"></a>
## Credentials

Model credentials belong to the harness credentials service. Set `DEEPSEEK_API_KEY` in the environment, or store it through the Web Models page. NC-AG never writes a credential into its database, its messages or its audit log; it asks the harness LLM service for a stream, and the harness resolves the credential.

<a id="mount-the-platform"></a>
## Mount the platform

```bash
dsh --profile web --patch packages/bundle/ncag/cordis.patch.yml
```

The patch inserts two rows: the platform service (`@deepseek-ai/dsh-tool-ncag/service`) and the tool plugin (`@deepseek-ai/dsh-tool-ncag`). The `ncag` profile composes `dsh-base`, the Web application and that bundle, so `dsh --profile ncag web` is the same tree. The first boot initializes `$DSH_HOME/profiles/ncag`.

The shipped Web application also mounts the platform without a patch profile: choose **NC-AG mode** in the agent select. Its preset inserts the same two rows inside an isolate realm, so the platform ends with the Session that selected the mode instead of publishing a host service. It reads `NCAG_DATABASE_PATH`, `NCAG_WORKSPACE_ROOT`, `NCAG_MODEL_PRICING` and the optional tuning variables, defaults both paths under `$DSH_HOME/ncag`, and shares one orchestrator per absolute database path with every other mount in the process.

<a id="verify-the-boot"></a>
## Verify the boot

```bash
dsh --profile ncag --dump-config | grep -A12 "dsh-bundle-ncag"
```

The dump must show the `ncag` row with the three required config values and the `tools-ncag` row. A boot in which the service failed logs `ncag (@deepseek-ai/dsh-tool-ncag/service): failed to import`; a boot in which the model port is missing logs `MISSING_CREDENTIAL` from the harness LLM plugin, and NC-AG tasks then report the missing route instead of running.

<a id="first-project"></a>
## First project

Through the model-facing tools:

```text
ncag_project  name: checkout-service, description: a small order API, repositoryPath: /abs/path/repo
ncag_request  projectId: <id from ncag_project>, text: "Add a REST endpoint that returns an order by id, with tests."
ncag_status   projectId: <id>
```

Through the control plane, when `NCAG_PORT` is set:

```bash
curl -X POST "$URL/api/projects" -H "authorization: Bearer $NCAG_TOKEN" -H "content-type: application/json" \
  -d '{"name":"checkout-service","description":"order API","repositoryPath":"/abs/path/repo"}'
```

A project created over a repository NC-AG generated itself gets an initial commit and the configured base branch, because a worktree needs a commit to branch from. A project over an existing repository is inspected as it is: NC-AG never rewrites a working repository to match a preference.

<a id="upgrades-and-schema-migrations"></a>
## Upgrades and schema migrations

The platform database carries `PRAGMA user_version`. A fresh database is created at the current version; an older database is migrated forward by the ordered migration list inside one transaction; a database newer than the build is refused rather than downgraded. Backup is copying the database file while the orchestrator is stopped; the JSONL-free layout means one file plus its WAL is the whole state.
