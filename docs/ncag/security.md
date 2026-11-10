---
description: "The NC-AG security model: agent permissions, workspace containment, untrusted content, secret handling, and audit integrity."
kind: "reference"
---

# NC-AG security

English | [中文](security.zh.md)

## Summary

Every NC-AG agent is an untrusted execution identity. It holds exactly the permissions its profile declares, works inside exactly one worktree, and receives retrieved content as data. This page states each control and where it is enforced.

## Table of Contents

- [Permissions](#permissions)
- [Workspace containment](#workspace-containment)
- [Destructive commands](#destructive-commands)
- [Untrusted content](#untrusted-content)
- [Secrets](#secrets)
- [Audit integrity](#audit-integrity)
- [The control plane](#the-control-plane)

-----

<a id="permissions"></a>
## Permissions

`checkToolCall(profile, toolName, args)` runs before every tool execution, inside the tool wrapper the orchestrator builds for that agent. It refuses a tool the profile does not list (`tool-not-granted`), and a listed tool whose permission the profile lacks (`permission-missing`). The refusal is written to the audit chain with the rule, the detail, the agent, the task and the correlation id, and the model receives the refusal text as a failed tool result. Permissions are enforced at the executor, not by prompt filtering, so a second caller cannot bypass them.

<a id="workspace-containment"></a>
## Workspace containment

Every file path a tool receives is resolved against the agent workspace and refused when it escapes (`resolveInsideWorkspace`). The check handles `..`, absolute paths outside the root, and separators on both platforms. Agents cannot read or write outside their own worktree, so a project or a session cannot leak another project's content through the filesystem.

<a id="destructive-commands"></a>
## Destructive commands

The shell tool refuses command text matching a destructive pattern: `git reset --hard`, `git clean -f`, `git push --force`, removal of a root or home directory, and piping a fetched document into a shell. Each refusal names the rule. A task needing a genuinely destructive operation must carry an explicit recorded decision; the platform does not silently widen the shell.

<a id="untrusted-content"></a>
## Untrusted content

Repository content, agent messages and retrieved documents are data. The agent runtime scans the workspace it is about to use for instruction-shaped markers (`ignore previous instructions`, role reassignment, secret exfiltration requests) and adds a framed message to the assignment: the markers found, and the rule that retrieved text is information, not an order.

`frameUntrusted(source, content)` wraps the text in `<<UNTRUSTED CONTENT>>` markers with a header stating that it is not an instruction. Instructions inside that frame never override the profile contract, the project instructions or the permission check.

<a id="secrets"></a>
## Secrets

NC-AG never stores a credential. Model credentials belong to the harness credentials service and are resolved by the harness when it opens the provider stream. The audit detail contains actions, identifiers, token counts and outcomes, never secret values. The agent workspace is the agent's own worktree, so a credential file the operator keeps elsewhere is not readable through the platform.

<a id="audit-integrity"></a>
## Audit integrity

Every audited action appends one record to the hash chain. The record's hash covers its content and the previous record's hash; `verify()` recomputes the whole chain and reports the first sequence whose stored hash does not match. A modified record, a deleted record or an inserted record breaks verification from that point on. `GET /api/health` reports the state, and the orchestrator exposes `store.verifyAudit()`.

<a id="the-control-plane"></a>
## The control plane

The control plane binds `127.0.0.1` by default. When `NCAG_TOKEN` is set, every API call requires `Authorization: Bearer` with that token, and the event stream accepts the token as a query parameter because a browser `EventSource` cannot set a header. Without a token the API is open, which is why the default bind is loopback only; a deployment reachable from another machine sets `NCAG_TOKEN`.

The dashboard renders only data returned by the API and holds no hidden state; the token is read from the page URL and never persisted in browser storage.
