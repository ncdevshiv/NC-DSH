# Agent Note: NC-AG follows the main model and owns its overrides in Settings

Status: implemented

## Problem

The shipped Web `ncag` preset required `NCAG_MODEL_PRICING` before the platform could mount. `createModelPrices('[]')` threw, preset activation recorded `broken`, and the healthy-only chip in `packages/client/ui-agent-preset` hid **NC-AG mode**: a deployment with no price table saw four modes instead of five. Two further consequences followed from the same table:

- The platform could not start without a deployment environment, so a user could not select the mode from the interface that ships it.
- Every route needed a declared price before any agent could run, even when accounting was not the user's concern.

## Decision

### The user's main model is the default route

`ModelRouter.resolve()` accepts the session's main model as `defaultRoute` and prefers it for every capability tier. A per-tier override from the NC-AG settings tab (`override`) wins over the default; without either, route discovery still falls back to the harness registry. The preset and the standalone `dsh-bundle-ncag` patch read `agentDefaultModel.currentSelection()` at mount, so a user who never edits NC-AG settings gets every NC-AG agent on the same model as their main agent.

### Prices are optional overrides, tokens are not

`modelPricing` becomes an optional override table (`'[]'` loads). `ModelRouter.price()` returns `null` for an unpriced route, and `UsageRecord.costUsd` records `0` while `CostManager` still appends a ledger row with the real input, output and cached token counts. Token accounting therefore never depends on prices. `CostManager.budget()` enforces a ceiling only when one is configured; the default autonomy policy carries no USD ceiling.

### Limits and overrides live in one settings namespace

`@deepseek-ai/dsh-ncag-service` declares volatile Config fields that the settings document owns: per-tier model overrides (`fastModel`, `standardModel`, `reasoningModel`, written `provider/model`), `pricingOverrides`, and five limits (`uiMaxConcurrentAgents`, `uiMaxAgentsPerTask`, `uiMaxStepsPerTask`, `uiMaxOutputTokens`, `uiMaxCostUsd`). Zero means unlimited for the agent, helper and USD ceilings; zero means "follow the deployment default" for steps and output tokens, which the agent runtime requires as finite numbers. `runtimeSettingsOf()` parses the fields on every orchestrator sample, so a saved change applies to the next dispatch or project without a restart.

### UI surface

The browser settings page reads the same `ncag` namespace through `ctx.configForms.get('ncag')`, following the page pattern of `ui-settings-subagent`.

## Alternatives considered

- **Ship a guessed default price table.** Rejected: a stale or wrong price misstates spend while looking authoritative; the user can enter real prices in the tab instead.
- **Drop cost accounting entirely.** Rejected: the ledger remains the only evidence of how much a run consumed, and token counts must survive regardless of pricing.
- **Keep the required environment variable and document it.** Rejected: it preserves the hidden-mode failure, which is the defect this note addresses.

## Consequences

- **NC-AG mode** appears in the agent select with no environment configuration.
- An unpriced run records tokens and reports cost as unknown; a user who wants USD accounting prices the routes in the settings tab.
- A project without a USD ceiling never pauses for budget, and concurrency/help ceilings apply only when the user sets them.
- The standalone `ncag` profile keeps its documented environment contract; only the shipped Web preset and bundle relax the required pricing.

## Verification

- `packages/ncag/ncag-orchestrator/tests/units.spec.ts` covers unpriced discovery, the main-model default for every tier, and the per-tier override.
- `packages/ncag/ncag-service/tests/service.spec.ts` covers a zero-pricing mount and settings parsing.
- `apps/cli/tests/web-agent-presets.e2e.ts` mounts the `ncag` preset and asserts the `ncag_*` tools and service.
