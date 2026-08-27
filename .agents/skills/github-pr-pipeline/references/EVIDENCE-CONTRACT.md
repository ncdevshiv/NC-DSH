# Shared Evidence Contract

All three skills use this contract.

## Claim levels

### VERIFIED
A direct repository artifact supports the claim.

Examples:
- source code implements the described behavior,
- a test demonstrates the behavior,
- git history shows the prior behavior.

### TESTED
The agent actually executed the check and observed the result.

`TESTED` must include the command/check and result.

### INFERRED
The claim is derived from implementation or related evidence but was not directly exercised.

### UNTESTED EDGE CASE
The agent identified a meaningful scenario but did not execute it.

### BLOCKED
A required verification could not be performed.

### CONFLICTING
Evidence sources disagree.

## Test evidence schema

```yaml
test:
  name: "<test/check name>"
  command: "<command when available>"
  scenario: "<what it exercises>"
  expected: "<expected result>"
  actual: "<observed result>"
  status: PASS|FAIL|SKIPPED|BLOCKED
  evidence_level: TESTED
```

## Change evidence schema

```yaml
change:
  area: "<module / behavior>"
  previous_state:
    statement: "<old behavior>"
    evidence: ["<file/test/commit reference>"]
  new_state:
    statement: "<new behavior>"
    evidence: ["<file/test reference>"]
  rationale: "<why>"
  alternatives:
    - name: "<option>"
      pros: ["..."]
      cons: ["..."]
      decision: chosen|rejected
  expected_behavior:
    - scenario: "<scenario>"
      result: "<expected outcome>"
  edge_cases:
    tested: ["..."]
    untested: ["..."]
```

## Minimum honesty rules

1. A passing test proves the scenario it exercises, not every similar scenario.
2. Type-checking is not runtime testing.
3. Lint success is not behavioral correctness.
4. Coverage percentage is not edge-case coverage.
5. A diff review is not a substitute for execution when runtime behavior is materially changed.
6. A previous test remaining green does not prove new behavior unless it exercises the new behavior.
