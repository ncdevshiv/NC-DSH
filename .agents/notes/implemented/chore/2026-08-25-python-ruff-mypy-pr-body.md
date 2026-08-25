# feat(python): adopt ruff and mypy for the SDK and runtime carrier

## Summary

Brings the Python subprojects to parity with the TypeScript side (100% coverage gate, oxlint type-aware linting, dozens of verify scripts) for lint and type analysis. Ruff is the Rust-core linter and formatter; mypy handles strict static typing on the public surface. Both follow the `[tool.ruff]` and `[tool.mypy]` sections inside each subproject's `pyproject.toml`, so configuration travels with the package rather than living in a separate config file.

## Tooling

- `python/sdk` `[dependency-groups].lint`: `ruff>=0.6.0`, `mypy>=1.11.0`, `types-deprecated`.
- `python/sdk-runtime` `[dependency-groups].lint`: `ruff>=0.6.0` (no source code to type-check; only config glue).
- Ruff rule selection: `E/F/W` (pycodestyle/pyflakes), `I` (isort), `B` (bugbear), `UP` (pyupgrade), `SIM` (simplify). Line length 100, target-version `py310`, double-quote strings.
- mypy: strict on the public surface (`api.py`, `models.py`), pydantic plugin for model validation, lenient on `client.py` (transport + subprocess + threading shape) and the test tree, where static types fight the runtime model.

## Root scripts

- `bun run python:sync` — `uv sync` both subprojects with `test+lint` groups.
- `bun run python:lint` — `ruff check` + `mypy src` on the SDK; `ruff check` on the runtime carrier.
- `bun run python:format` — `ruff format` on both.
- `python:lint` added to the `hygiene` chain so it runs in CI alongside the TypeScript gates.

## Source fixes (auto-applied by ruff + three hand-fixes)

- 20 import-ordering issues auto-fixed (isort).
- 8 files reformatted.
- 1 leftover unused-variable in `tests/test_client.py:419` removed (`result = harness.run(...)` was dead since the rename to a context manager).
- 1 `try/except/pass` in `client.py:107` replaced with `contextlib.suppress(ProcessLookupError)` (`SIM105`).
- 1 `**kwargs: object` → `**kwargs: Any` and `__exit__` parameters typed in `api.py` to satisfy strict mypy on the public surface.

## Validation

- `bun run python:lint`: **All checks passed**; mypy: no issues found in 5 source files.
- `pytest` in the SDK env: 49 passed, 3 failed (WinError 193 on `dsh-jsonrpc-agent-*` — pre-existing missing-bundled-binary issue, not introduced by this change), 10 skipped.

## Docs

- `python/development.md`: new 'Lint and type-check' section between 'Validate the SDK' and 'Run against Node source' documenting the rationale, the strict/lenient split, and the `bun run` commands.

## Follow-up

- PR4: TypeScript 6 → 7 (full revalidation of tsc, doc-typecheck, verify-type-equiv).
- Document the oxfmt decision.
