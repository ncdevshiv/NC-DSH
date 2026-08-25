# docs(development): record the decision to hold for oxfmt

## Summary

Adds a short "Code style and formatting" section to `docs/development.md` and the matching Chinese translation in `development.zh.md`, documenting the explicit decision to stay formatter-less and wait for oxfmt to stabilize.

## Why

This is the last open item from the upgradation plan. The repository does not currently run a formatter in CI or the lefthook chain — style is enforced by convention plus the `@stylistic` ruleset inside `oxlint`. The decision to keep it that way (vs. adopting Biome 2 or Prettier now) needed to be captured as an in-repo fact, not left implicit.

The three real options are spelled out:

- **Stay as-is.** Cheapest, defensible while the team is small.
- **Adopt Biome 2.** Rust-native, fast, opinionated, but a separate vendor from the rest of the toolchain.
- **Adopt oxfmt.** The oxc project's own formatter; still maturing, but the natural endgame because it completes the oxlint/oxfmt/oxc suite the rest of the toolchain already runs on.

The decision is to **hold for oxfmt stabilization**, then adopt it. It is the only option that does not introduce a second lint/format vendor alongside `oxlint`/`oxlint-tsgolint`, and the cross-package cost of maintaining two style policies is the only argument that would justify a one-off large formatting commit.

## Files changed

- `docs/development.md` — new "Code style and formatting" section between the Git integrations and CI gates sections.
- `docs/development.zh.md` — matching Chinese translation.
- `docs/development.i18n.yaml` — bilingual pairing seal regenerated via `bun run verify-translation-pairing docs/development.md --write`.

30 lines added, 2 lines changed in the seal. Docs-only, no code or config touched.
