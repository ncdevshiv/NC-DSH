# Agent Note: jsdom 30 under Vitest's wildcard peer declaration

Status: implemented

English | [中文](2026-08-25-jsdom-30-under-vitest-wildcard-peer.zh.md)

## Problem

jsdom was exact-pinned at 29.1.1 while Vitest 4.1.11 declares jsdom as a wildcard peer, so the DOM-emulation dependency trailed a major behind what the test runner already accepts, and `@types/jsdom` lagged further at 28.

## Decision

jsdom moves from the 29.1.1 exact pin to caret `^30.0.0` (resolved 30.0.1), and `@types/jsdom` from 28.0.3 to 30.0.0. Because Vitest 4.1.11 declares jsdom as a wildcard peer, 30.x is supported without any other package change. Playwright already resolves to 1.62.1 in the lockfile via the caret floor in `apps/web/package.json` from the earlier [toolchain bump wave](../process/2026-08-25-toolchain-floor-bump-wave.md), so no further action is needed there. This sits beside [Vitest's ownership of the jsdom/WebStorage arrangement](2026-07-30-vitest-jsdom-webstorage-ownership.md): that note owns who provides the DOM environment; this one only moves its major.

## Verification

A full `vitest run` on a clean state with both bumps applied: **13,658 passed / 33 failed / 77 skipped**, out of 13,768 tests. All 33 failures are pre-existing Windows-environment limitations — EPERM symlinks without Developer Mode, `CreateProcessAsUserW` ACL-runner failures, one `@adobe/react-spectrum` CSS import, timeouts, and a shiki vm artifact — the same shape as the 52 stable-environment failures recorded on the post-migration baseline. Test files that had surfaced as failing in cascade run cleanly in isolation, so none of the failures is a jsdom 30 behavior-change regression.

## Alternatives considered

**Stay on the jsdom 29 exact pin.** Rejected: an exact pin buys nothing against a wildcard-peer consumer — Vitest accepts any major — and the pin silently accumulates major-version debt.

**Bump jsdom without `@types/jsdom`.** Rejected: the 28-era types describe an older API; leaving types behind the runtime invites false type errors in test code for no isolation benefit.

## Consequences

DOM emulation runs on jsdom 30 with matching 30-era types under the same Vitest-managed environment contract. The Windows-environment failure set keeps its baseline shape, so future full-suite runs compare against 13,658/33/77 instead of treating any of the 33 as a new regression.
