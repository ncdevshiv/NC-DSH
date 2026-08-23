#!/usr/bin/env python3
"""Run local js-bindgen client tests through moli."""
from __future__ import annotations

import argparse
import os
import subprocess
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_JS_BINDGEN = REPO_ROOT.parent / "js-bindgen"
DEFAULT_TARGET = "wasm64-unknown-unknown"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run ../js-bindgen/client tests with moli as the WebDriver runner.",
    )
    parser.add_argument(
        "--js-bindgen",
        type=Path,
        default=DEFAULT_JS_BINDGEN,
        help=f"js-bindgen checkout path; default: {DEFAULT_JS_BINDGEN}",
    )
    parser.add_argument(
        "--moli",
        type=Path,
        default=REPO_ROOT / "target" / "release" / "moli",
        help="moli binary to pass as JBG_TEST_MOLI_DRIVER_PATH",
    )
    parser.add_argument(
        "--build-moli",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="build release moli before running tests when the binary is missing",
    )
    parser.add_argument(
        "--toolchain",
        default="nightly",
        help="Rust toolchain for the js-bindgen client test command",
    )
    parser.add_argument(
        "--target",
        default=DEFAULT_TARGET,
        help=f"Rust target for js-bindgen tests; default: {DEFAULT_TARGET}",
    )
    parser.add_argument(
        "--build-std",
        default="panic_abort,std",
        help="value for -Zbuild-std; use an empty string to omit it",
    )
    parser.add_argument(
        "--no-workspace",
        action="store_true",
        help="do not pass --workspace to cargo test",
    )
    parser.add_argument(
        "cargo_args",
        nargs=argparse.REMAINDER,
        help="extra args appended after cargo test; prefix with --",
    )
    return parser.parse_args()


def run(command: list[str], *, cwd: Path, env: dict[str, str] | None = None) -> None:
    print("+", " ".join(command), f"(cwd={cwd})", flush=True)
    subprocess.run(command, cwd=cwd, env=env, check=True)


def ensure_moli(binary: Path, build: bool) -> Path:
    binary = binary if binary.is_absolute() else REPO_ROOT / binary
    if binary.exists():
        return binary.resolve()

    if not build:
        raise SystemExit(f"moli binary not found: {binary}")

    run(["cargo", "build", "--release", "-p", "moli"], cwd=REPO_ROOT)
    if not binary.exists():
        raise SystemExit(f"moli build finished but binary is missing: {binary}")
    return binary.resolve()


def main() -> int:
    args = parse_args()
    js_bindgen = args.js_bindgen.resolve()
    client_dir = js_bindgen / "client"
    if not (client_dir / "Cargo.toml").is_file():
        raise SystemExit(f"js-bindgen client workspace not found: {client_dir}")

    moli = ensure_moli(args.moli, args.build_moli)
    command = [
        "cargo",
        f"+{args.toolchain}",
        "test",
        "--target",
        args.target,
    ]
    if args.build_std:
        command.append(f"-Zbuild-std={args.build_std}")
    if not args.no_workspace:
        command.append("--workspace")

    extra = args.cargo_args
    if extra[:1] == ["--"]:
        extra = extra[1:]
    command.extend(extra)

    env = os.environ.copy()
    env["CI"] = "true"
    env["JBG_DEV"] = "1"
    # This environment variable is owned by the external js-bindgen test harness.
    env["JBG_TEST_MOLI_DRIVER_PATH"] = str(moli)
    run(command, cwd=client_dir, env=env)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
