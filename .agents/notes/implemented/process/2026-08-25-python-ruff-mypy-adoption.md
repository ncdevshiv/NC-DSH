# Agent Note: ruff and mypy lint the Python SDK and runtime carrier

Status: implemented

English | [中文](2026-08-25-python-ruff-mypy-adoption.zh.md)

## Problem

The Python subprojects had no static lint or type analysis, while the TypeScript side runs type-aware linting plus dozens of verify gates. Python defects surfaced only at pytest time, and formatting discipline did not exist.

## Decision

Ruff is the linter and formatter, and mypy the static typer, for `python/sdk` and `python/sdk-runtime`. Configuration travels with each subproject: the `[tool.ruff]` and `[tool.mypy]` sections live in that subproject's own `pyproject.toml`, not in a separate shared config file.

Rule selection is `E/F/W` (pycodestyle/pyflakes), `I` (isort), `B` (bugbear), `UP` (pyupgrade), and `SIM` (simplify), with line length 100, target-version `py310`, and double-quote strings. mypy runs strict on the public surface (`api.py`, `models.py`) with the pydantic plugin for model validation, and lenient on `client.py` — whose transport, subprocess, and threading shape fights static typing — and on the test tree.

Lint dependencies ride the standard groups: `python/sdk`'s `[dependency-groups].lint` holds `ruff>=0.6.0`, `mypy>=1.11.0`, and `types-deprecated`; `python/sdk-runtime` carries only `ruff>=0.6.0`, being config glue with no source to type-check.

Root scripts expose the workflow: `bun run python:sync` (`uv sync` of both subprojects with the `test+lint` groups), `bun run python:lint` (`ruff check` + `mypy src` on the SDK, `ruff check` on the runtime carrier), and `bun run python:format` (`ruff format` on both). `python:lint` joined the `hygiene` chain, so CI runs it beside the TypeScript gates.

Adoption applied ruff's automatic fixes plus three hand fixes: 20 import-order corrections across 8 reformatted files; removal of a dead variable at `tests/test_client.py:419`; replacement of a `try/except/pass` at `client.py:107` with `contextlib.suppress(ProcessLookupError)` (`SIM105`); and strict-mypy conformance in `api.py` (`**kwargs: object` became `**kwargs: Any`, with typed `__exit__` parameters). The rationale and commands are documented in `python/development.md` under 'Lint and type-check'.

## Verification

`bun run python:lint` passes: ruff reports all checks passed and mypy finds no issues in 5 source files. The SDK pytest suite on the linted tree: 49 passed, 10 skipped, 3 failed — all three failures are `WinError 193` on `dsh-jsonrpc-agent-*`, the pre-existing missing-bundled-binary issue, not introduced by this change.

## Alternatives considered

**Uniform strict mypy across every module.** Rejected: forcing strict mode onto `client.py`'s transport/subprocess/threading code and the test tree produces noise that buries the public-surface guarantees strict mode exists to protect.

**flake8 + black + isort.** Rejected: three tools and three configs duplicate what ruff provides with one Rust-core binary and one `[tool.ruff]` section per package.

**Leave Python to runtime testing alone.** Rejected: parity with the TypeScript-side gates is the point — defects should surface at lint/typecheck time, not only inside pytest.

## Consequences

Python contributions get lint, format, and strict-public-surface type feedback through the same `hygiene` chain as TypeScript work. The lenient zones (`client.py`, the test tree) are deliberate carve-outs, so new transport-shape code belongs there only with cause. The 3-failure pytest baseline remains owned by the bundled-binary issue, not by linting.
