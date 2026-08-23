from __future__ import annotations

import hashlib
import os
import shutil
from pathlib import Path


PROJECT_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = PROJECT_ROOT.parent

PROXY_ENV_KEYS = (
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
)


def clear_proxy_env(env: dict[str, str]) -> dict[str, str]:
    result = dict(env)
    for key in PROXY_ENV_KEYS:
        result.pop(key, None)
    result["NO_PROXY"] = "*"
    result["no_proxy"] = "*"
    return result


def moli_binary(override: str | None = None) -> Path:
    configured = override or os.environ.get("MOLI_BIN")
    if configured:
        return Path(configured).expanduser().resolve()
    candidates = [
        REPO_ROOT / "target" / "debug" / "moli",
        REPO_ROOT / "target" / "release" / "moli",
    ]
    existing = [candidate for candidate in candidates if candidate.is_file()]
    if not existing:
        raise RuntimeError(
            "missing moli binary; run `cargo build -p moli` or set MOLI_BIN"
        )
    return max(existing, key=lambda candidate: candidate.stat().st_mtime)


def chromium_binary(override: str | None = None) -> Path:
    configured = override or os.environ.get("CHROMIUM_BIN")
    if configured:
        path = Path(configured).expanduser().resolve()
        if not path.is_file():
            raise RuntimeError(f"Chromium binary does not exist: {path}")
        return path
    for name in ("chromium", "chromium-browser", "google-chrome", "google-chrome-stable"):
        resolved = shutil.which(name)
        if resolved:
            return Path(resolved).resolve()
    raise RuntimeError("missing Chromium binary; pass --chromium-bin or set CHROMIUM_BIN")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_fixture_tree(root: Path) -> str:
    if not root.is_dir():
        raise RuntimeError(f"fixture dist directory does not exist: {root}")
    digest = hashlib.sha256()
    files = sorted(
        (
            path
            for path in root.rglob("*")
            if path.is_file()
            and path.relative_to(root).as_posix() not in {"manifest.json", "metafile.json"}
        ),
        key=lambda path: path.relative_to(root).as_posix(),
    )
    for path in files:
        relative = path.relative_to(root).as_posix()
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        with path.open("rb") as handle:
            while chunk := handle.read(1024 * 1024):
                digest.update(chunk)
        digest.update(b"\0")
    return digest.hexdigest()
