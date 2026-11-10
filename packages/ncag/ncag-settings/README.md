---
description: "Host settings namespace for NC-AG: per-tier model overrides, price overrides and optional limits edited from the Web settings tab."
kind: "package-plugin"
---

# @deepseek-ai/dsh-ncag-settings

English | [中文](README.zh.md)

## Summary

Owns the `ncag-settings` settings namespace. The Web settings tab edits it, and the NC-AG platform reads it on every dispatch, so a saved change applies to the next project or operation without restarting. Every override is optional: an empty model field follows the user's main model, an empty price table records tokens without USD, and a zero limit is unlimited.

## Table of Contents

- [Use this package](#use-this-package)
- [Understand the implementation](#understand-the-implementation)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)

-----

<a id="use-this-package"></a>
## Use this package

Mount the row beside the Web application and before the `ncag` preset's platform row. The platform service resolves `ctx.ncagSettings` from the global service store, so the preset does not need its own settings copy.

| Field | Meaning |
|---|---|
| `fastModel`, `standardModel`, `reasoningModel` | `provider/model` override for one tier; empty follows the user's main model |
| `pricingOverrides` | JSON price entries; empty records tokens only |
| `uiMaxConcurrentAgents` | live-agent ceiling per project; 0 is unlimited |
| `uiMaxAgentsPerTask` | helper ceiling per task; 0 is unlimited |
| `uiMaxStepsPerTask` | model steps per task; 0 follows the deployment default |
| `uiMaxOutputTokens` | output tokens per call; 0 follows the deployment default |
| `uiMaxCostUsd` | project spend ceiling; 0 is unlimited |

<a id="understand-the-implementation"></a>
## Understand the implementation

`runtimeSettingsOf()` converts the stored values into `NcagRuntimeSettings`: it parses each `provider/model` text into a route, parses the price overrides, and maps zero limits to `null` (unlimited). The platform samples that view, so the settings document is the single source of truth and no restart is required.

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **Usage analytics are not rendered here.** The settings page edits configuration; per-project usage is reported by the `ncag_status` tool and the control plane.
- **Step and token zeros follow the deployment.** The agent runtime requires finite numbers, so these two limits cannot be truly unbounded.
