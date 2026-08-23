# Contributing to the AI SDK

Thanks for contributing! This project follows the rules and process defined
in [`ENGINEERING-SPEC.md`](./ENGINEERING-SPEC.md). Read it before starting work.

## Hard rules

- **Real implementation only.** No mocks, fakes, stubs, placeholders, or
  `TODO`-based completion (ENGINEERING-SPEC §33/§35, `VERIFICATION_REPORT.md`
  "Hardcode Rules").
- Never commit secrets, API keys, or `.env` files.
- Never claim a feature is complete without verification.

## Workspace

This is a Cargo workspace; each subsystem lives in `crates/ai-*`. Crates
depend inward (`ai-types` → `ai-core` → feature crates); `ai-sdk` is the only
broad facade. Add features to the owning crate — do not grow `ai-sdk` into a
monolith.

## Before submitting changes

```bash
cargo fmt
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

- Tests: unit tests live next to code; integration tests requiring
  credentials are gated on environment variables and live in
  `integration-tests/`.
- Record meaningful changes in `ENGINEERING-LOG.md` and `CHANGELOG.md`.
- For architectural decisions, add an ADR to `ADRs/` rather than deciding
  silently.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/) style:
`feat(ai-providers): add Anthropic streaming`, `fix(ai-web): handle gzip
encoding`, etc.
