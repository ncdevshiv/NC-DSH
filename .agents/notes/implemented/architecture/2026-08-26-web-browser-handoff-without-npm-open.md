# Agent Note: `dsh web` browser handoff without npm `open`

Status: implemented

English | [中文](2026-08-26-web-browser-handoff-without-npm-open.zh.md)

## Problem

`packages/bundle/web-app` opened the default browser through the npm `open@11` package running inside a helper Node child. The child existed only to host that dependency: it cost a PowerShell startup per Windows launch (~1.2 s handoff), pulled a 68-file install tree, and its credential isolation was observable only through mocks. The Bun-native `Bun.open` work needed the dependency gone either way.

## Decision

`packages/bundle/web-app` opens the default browser in-process: it calls the built-in `Bun.open` when the runtime provides one, and otherwise spawns the platform opener (`cmd /c start`, `/usr/bin/open`, `xdg-open`) directly with `scrubbedParentEnv()`. The npm `open@11` dependency is gone, along with the helper-child architecture that existed only to run it.

### Why the helper child existed, and why deleting it is safe

The old design spawned a Node child to import `open` because:

1. **Credential isolation** — the child got `scrubbedParentEnv()`, so `DEEPSEEK_API_KEY` never reached the opener. The direct spawn keeps the identical guarantee at the same seam: `browser-opener.ts` passes `scrubbedParentEnv()` explicitly, and `tests/browser-opener.spec.ts` asserts credential-shaped and `DSH_*` names are absent from the spawned environment. Under Bun there is no child process at all, so nothing carries credentials anywhere.
2. **Windows launcher-wait** — npm `open` resolved at PowerShell *spawn*, before the URL reached the shell, so a second process had to outlive that gap. The direct `cmd /c start` route settles on the launcher's own exit; there is no gap to paper over.
3. **Specifier-seam testing** — the assembled snapshot hooked the `open` module specifier. The replacement seam is `internals.openBrowser`, which the fixture swaps directly; the snapshot's evidence lines are unchanged.

### The two contracts that moved

- **Where credentials must not appear** shifted from "the opener child's environment" (observable only through a mock running inside a scrubbed child) to "the opener spawn options" — asserted precisely by unit test on the real builder/spawn seam. The snapshot record dropped `apiKeyPresent`/`dshHomePresent`; those fields could only ever observe the mock, not the OS opener.
- **Snapshot lifetime** — `dsh web` has no self-shutdown; the old happy-path snapshot recorded `exitCode: 0` only because its recording run terminated externally within the timeout window. Test 1 now uses the same bounded `DSH_BROWSER_OPEN_TEST_EXIT_ON_READY=1` mechanism as its SSH sibling, making the assembled verification deterministic instead of lucky.

### Settle on `exit`, not `close`

The opener wait resolves on the launcher's `exit` event. `close` requires the stdio pipes to drain, and Windows' `start` can leave those pipes owned by the process it dispatched (a console-hosted target inherits them), so pipe drain has no time bound while process lifetime does. Stderr accumulated up to exit is still the failure reason.

## Alternatives considered

**Wait for upstream `Bun.open` alone.** Rejected: dsh runs on Node today, so the page still needs a launcher there; keeping npm `open` for that case would preserve the very startup cost and dependency surface this change deletes.

**Shell out through a cross-platform helper (e.g., a bundled script) instead of direct platform commands.** Rejected: it re-creates a child whose only job is compatibility npm already provided badly, while the three platform openers are one argv each.

## Consequences

- Handoff end-to-end (call → opener settled): **1236.9 ms → 79.1 ms mean (15.6×)** on Windows, Node 24; npm `open` paid a PowerShell startup per launch.
- Per-handoff module evaluation of the `open` graph (**32.8 ms**) deleted, along with 68 files / ~132 KB of runtime dependencies from the install tree.
- Credential isolation is asserted at the real spawn seam rather than observed through a child-hosted mock, and the happy-path snapshot's success no longer depends on an external kill landing inside its timeout window.
