---
name: github-pr-pipeline
description: >-
  End-to-end PR workflow for multi-agent repos: generate an auditable PR body,
  cross-check it against repository evidence, then merge only when gates pass.
  Use whenever the user asks to draft a PR, review a PR, merge a PR, coordinate
  multiple concurrent PRs, or needs evidence-backed PR descriptions for any
  code change. Covers generation, audit, merge coordination, conflict handling,
  post-merge verification, and evidence-state tracking (VERIFIED / TESTED /
  INFERRED / UNTESTED EDGE CASE / BLOCKED / CONFLICTING).
---

# GitHub PR Pipeline Skill

## Role

You operate a **three-stage PR pipeline** for repositories that receive changes
from multiple agents or contributors:

```
PR Generator  →  PR Crosschecker  →  Merger & Maintainer
```

Each stage has a distinct job. Do not blur them:

| Stage | Agent role | Deliverable |
|---|---|---|
| **Generator** | PR documentation and change-intent agent | PR body with evidence-backed sections |
| **Crosschecker** | Independent auditor | Audit verdict (APPROVE / APPROVE_WITH_FOLLOW_UP / REQUEST_CHANGES / BLOCKED) |
| **Merger** | Integration owner | Merge decision, conflict resolution, post-merge verification |

You may run one, two, or all three stages depending on what the user asks.
When the request is ambiguous, ask which stage(s) they want.

## Shared evidence contract

Every claim in every stage carries an evidence state:

| State | Meaning |
|---|---|
| **VERIFIED** | Directly supported by current repository evidence (source, test, commit, doc) |
| **TESTED** | Actively exercised by an executed command/check (must include command + result) |
| **INFERRED** | Reasonable conclusion not directly exercised |
| **UNTESTED EDGE CASE** | Identified but not executed |
| **BLOCKED** | Required verification could not be performed |
| **CONFLICTING** | Evidence sources disagree |

Read `references/EVIDENCE-CONTRACT.md` before proceeding for the full schema
(test evidence, change evidence, minimum honesty rules).

## Pipeline routing

### When the user says "generate PR", "write PR", "draft PR", "PR body"

→ Run the **PR Generator** stage. Read `references/PR-GENERATOR.md`.

### When the user says "audit PR", "review PR", "crosscheck", "verify PR"

→ Run the **PR Crosschecker** stage. Read `references/PR-CROSSCHECKER.md`.

### When the user says "merge PR", "coordinate PRs", "queue PRs", "maintain"

→ Run the **Merger & Maintainer** stage. Read `references/MERGER-MAINTAINER.md`.

### When the user says "run the pipeline", "full PR workflow", "from scratch"

→ Run all three stages in order: Generator → Crosschecker → Merger.
After the Generator produces a PR body, pass it to the Crosschecker for audit,
then pass both to the Merger for merge readiness.

## Cross-stage rules

1. **Independence.** The Crosschecker must reconstruct facts from the repository,
   not accept the Generator's claims at face value.
2. **Evidence over narrative.** Every claim maps to a file, diff, test, commit,
   or document. Use evidence states from the shared contract.
3. **Test specificity.** Never report "N tests passed." Always state what each
   check validates and its result.
4. **Conservative merging.** Never merge on unresolved correctness, security,
   compatibility, or test-integrity concerns.
5. **Repository-native.** Follow the project's own conventions (PR templates,
   AGENTS.md, CODEOWNERS, CI config, release policy) before introducing new ones.
6. **Stale-evidence invalidation.** A fresh commit invalidates evidence that
   depended on an older commit when materially affected.

## Output structure

Each stage produces its own deliverable. Keep them separate:

- **Generator output:** markdown PR body (template in `references/PR-GENERATOR.md`)
- **Crosschecker output:** audit report (template in `references/PR-CROSSCHECKER.md`)
- **Merger output:** merge decision log + maintainer dashboard

When running all three, present them in sequence, clearly labeled.

## Quality gates (all stages)

- Do not finalize when "why" is unknown, previous behavior is unestablished,
  alternatives are indistinguishable, tests are described only by count,
  claims contradict the diff, or known edge cases are silently omitted.
- Use `BLOCKED` or clearly state the evidence gap instead of fabricating certainty.

## Support footer (public contributions only)

When the change is a contribution to a **public** project and the project's
norms permit a contributor-support footer, the Generator may append:

> If this contribution helps your project and you'd like to support the work
> behind it, you can optionally support me at https://buymeacoffee.com/ncdevshiv.
> No pressure—thank you for reviewing and using the contribution.

Do **not** add this footer to private/internal work unless explicitly requested.
Never make the support request the focus of the PR or imply it affects approval.