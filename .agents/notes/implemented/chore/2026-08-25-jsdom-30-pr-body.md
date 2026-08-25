# chore(deps): bump jsdom 29→30 and @types/jsdom 28→30

## Summary

One-major version drift in the DOM-emulation dependency. Vitest 4.1.11 declares jsdom as a wildcard peer, so 30.x is supported without any other change.

## Changes

| Package | From | To |
| --- | --- | --- |
| jsdom | 29.1.1 (exact pin) | 30.0.1 (caret `^30.0.0`) |
| @types/jsdom | 28.0.3 | 30.0.0 |

Playwright is already at 1.62.1 in the lockfile (resolved during the earlier chore-bump pass via the caret floor in `apps/web/package.json`), so no further action is needed.

## Validation

Full `vitest run` on a clean state with both bumps applied:

- **13,658 passed / 33 failed / 77 skipped** (out of 13,768)
- The 33 failures are all pre-existing Windows-environment limitations (EPERM symlink without Developer Mode, `CreateProcessAsUserW` ACL-runner failures, `@adobe/react-spectrum` CSS import, timeouts, shiki vm artifact) — same shape as the 52 stable env failures recorded on the post-migration baseline.
- No jsdom 30 behavior-change regressions: the test files that surfaced as failing in cascade ran cleanly in isolation.

## Follow-up

- PR3: Vite 6 → 8 + React 18 → 19 + @vitejs/plugin-react 6.
- PR4: TypeScript 6 → 7 (full revalidation of tsc, doc-typecheck, verify-type-equiv).
- Adopt ruff + mypy in `python/sdk`.
