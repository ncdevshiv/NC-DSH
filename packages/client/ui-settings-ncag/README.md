---
description: "The NC-AG Settings page on the dsh web client: a live monitor of the host-plane platform plus the model, price, and limit overrides its agents use."
kind: "package-reference"
---

# @deepseek-ai/dsh-client-ui-settings-ncag

English | [中文](README.zh.md)

## Summary

Adds one Settings section, **NC-AG**, directly below **Agent presets**. The page monitors the autonomous multi-agent platform — health, usage, effective routes, projects, tasks, agents, decisions, and the event timeline — and edits the model overrides, prices, and limits its agents run with. The monitor reads the `ncag` Remote namespace; the configuration form reads the `ncag-settings` settings namespace.

## Table of Contents

- [Use this package](#use-this-package)
- [Understand the implementation](#understand-the-implementation)
- [Further Exploration](#further-exploration)
- [Model Experience](#model-experience)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)

-----

<a id="use-this-package"></a>
## Use this package

Open **Settings** and select **NC-AG** below **Agent presets**.

**Overview** shows the audit-chain state, the snapshot time, model calls and tokens, spend, project and live-agent counts, the resolved model route per tier with its effective price, the registered agent profiles, and every pending decision.

**Projects** lists the workstreams with their task, agent, and pending-decision counts; creating a project anchors it at the first workspace unless a repository path is given. Selecting a project reveals its tasks (cancel, reassign by capability), its agents (instruct, cancel), and its decisions. A work request submitted here is normalized and planned by the platform.

**Timeline** lists the newest platform events, and opens one correlation's trace — audit records, events, messages, and tool executions.

**Configure** holds the fast, standard, and reasoning tier overrides (`provider/model`), the optional USD price table (JSON), and the live-agent, helper, step, output-token, and spend ceilings. Zero disables the agent, helper, and USD ceilings; for steps and output tokens zero follows the deployment default.

<a id="understand-the-implementation"></a>
## Understand the implementation

The Host half is an empty `apply`; the browser half owns the page. `NcagMonitorController` polls `ctx.remote.ncag.snapshot` and `recentEvents` and exposes the operator actions; `NcagCardController` stages every configuration field over one `SettingsFormModel` bound to `ctx.configForms.get('ncag-settings')`. The section registers into the Settings shell's `settings.section` slot with `order: 25`, so it renders immediately below Agent presets, and its label is a thunk that follows the active locale without re-registration. A deployment without the platform rows still mounts the page and reports the platform as unavailable.

<a id="further-exploration"></a>
## Further Exploration

- [ncag-controller](../../api/ncag-controller/README.md) — the Remote namespace this page reads.
- [ncag-settings](../../ncag/ncag-settings/README.md) — the Host namespace the Configure tab edits.
- [ui-agent-preset](../ui-agent-preset/README.md) — the section directly above this one.

<a id="model-experience"></a>
## Model Experience

Indirectly: the saved overrides decide which provider and model every NC-AG agent uses, and the limits decide how many agents, steps, tokens, and dollars a project may consume.

#### KV Cache effect

A model override changes the route for later NC-AG agents; it does not rewrite an existing conversation.

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **Polling.** The monitor re-reads on an interval; a pushed timeline needs a stream method on the Remote.
- **Step and token zeros follow the deployment**, because the agent runtime requires finite numbers.
