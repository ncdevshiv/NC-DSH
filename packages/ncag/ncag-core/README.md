---
description: "The NC-AG domain vocabulary: branded identifiers, durable record types, the agent and task lifecycle machines, and the built-in profile registry."
kind: "package-library"
---

# @deepseek-ai/dsh-ncag-core

English | [中文](README.zh.md)

## Summary

This package is the vocabulary every NC-AG module speaks: identifiers, records, lifecycle machines and profiles. It has no I/O, no service and no state. Its value is that the domain cannot drift: a task status is one of the enumerated states, a transition is checked against one table, and an identifier is branded so a project id cannot be passed where a task id is expected.

## Table of Contents

- [Exports](#exports)
- [Identifiers](#identifiers)
- [Lifecycle machines](#lifecycle-machines)
- [Profiles](#profiles)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)
- [Dev Note](#dev-note)

-----

<a id="exports"></a>
## Exports

| Module | Contents |
|---|---|
| `ids.ts` | `ProjectId` … `CorrelationId` brands, `newId`, `adoptId`, and the `IdFor` family map |
| `types.ts` | The record types: project, request, requirement, task, agent instance, profile, message, decision, finding, evidence, lease, change set, workspace, audit record, event |
| `machines.ts` | `AGENT_TRANSITIONS`, `TASK_TRANSITIONS`, `transitionAgent`, `transitionTask`, `canTransition*`, `allowedTransitions`, `NcagStateError` |
| `profiles.ts` | `BUILT_IN_PROFILES`, `builtInProfile`, `validateProfileSeed`, `profilesForCapability`, `tierForCapability` |

<a id="identifiers"></a>
## Identifiers

`newId(kind)` mints a prefixed UUID (`prj_…`, `tsk_…`, `agt_…`) and returns the exact brand for that family, so no cast is needed at the call site. `adoptId(kind, text)` re-establishes the brand for text read back from the database, and refuses text whose prefix does not match the family. That refusal is why a corrupt or mismatched identifier fails at the read instead of propagating.

<a id="lifecycle-machines"></a>
## Lifecycle machines

`transitionAgent(from, to)` and `transitionTask(from, to)` return the requested state or throw `NcagStateError` naming both states and the machine. A self-transition returns the state unchanged. `canTransitionAgent` and `canTransitionTask` answer without throwing, for planners and dashboards. `allowedTransitions(machine, state)` returns the full destination list.

The tables are the authority. `STOPPED` has no outgoing transition, `FAILED` may only `RECOVERING` or `STOPPING`, and `COMPLETED` is terminal for a task. Every persisted status change in the platform goes through one of these functions.

<a id="profiles"></a>
## Profiles

The registry ships sixteen seeds. `builtInProfile(seed)` attaches the stable `prf_builtin_<key>` identity. `validateProfileSeed` enforces the key format, uniqueness, and the presence of a capability, an identity, responsibilities and restrictions, so a created profile is complete before it can be used.

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **Profiles are a compile-time table.** The registry is data in `profiles.ts`; a deployment adds profiles at runtime through `AgentRegistry.create`, which persists them, so no code change is needed to specialize. A file-backed profile pack would remove the last compile-time surface and has no consumer yet.

<a id="dev-note"></a>
## Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

Adding a state is a change to `types.ts` and `machines.ts` together; adding a state to one without the other makes a state reachable that nothing can leave, or a transition the type system rejects. Adding a profile is a change to `profiles.ts` only.

</details>
