#!/usr/bin/env python3
"""
OpenMem uv Bootstrap & Dependency Installer.

Auto-installs uv (if missing), then uses it to install all dependencies.
Works on Windows, Linux, macOS. Zero manual setup.

Usage:
    python bin/uv_bootstrap.py              # Bootstrap uv + install deps
    python bin/uv_bootstrap.py --uv-only    # Only install uv, skip deps
    python bin/uv_bootstrap.py --check      # Check if uv is available
"""

import os
import sys
import subprocess
import platform
from pathlib import Path
from datetime import datetime

BIN_DIR = Path(__file__).parent
OPENMEM_ROOT = BIN_DIR.parent
UV_DIR = BIN_DIR / "uv"  # uv binary lives here for portability


def log(msg):
    """Print with timestamp and prefix."""
    print(f"[uv-bootstrap] {msg}")


def check_uv():
    """Check if uv is available."""
    # Check local bin/uv first
    if platform.system().lower() == "windows":
        uv_local = UV_DIR / "uv.exe"
    else:
        uv_local = UV_DIR / "uv"

    if uv_local.exists():
        return str(uv_local)

    # Check system PATH
    try:
        result = subprocess.run(
            ["uv", "--version"],
            capture_output=True, text=True, timeout=10
        )
        if result.returncode == 0:
            return "uv"  # In PATH
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass

    return None


def install_uv():
    """
    Download and install uv binary into bin/uv/.

    Returns path to uv binary or None on failure.
    """
    system = platform.system().lower()
    arch = platform.machine().lower()

    log(f"Installing uv for {system}/{arch}...")

    # Determine download URL
    if system == "windows":
        if arch in ("x86_64", "amd64"):
            filename = "uv-x86_64-pc-windows-msvc.zip"
        elif arch in ("aarch64", "arm64"):
            filename = "uv-aarch64-pc-windows-msvc.zip"
        else:
            filename = "uv-x86_64-pc-windows-msvc.zip"  # Default
        target = UV_DIR / "uv.exe"
        is_zip = True
    elif system == "linux":
        if arch in ("x86_64", "amd64"):
            filename = "uv-x86_64-unknown-linux-musl.tar.gz"
        elif arch in ("aarch64", "arm64"):
            filename = "uv-aarch64-unknown-linux-musl.tar.gz"
        else:
            filename = "uv-x86_64-unknown-linux-musl.tar.gz"
        target = UV_DIR / "uv"
        is_zip = False
    elif system == "darwin":
        if arch in ("x86_64", "amd64"):
            filename = "uv-x86_64-apple-darwin.tar.gz"
        elif arch in ("aarch64", "arm64"):
            filename = "uv-aarch64-apple-darwin.tar.gz"
        else:
            filename = "uv-x86_64-apple-darwin.tar.gz"
        target = UV_DIR / "uv"
        is_zip = False
    else:
        log(f"Unsupported platform: {system}/{arch}")
        return None

    UV_DIR.mkdir(parents=True, exist_ok=True)

    import urllib.request
    import tempfile
    import zipfile
    import tarfile
    import shutil

    # Use latest release via GitHub API or direct URL
    # Using a known recent version for reliability
    uv_version = "0.5.25"
    base_url = f"https://github.com/astral-sh/uv/releases/download/{uv_version}"
    url = f"{base_url}/{filename}"

    log(f"Downloading uv {uv_version} from {url}...")

    try:
        # Download
        archive_path = UV_DIR / filename
        urllib.request.urlretrieve(url, archive_path)

        # Extract
        if is_zip:
            with zipfile.ZipFile(archive_path, "r") as zf:
                # Find the uv binary in the archive
                for name in zf.namelist():
                    if name.endswith("uv.exe") or (not system == "windows" and name.endswith("/uv")):
                        zf.extract(name, UV_DIR)
                        # Move to target if extracted in subdir
                        extracted = UV_DIR / name
                        if extracted != target:
                            shutil.move(str(extracted), str(target))
                            try:
                                extracted.parent.rmdir()
                            except OSError:
                                pass
                        break
        else:
            with tarfile.open(archive_path, "r:gz") as tf:
                tf.extractall(UV_DIR)
                # Move to target if in subdir
                extracted_name = filename.replace(".tar.gz", "")
                extracted_uv = UV_DIR / extracted_name / "uv"
                if extracted_uv.exists():
                    shutil.move(str(extracted_uv), str(target))
                    try:
                        (UV_DIR / extracted_name).rmdir()
                    except OSError:
                        pass

        # Make executable on Unix
        if system != "windows":
            target.chmod(0o755)

        # Cleanup archive
        archive_path.unlink(missing_ok=True)

        if target.exists():
            log(f"uv installed to {target}")
            return str(target)
        else:
            log("uv binary not found after extraction")
            return None

    except Exception as e:
        log(f"uv download failed: {e}")
        return None


