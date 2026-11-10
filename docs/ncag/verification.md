---
description: "How NC-AG decides a task is complete: the evidence it requires, where it is stored, and how to reproduce the verification."
kind: "reference"
---

# NC-AG verification

English | [中文](verification.zh.md)

## Summary

A task completes when its recorded acceptance evidence passes, never because an agent says it succeeded. Evidence is one row per executed check, carrying the criterion, the method, the exact command, the exit code, the captured output and the producing agent. This page states how the command is resolved, what is recorded, and how to reproduce a verification.

## Table of Contents

- [The rule](#the-rule)
- [Where the command comes from](#where-the-command-comes-from)
- [What is recorded](#what-is-recorded)
- [Reviews are separate evidence](#reviews-are-separate-evidence)
- [Reproducing a verification](#reproducing-a-verification)
- [Why a check can fail to run](#why-a-check-can-fail-to-run)

-----

<a id="the-rule"></a>
## The rule

`unverifiedCriteria(repositories, taskId, criteria)` lists the criteria with no passing evidence. The orchestrator moves a task to `VERIFICATION_PENDING`, executes each criterion, then completes the task only when that list is empty. Otherwise the task moves to `FAILED` with the unverified criterion identifiers as the reason. Both outcomes are emitted as a `verification/completed` event with each criterion's method, status and command.

<a id="where-the-command-comes-from"></a>
## Where the command comes from

The command is the criterion's own `command` when planning set one, otherwise the project's configured check for the criterion's verification method. A project stores `checks` as a method-to-command table:

```ts
createProject({
  name: 'checkout',
  description: 'order API',
  repositoryPath: '/abs/path/repo',
  checks: {
    'unit-test': 'npm test',
    'typecheck': 'npx tsc --noEmit',
    'manual-review': 'git diff --check',
  },
})
```

The platform never guesses a command. An absent method means that check cannot be executed in this deployment, and the criterion fails with the reason rather than passing.

<a id="what-is-recorded"></a>
## What is recorded

Each executed check appends one evidence row: task, project, criterion identifier, method, status (`PASSED`, `FAILED`, `ERROR`), the exact command, the exit code, the captured output (the first 8000 characters), the recording instant and the producing agent. Output is clipped, not summarized: the first characters of the real output are kept so a later reader can see what the command actually printed.

<a id="reviews-are-separate-evidence"></a>
## Reviews are separate evidence

Review and verification answer different questions. Review asks whether the delivered evidence shows a defect: no changed files, a failed command, an unverified criterion, an empty completion report, or a finding the delegated reviewer grounded in a named artifact. Verification asks whether the criteria pass now. A review rejection stops the task before verification; a verification failure stops it after.

Every review finding carries severity, description, evidence, artifact, impact, recommended correction, verification method and status. Findings are durable and remain visible after resolution.

<a id="reproducing-a-verification"></a>
## Reproducing a verification

The evidence row names the workspace and the command. To reproduce:

```bash
cd <workspace path recorded for the agent>
<the exact command from the evidence row>
```

Re-running the command in the same workspace reproduces the recorded exit code and output, which is how an auditor checks that the delivery matches its evidence rather than trusting the report.

<a id="why-a-check-can-fail-to-run"></a>
## Why a check can fail to run

A check records `FAILED` with `spawnSync <executable> ENOENT` when the configured shell cannot be found. NC-AG resolves the shell from `NCAG_SHELL`, then `powershell.exe` on Windows and `pwsh` elsewhere; a deployment without that shell must configure `NCAG_SHELL`. The failure is recorded as evidence, not swallowed, because a check that did not run has not passed.
