---
description: "The dsh bundle that mounts the NC-AG autonomous multi-agent platform and its model-facing tools."
kind: "package-bundle"
---

# @deepseek-ai/dsh-bundle-ncag

English | [中文](README.zh.md)

## Summary

This bundle inserts two rows beside the harness composition: the NC-AG platform service and the tool plugin that exposes `ncag_project`, `ncag_request`, `ncag_status` and `ncag_decide`. The orchestrator spawns its own agents through the harness LLM service, so every NC-AG role runs on the same configured provider routes as the rest of dsh.

## Table of Contents

- [Use this bundle](#use-this-bundle)
- [Configuration](#configuration)
- [What it does not replace](#what-it-does-not-replace)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)
- [Dev Note](#dev-note)

-----

<a id="use-this-bundle"></a>
## Use this bundle

```bash
dsh --profile web --patch packages/bundle/ncag/cordis.patch.yml
```

The `ncag` profile stacks `dsh-base`, the Web application and this bundle, so `dsh --profile ncag web` is the same tree with one name.

<a id="configuration"></a>
## Configuration

| Variable | Required | Meaning |
|---|---|---|
| `NCAG_DATABASE_PATH` | no | absolute platform database path (defaults under the harness home) |
| `NCAG_WORKSPACE_ROOT` | no | absolute agent workspace directory (defaults under the harness home) |
| `NCAG_MODEL_PRICING` | no | JSON price overrides; empty records tokens without USD |
| `NCAG_PREFERRED_PROVIDERS` | no | provider names in priority order |
| `NCAG_MAX_CONCURRENT_AGENTS` | no | live-agent ceiling per project |
| `NCAG_MAX_STEPS` | no | model steps per task |
| `NCAG_MAX_OUTPUT_TOKENS` | no | output ceiling per call |

The platform loads with no environment. The model route follows the user's main model until the NC-AG settings tab names a per-tier override.

<a id="what-it-does-not-replace"></a>
## What it does not replace

Nothing. The patch only inserts rows; it does not swap the filesystem provider, the tool registry, the shell, the model adapter or any other base row. NC-AG runs beside the harness agent loop and uses the harness LLM service as its model port.

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **The control plane is separate.** This bundle mounts the platform service; serving the dashboard over HTTP is [`ncag-dashboard`](../../ncag/ncag-dashboard/README.md), which a standalone deployment starts through `runPlatform`.
- **The profile template ships with the Web application.** A headless NC-AG-only profile would need a row list without the Web bundle; the `ncag` template composes the Web application because that is the composition an operator uses.

<a id="dev-note"></a>
## Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

The two rows are separate packages so the service can be composed without the tools (a headless platform) and the tools can be composed against a platform service mounted elsewhere.

</details>