def run_uv(args, cwd=None):
    """Run a uv command."""
    uv_path = check_uv()
    if not uv_path:
        log("uv not found, attempting to install...")
        uv_path = install_uv()
        if not uv_path:
            log("ERROR: Could not install uv. Install manually: https://docs.astral.sh/uv/")
            return False

    cmd = [uv_path] + args
    log(f"Running: {' '.join(cmd)}")

    try:
        result = subprocess.run(
            cmd,
            cwd=cwd or str(OPENMEM_ROOT),
            capture_output=True,
            text=True,
            timeout=600,
        )
        if result.stdout:
            for line in result.stdout.strip().split("\n")[-5:]:
                log(f"  {line}")
        if result.returncode != 0 and result.stderr:
            for line in result.stderr.strip().split("\n")[-5:]:
                log(f"  stderr: {line}")
        return result.returncode == 0
    except subprocess.TimeoutExpired:
        log("  uv command timed out (10 min limit)")
        return False
    except Exception as e:
        log(f"  uv command failed: {e}")
        return False


def bootstrap_and_install():
    """
    Full bootstrap: install uv if missing, then install all deps.

    Returns True if successful.
    """
    # Fix Windows console encoding
    if sys.platform == "win32":
        try:
            sys.stdout.reconfigure(encoding="utf-8")
            sys.stderr.reconfigure(encoding="utf-8")
        except AttributeError:
            pass

    log("=" * 60)
    log("OpenMem uv Bootstrap")
    log("=" * 60)

    # Step 1: Ensure uv is available
    uv_path = check_uv()
    if uv_path:
        log(f"uv found: {uv_path}")
    else:
        log("uv not found, installing...")
        uv_path = install_uv()
        if not uv_path:
            log("ERROR: Failed to install uv")
            return False

    # Verify uv works
    try:
        result = subprocess.run(
            [uv_path, "--version"],
            capture_output=True, text=True, timeout=10
        )
        log(f"uv version: {result.stdout.strip()}")
    except Exception as e:
        log(f"uv verification failed: {e}")
        return False

    # Step 2: Install dependencies using uv
    log("")
    log("Installing dependencies with uv...")

    # Create .venv if it doesn't exist (for uv pip mode)
    venv_path = OPENMEM_ROOT / ".venv"
    if not venv_path.exists():
        log("Creating virtual environment...")
        if not run_uv(["venv", str(venv_path)]):
            log("WARNING: Could not create .venv, falling back to uv pip --system")
            # Use system-wide install
            success = run_uv(["pip", "install", "-r", "requirements.txt"])
            if success:
                log("Dependencies installed (system-wide)")
            return success

    # Use the venv
    log(f"Using virtual environment: {venv_path}")

    # Sync dependencies via uv pip
    if sys.platform == "win32":
        venv_python = venv_path / "Scripts" / "python.exe"
    else:
        venv_python = venv_path / "bin" / "python"

    # Install via uv pip in the venv
    success = run_uv([
        "pip", "install",
        "--python", str(venv_python),
        "-r", str(OPENMEM_ROOT / "requirements.txt"),
    ])

    if success:
        log("")
        log("Dependencies installed successfully")
        log(f"Activate with: {venv_path}")
        log(f"Or run: {venv_python} bin/launcher.py")
    else:
        log("")
        log("WARNING: Some dependencies may have failed to install")
        log("Try installing manually:")
        log(f"  {uv_path} pip install -r requirements.txt")

    return success


def main():
    import argparse

    parser = argparse.ArgumentParser(description="OpenMem uv Bootstrap")
    parser.add_argument("--uv-only", action="store_true", help="Only install uv, skip deps")
    parser.add_argument("--check", action="store_true", help="Check if uv is available")

    args = parser.parse_args()

    if args.check:
        path = check_uv()
        if path:
            print(f"uv found: {path}")
            result = subprocess.run([path, "--version"], capture_output=True, text=True)
            print(f"version: {result.stdout.strip()}")
        else:
            print("uv not found")
            print("Run: python bin/uv_bootstrap.py  to install")
        return

    if args.uv_only:
        path = check_uv() or install_uv()
        if path:
            print(f"uv installed: {path}")
        else:
            print("Failed to install uv")
        return

    success = bootstrap_and_install()
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
