---
description: "NC-AG architecture decision records: the significant choices behind the platform, with the alternatives and the tradeoffs."
kind: "reference"
---

# NC-AG architecture decision records

English | [中文](README.zh.md)

## Summary

These records state the significant design decisions behind NC-AG and what each one gave up. They describe the shipped platform in the present tense. A later record supersedes an earlier one explicitly instead of rewriting it.

## Table of Contents

- [ADR 1 — a platform-owned store](#adr-1)
- [ADR 2 — isolated worktrees with leases and fencing](#adr-2)
- [ADR 3 — evidence-based completion](#adr-3)
- [ADR 4 — model tiers instead of per-profile models](#adr-4)
- [ADR 5 — deterministic planning by default](#adr-5)
- [ADR 6 — the orchestrator decides, infrastructure enforces](#adr-6)

-----
<a id="adr-1"></a>
## ADR 1 — a platform-owned store

**Decision.** NC-AG owns its SQLite database instead of extending the harness session log.

**Why.** The platform runs many agents that are not harness Agent values, and their tasks, messages, leases and decisions must survive a harness restart without touching the session format. A separate database also lets the platform be embedded in any host process, including a standalone operator tool.

**Given up.** The platform does not appear in the harness session list and does not use the harness replay. It provides its own event stream and its own audit chain instead.

<a id="adr-2"></a>
## ADR 2 — isolated worktrees with leases and fencing

**Decision.** Every agent writes inside its own Git worktree, under a path lease carrying a monotonic fencing token, and changes land through the integration manager.

**Why.** Two agents editing one directory lose work between commit boundaries, and a shared tree cannot be rolled back per agent. A lease with a fencing token also refuses a slow agent that still holds an expired lease, which a timestamp check alone would not.

**Given up.** More setup per task (a worktree and a branch), and integration work that a shared directory would not need. The platform accepts that cost because losing an agent work is worse.

<a id="adr-3"></a>
## ADR 3 — evidence-based completion

**Decision.** A task completes only when every acceptance criterion has passing evidence recorded by an executed check. An agent report is never evidence.

**Why.** An agent claiming success is not proof of success, and a clean merge is not proof of a correct integration. Storing the command, exit code and output makes the claim checkable by a later reader.

**Given up.** Completion needs a configured check per verification method, and a deployment that configures no check cannot complete such a task. The platform reports that as a failure rather than passing it silently.

<a id="adr-4"></a>
## ADR 4 — model tiers instead of per-profile models

**Decision.** A profile declares a capability tier; the router resolves the tier to a concrete priced route using the deployment preferences and the selection policy.

**Why.** Per-profile model ids make every profile a deployment artifact and break when a provider renames a model. Tiers let one configuration drive the whole organization and keep the cheapest capable route for routine work.

**Given up.** A deployment cannot pin a profile to one exact model without pricing it as its own route. That is deliberate: an unpriced route is refused, so a pinned model is an explicit, costed choice.

<a id="adr-5"></a>
## ADR 5 — deterministic planning by default

**Decision.** Request analysis, requirement normalization and task-graph construction run through the deterministic analyzer. A model-assisted plan is not the default path.

**Why.** Planning happens before any agent exists, so a model call there would be unaccounted work with no owner, no route and no review. The deterministic analyzer produces the same record types a model plan would be validated into, so the two paths are interchangeable later without changing any downstream module.

**Given up.** A plan built from keywords and statement structure is simpler than one a reasoning model could produce for an ambiguous request. The platform compensates by recording every ambiguity and by raising a decision when an ambiguity is material.

<a id="adr-6"></a>
## ADR 6 — the orchestrator decides, infrastructure enforces

**Decision.** The orchestrator makes planning, scheduling, review and decision choices. The store, the runtime, Git and the lease manager enforce them, and the enforcement modules do not call back into the orchestrator for permission.

**Why.** A single intelligence that also mechanically enforces its own decisions becomes a single point of failure: a restart or a bug in it loses enforcement too. Keeping enforcement deterministic means state survives, and a replacement orchestrator resumes over the same enforcement layer.

**Given up.** Some operations take one more recorded step (a decision plus an enforcement call) than a tightly coupled implementation would. The audit trail is the return on that cost.

