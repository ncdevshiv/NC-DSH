# Agent Note: A whole-section provider resets; it never deletes

Status: implemented

English | [中文](2026-08-23-whole-section-provider-reset-framing.zh.md)

## Problem

The Models page offered **Delete** on every configured row, but for a whole-section provider — `deepseek-official`, whose configurable-provider entry carries an empty settings path — no write can remove the row. Its profile is its namespace itself: the namespace resolves through schema defaults even with an empty user layer, the store marks such a row configured whenever the namespace exists, and `dsh-llm-deepseek` registers its adapter route and directory entry unconditionally ([web configuration plane](../architecture/2026-07-30-web-config-plane.md)). Deleting cleared the user section and the page-managed key correctly; the write was never the defect.

The framing was. A pristine profile rendered the restore-defaults description, but any user-layer override flipped `removable` to true, so the dialog claimed *"Deleting {provider} removes its configuration"* while the row was structurally guaranteed to remain after the operation succeeded. A user who deleted DeepSeek to stop seeing it found the row (or, with no other usable provider, the first-run setup card) still on the page — the reported "remove does not remove" defect.

## Decision

- `restoresBase` now covers every whole-section target regardless of authorship: `settingsPath.length === 0 || row.removable !== true`. Authorship continues to decide path-addressed targets — a user-authored pi-ai profile deletes and its row genuinely leaves the list; a base-matching one resets.
- The destructive control and its dialog select between two copy families keyed on `restoresBase`: Delete (`deleteTitle`, `deleteDescription*`, `deleteConfirm`, `deleting`) versus Reset (`resetTitle`, `resetRestoresBase*`, `resetConfirm`, `resetting`). The writes are unchanged: the same idempotent unsets in the same order, credential before profile ([provider credential lifecycle](../bug-fix/2026-08-06-provider-credential-lifecycle.md)).
- The now-unreachable `deleteRestoresBase*` keys are deleted from both dictionaries, and both READMEs state that a whole-section row cannot be removed and its action is Reset.

## Alternatives considered

**Real removal: a user-layer tombstone gating the adapter registration and directory entry, threaded through `ConfigurableProviderView`, with a cascade policy for dependents.** Rejected for now. It overturns recorded decisions (the route stays registered with no user-layer profile; the directory declaration is composition-owned), needs re-add UX once the row can vanish, and drags in unresolved dependents — the composed default model selection and the DeepSeek search provider both name `deepseek-official` and would keep targeting a removed route. That is a product feature requiring its own design, not a wording repair.

**Hide the action for whole-section rows instead of renaming it.** Rejected: resetting overrides and the stored key is a real need, and hiding the control would push users to hand-editing `settings.yaml` while keeping the misleading claim reachable nowhere.

**Keep the Delete label and correct only the description text.** Rejected: the button and title are the first-read frame, and the customized-profile case would still read as removal.

## Consequences

- Every whole-section built-in renders Reset whatever its profile state; pi-ai deletion behavior and copy are untouched.
- Environment-sourced credentials remain outside the write (pre-existing, documented): Reset copy for an unidentified target says the key is managed elsewhere and kept, which stays accurate.
- Component tests pin both whole-section postures — pristine (no credential write, root-path unset) and customized with a stored key (credential unset, root-path unset, pending Resetting label) — alongside the unchanged pi-ai Delete pins.
