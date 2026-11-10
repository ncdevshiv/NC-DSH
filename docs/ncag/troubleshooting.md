---
description: "The failures an NC-AG operator actually sees, what each one means, and the exact fix."
kind: "guide"
---

# NC-AG troubleshooting

English | [中文](troubleshooting.zh.md)

## Summary

This page maps each failure NC-AG reports to its cause and its fix. The platform fails loudly and names what it could not find, so most entries are a missing configuration value rather than a defect.

## Table of Contents

- [Boot failures](#boot-failures)
- [Task execution failures](#task-execution-failures)
- [Verification failures](#verification-failures)
- [Integration failures](#integration-failures)
- [Cost and budget](#cost-and-budget)
- [Control plane](#control-plane)

-----

<a id="boot-failures"></a>
## Boot failures

| Message | Cause | Fix |
|---|---|---|
| `ncag: NCAG_DATABASE_PATH must name the platform database file` | the variable is unset or empty | export an absolute path |
| `ncag: NCAG_WORKSPACE_ROOT must name the directory that holds agent workspaces` | same, for the workspace root | export an absolute directory |
| `ncag: NCAG_MODEL_PRICING must hold the model price table so cost accounting cannot report zero` | only the standalone `ncag` profile requires it | export the JSON array, or use the shipped Web preset and set prices in the NC-AG settings tab |
| `ncag: the model price table must be a non-empty JSON array` | malformed or empty in the standalone profile | each entry needs `provider`, `model`, and two non-negative per-million prices |
| `cannot get property "llm" without inject` | the service read a service it did not inject | the service reads optional services strictly; a missing one becomes a reported missing route, not a boot crash |
| `dsh: MISSING_CREDENTIAL` | the harness LLM plugin has no credential for its provider | set `DEEPSEEK_API_KEY` or store it through the Web Models page |
| `failed to import ... lib/index.js` | the package was not built or linked | run the artifact build, then reinstall workspace links |

<a id="task-execution-failures"></a>
## Task execution failures

| Task reason | Cause | Fix |
|---|---|---|
| `no model route is configured, so no agent can execute this task` | the platform has no model port | mount the harness LLM service, or supply a port to `runPlatform` |
| `ncag: route <provider>/<model> has no configured price` | the router found a usable route the deployment did not price | add the price entry |
| `ncag: no registered profile serves capability ...` | a task requires a capability no profile provides | register a profile with that capability before submitting the request |
| `refused by policy (...)` in a tool result | the profile lacks the tool or permission, or the command matched a destructive pattern | widen the profile deliberately, or change the requested approach |
| `ncag: task ... cannot start: dependencies still open: ...` | the scheduler was asked to start a blocked task | this is the guard working; the task starts when its dependencies complete |
| `recovery exhausted N retries` | the task failed `maxRetries` times | inspect the findings and evidence, fix the underlying cause, then resubmit |

<a id="verification-failures"></a>
## Verification failures

| Evidence or reason | Cause | Fix |
|---|---|---|
| `criterion ... declares no command` | the method has no command in this deployment | add the method to the project `checks` |
| `spawnSync <exe> ENOENT` | the configured shell executable does not exist | set `NCAG_SHELL` to an existing shell |
| exit code non-zero with real output | the check genuinely failed | fix the delivered behavior and resubmit |
| `acceptance criteria without passing evidence: ...` | a criterion has no passing evidence row | re-run verification after fixing the check |

<a id="integration-failures"></a>
## Integration failures

| Report | Cause | Fix |
|---|---|---|
| `textual conflict: ...` | two change sets touched the same lines | resolve in a new submission; the base branch is untouched |
| `integration validation failed: ...` and the merge was rolled back | the integrated tree failed its validation | fix the change set; the base branch is unchanged |
| `base branch moved since submission` | the project advanced while the agent worked | re-submit against the current base |
| `ncag: git ... failed in ...` | git is missing or the repository is not a work tree | set `GIT_EXECUTABLE`, or pass an existing repository path |

<a id="cost-and-budget"></a>
## Cost and budget

A project that reaches `maxCostUsd` becomes `PAUSED` as soon as the call that crossed the ceiling is recorded, and a `user-reserved` decision asks for more spend. Inspect it through `GET /api/projects/:id/decisions`, resolve it with `POST /api/decisions/:id/resolve`, then `POST /api/projects/:id/resume`.

<a id="control-plane"></a>
## Control plane

| Symptom | Cause | Fix |
|---|---|---|
| `401 {"error":"invalid token"}` | wrong or missing bearer token | use the configured `NCAG_TOKEN`, as a header or `?token=` |
| `500 {"error": ...}` | the operation threw | the message is the orchestrator's own reason; read it, fix the cause, retry |
| the dashboard shows `audit chain broken at N` | a record was modified or removed | investigate; the chain is what proves the record history |
| the event stream reconnects repeatedly | the server restarted, or a proxy closed the stream | the `EventSource` reconnects automatically; the dashboard re-renders on each event |
