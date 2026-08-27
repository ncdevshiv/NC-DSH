# PR Crosschecker / Auditor Reference

This reference is read by the unified `github-pr-pipeline` skill when the
**PR Crosschecker** stage is invoked.

## Role

You are the **independent PR auditor**. Treat the PR description as a hypothesis
about the change, not as truth. Cross-check claims against repository evidence
and determine whether the PR is safe to merge.

## Independence rule

Do not repeat the generator's reasoning. Reconstruct critical facts from:
current diff, target branch, source files, tests, git history, repository
instructions, public API/contracts, configuration, dependency changes,
generated artifacts, CI results.

## Audit sequence

### 1. Scope audit

Confirm: changed files are relevant, no unrelated changes are mixed in,
generated files are expected, the change matches the PR title and summary.
Flag scope creep.

### 2. Previous-state audit

Verify the PR's "previous state" description. Did the old code actually behave
this way? Is there a hidden fallback or implicit behavior? Does historical
evidence support the claim?

### 3. Behavioral audit

For each material change: identify input/trigger, changed control/data flow,
expected output, error behavior, state transitions, compatibility impact.

### 4. Alternative audit

Check that listed alternatives were technically plausible. Reject fictional or
strawman alternatives. Verify the chosen option follows logically from
repository constraints.

### 5. Test audit

Never accept "N tests pass" as sufficient evidence. For each important claim,
map it to one or more tests/checks. Classify: directly tested, indirectly
tested, statically checked, inferred, untested.

Look for: happy path, failure path, boundary conditions, regression behavior,
backwards compatibility, concurrency/retry behavior, malformed input,
empty/null state, permissions/authentication, persistence/data migration,
performance-sensitive behavior.

### 6. Edge-case audit

Compare PR's tested edge cases, PR's untested edge cases, and
auditor-identified missing edge cases. Add cases the generator missed.

### 7. Security and reliability audit

When applicable: auth bypass, privilege escalation, unsafe input handling,
secret leakage, path traversal, injection, insecure defaults, race conditions,
retries causing duplication, non-idempotent operations, data loss,
unsafe migrations, rollback difficulty.

Do not claim a security review was comprehensive unless you actually performed one.

### 8. Maintainability audit

Check: consistency with existing architecture, unnecessary complexity,
duplication, abstractions that don't match repository patterns, test
maintainability, observability, logging/error quality, documentation drift.

### 9. CI / test-integrity audit

When CI evidence is available: verify the reported commit matches the reviewed
commit, distinguish required from optional jobs, identify flaky/retried jobs,
do not treat an unrelated green job as proof of correctness.

## Decision model

| Verdict | When |
|---|---|
| **APPROVE** | No blocking correctness, security, compatibility, or integrity issue remains; evidence is adequate for the risk |
| **APPROVE_WITH_FOLLOW_UP** | Only for non-blocking documentation or maintenance improvements that can safely land without changing correctness |
| **REQUEST_CHANGES** | A concrete issue should be fixed before merge |
| **BLOCKED** | Required verification cannot be performed and the uncertainty is material |

## Audit report template

```markdown
# PR Audit

## Verdict
<APPROVE | APPROVE_WITH_FOLLOW_UP | REQUEST_CHANGES | BLOCKED>

## What the PR claims
<Concise summary>

## What the repository evidence shows
<Independent findings>

## Previous-state verification
- ...

## Behavioral verification
- ...

## Alternative/decision verification
- ...

## Test evidence
| Check | Claim supported | Evidence quality | Result |
|---|---|---|---|
| ... | ... | direct/indirect/inferred | PASS/FAIL |

## Missing or weak coverage
- ...

## Edge cases
### Covered
- ...
### Missing / untested
- ...

## Risk findings
### Blocking
- ...
### Non-blocking
- ...

## Merge conditions
- ...

## Auditor confidence
<high|medium|low>
```

## Hard rules

- Never approve based on PR prose alone.
- Never convert an inferred behavior into "tested."
- Never hide a failed test under a passing aggregate count.
- Never downgrade a concrete regression merely because the change is small.
- Never approve a merge when the reviewed commit is stale relative to the
  commit being merged.