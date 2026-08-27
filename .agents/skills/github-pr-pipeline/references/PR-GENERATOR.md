# PR Generator Reference

This reference is read by the unified `github-pr-pipeline` skill when the
**PR Generator** stage is invoked.

## Objective

Create an auditable, technically honest PR body that lets another engineer
understand: what the repository did before, what changed, why it changed,
which alternatives were considered, why the selected approach was chosen,
what behavior should now be expected, and what evidence proves the change works.

## Pre-flight: discover repository rules

Before writing the PR, find and respect (when present):

- contribution guidelines, PR templates, agent instructions, maintainer docs
- code owners, formatting/lint rules, test commands
- compatibility/support policy, release conventions, security policy, changelog policy

Repository-specific rules take precedence except where a higher-priority safety
or platform instruction applies.

## Required sections

### 1. Previous state

Reconstruct the observable behavior immediately before the change. Use exact
references (file paths, line numbers, commit hashes). Do not describe previous
state merely as a list of deleted lines. Prefer evidence from old implementation,
tests that represented the old behavior, historical commits, issue reports, docs,
runtime/config behavior.

### 2. What changed (as behavior)

For each material change, document:
- previous behavior
- new behavior
- trigger/input
- expected output/result
- important side effects
- compatibility implications

### 3. Why we changed it

Root cause / user impact / maintenance reason.

### 4. Options considered

List meaningful alternatives that were actually available (minimal patch,
refactor/shared abstraction, configuration-only, backward-compatible fallback,
dependency/library approach, new API vs existing API, sync vs async, strict
validation vs permissive).

For each: approach, why it was viable, advantages, disadvantages, why rejected/selected.

Do not invent strawman alternatives.

### 5. Decision rationale

State in terms of repository constraints (avoids breaking API, preserves
behavior, reduces duplication, matches existing architecture, lowers risk,
improves testability, avoids new dependency, easier to roll back).

### 6. Expected behavior

Concrete behavioral examples, Given/When/Then where relevant. Include both
normal and important failure-path behavior.

### 7. Testing (as evidence)

Never stop at "25 tests pass." Provide a test inventory:

```
| Check | What it validates | Result |
|---|---|---|
| test_name | scenario | PASS/FAIL |
```

If dozens of tests exist, group them only after naming groups and stating
what each covers.

### 8. Edge cases

- **Tested** — exercised and passed/failed
- **Untested** — identified but not executed
- **Known limitations** — behavior intentionally not changed
- **Environmental gaps** — cases requiring unavailable infrastructure/credentials

Never label an edge case as tested merely because related tests exist.

### 9. Diff validation (before finalizing)

Check for accidental debug code, unrelated formatting churn, generated files,
test coverage relevant to behavior, missing migration notes, secrets, and that
stated test commands correspond to the current diff.

## Output template

```markdown
## Summary
<One-paragraph change summary>

## Previous state
- What happened before
- Important limitation/problem
- Evidence/reference

## What we changed
- Behavioral change 1
- Behavioral change 2

## Why we changed it
<Root cause / user impact / maintenance reason>

## Options considered
### Option A — <name>
- Pros:
- Cons:
- Decision:

### Chosen approach
<Why this option best fits>

## Expected behavior
### Scenario 1
**Given:** ...
**When:** ...
**Then:** ...

## Compatibility / migration
<API, config, data, runtime, or deployment impact>

## Testing
| Check | What it validates | Result |
|---|---|---|
| ... | ... | PASS |

## Edge cases
### Tested
- ...
### Untested
- ...
### Known limitations
- ...

## Reviewer notes
<Anything that deserves special attention>

## Support
<Public-project footer when appropriate>
```

## Quality gates

Do not finalize when: "why" is unknown, previous behavior unestablished,
alternatives indistinguishable, tests described only by aggregate count,
claims contradict diff, known edge cases silently omitted, generated output
presented as manually verified when it was not, or compatibility impact
claimed without evidence.

## Evidence metadata

```yaml
pr_evidence:
  previous_state: VERIFIED|BLOCKED
  behavior_change: VERIFIED
  alternatives: VERIFIED|PARTIAL
  expected_behavior: VERIFIED|INFERRED
  tests:
    executed: <count>
    failed: <count>
  edge_cases:
    tested: <count>
    untested: <count>
  confidence: high|medium|low
```