---
description: "The sixteen built-in NC-AG agent specializations, what each owns, and how to register a new one without proliferating agents."
kind: "reference"
---

# NC-AG agent profiles

English | [中文](agent-profiles.zh.md)

## Summary

A profile is a reusable specialization: identity, personality, expertise, capabilities, permitted tools, permissions, responsibilities, restrictions, required inputs, deliverables, quality standard, escalation rules and performance metrics. Profiles are data. Planning selects one by capability, and the orchestrator spawns an instance per task. [`ncag-core/profiles.ts`](../../packages/ncag/ncag-core/src/profiles.ts) is the registry table.

## Table of Contents

- [Built-in profiles](#built-in-profiles)
- [Capabilities and tiers](#capabilities-and-tiers)
- [Adding a profile](#adding-a-profile)

-----

<a id="built-in-profiles"></a>
## Built-in profiles

| Key | Role | Capabilities | Permitted tools | Tier |
|---|---|---|---|---|
| `general-purpose` | miscellaneous work, research, support | research, analysis, general-implementation | read, glob, grep, write, edit | standard |
| `system-architect` | boundaries, interfaces, architecture record | architecture, interface-design, dependency-analysis, architecture-review | read, glob, grep, write, edit | reasoning |
| `backend-engineer` | services, business logic, authorization | backend-implementation, server-testing, persistence | read, glob, grep, write, edit, shell | standard |
| `frontend-engineer` | interface, state, accessibility | frontend-implementation, ui-accessibility, frontend-testing | read, glob, grep, write, edit, shell | standard |
| `api-engineer` | schemas, endpoints, versioning | api-contract, api-implementation, api-validation | read, glob, grep, write, edit, shell | reasoning |
| `database-engineer` | schema, migrations, query performance | database-design, migrations, query-optimization | read, glob, grep, write, edit, shell | standard |
| `devops-engineer` | build, deployment, CI, health | devops, deployment, observability, operational-health | read, glob, grep, write, edit, shell | standard |
| `documentation-engineer` | product, API, operational docs | documentation, documentation-review | read, glob, grep, write, edit | fast |
| `qa-engineer` | functional, regression, acceptance verification | test-planning, functional-verification, regression-testing | read, glob, grep, write, edit, shell | standard |
| `e2e-test-engineer` | journeys, workflows, failure paths | e2e-testing, journey-testing, failure-path-testing | read, glob, grep, write, edit, shell | standard |
| `security-auditor` | auth, secrets, permissions, dependency risk | security-review, threat-analysis, permission-audit | read, glob, grep, shell | reasoning |
| `code-reviewer` | correctness, maintainability, regressions | code-review, quality-review | read, glob, grep, shell | reasoning |
| `critiquer` | assumptions, scenarios, alternatives | critique, assumption-challenge, alternative-analysis | read, glob, grep | reasoning |
| `integration-engineer` | merge coordination, semantic conflict | integration, merge-conflict-resolution, integration-validation | read, glob, grep, write, edit, shell | reasoning |
| `performance-engineer` | measurement, load, optimization | performance-analysis, load-testing, optimization | read, glob, grep, shell | standard |
| `product-analyst` | intent, requirements, traceability | requirements-analysis, journey-definition, acceptance-definition | read, glob, grep | standard |

Every permission a profile holds is enforced before a tool runs: `workspace:read` covers `read`, `glob` and `grep`; `workspace:write` covers `write` and `edit`; `shell:execute` covers the shell tool. A profile that lists a tool without its permission cannot execute it, and the refusal is audited.

<a id="capabilities-and-tiers"></a>
## Capabilities and tiers

Planning matches a task's `requiredCapability` against the registry. The task's execution tier comes from the first profile that provides the capability, and the router then selects the cheapest route serving that tier among the preferred providers. That is how a standard-tier role lands on a cheap model while architecture and review land on a reasoning model, without any per-task model configuration.

<a id="adding-a-profile"></a>
## Adding a profile

`AgentRegistry.create(seed)` validates the seed first: the key must be 3-40 lowercase alphanumeric or dash characters and must not collide, at least one capability is required, identity and responsibilities must be stated, and restrictions must be declared. The registered profile is durable, so a restart keeps it.

```ts
const profile = orchestrator.agents.create({
  key: 'mobile-engineer',
  role: 'Mobile engineer',
  identity: 'You own the mobile client.',
  personality: 'Platform-pragmatic; you test on a real target.',
  expertise: ['iOS', 'Android'],
  capabilities: ['mobile-implementation'],
  toolNames: ['read', 'write', 'edit', 'shell', 'request_help', 'hand_off'],
  responsibilities: ['Implement the mobile client against the approved contract.'],
  restrictions: ['Do not change the published API contract.'],
  permissions: ['workspace:read', 'workspace:write', 'shell:execute', 'communication:send', 'resource:lease'],
  requiredInputs: ['approved interface contract'],
  deliverables: ['implementation', 'device test output'],
  qualityStandard: 'Every screen is exercised on a device or emulator with recorded output.',
  escalationRules: ['Escalate when the approved contract cannot express the required behavior.'],
  performanceMetrics: ['device test pass rate'],
  defaultTier: 'standard',
})
```

The orchestrator checks for an existing profile that already provides the capability before creating another one, so a task never spawns a redundant specialization.

Every built-in profile lists the two coordination tools, because an agent that cannot ask for help or hand work off is on its own: `request_help` needs the `communication:send` permission, and `hand_off` needs the same one. A profile without that permission may still use the filesystem and shell tools it declares, but it cannot reach another agent.
