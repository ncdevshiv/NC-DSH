#!/usr/bin/env python3
"""
OpenMem Installer — uv-powered.

Installs uv (if missing), all dependencies, initializes database,
and sets up the complete environment.

Usage:
    python bin/install.py              # Full installation
    python bin/install.py --uv-only    # Only install uv
    python bin/install.py --deps       # Install deps only (assumes uv present)
    python bin/install.py --init-db    # Initialize DB only
    python bin/install.py --skills     # Generate skills only
    python bin/install.py --all        # Full installation (default)
"""

import os
import sys
import json
import subprocess
from pathlib import Path
from datetime import datetime

BIN_DIR = Path(__file__).parent
OPENMEM_ROOT = BIN_DIR.parent
DATA_DIR = OPENMEM_ROOT / "data"
REQUIREMENTS = OPENMEM_ROOT / "requirements.txt"


class OpenMemInstaller:
    """Handles full OpenMem installation using uv."""

    def __init__(self):
        self.steps_completed = []
        self.steps_failed = []
        self.uv_path = None
        # Fix Windows console encoding
        if sys.platform == "win32":
            try:
                sys.stdout.reconfigure(encoding="utf-8")
                sys.stderr.reconfigure(encoding="utf-8")
            except AttributeError:
                pass

    def log(self, msg):
        """Print status message."""
        print(f"[OpenMem Install] {msg}")

    # ------------------------------------------------------------------ #
    # uv helpers
    # ------------------------------------------------------------------ #

    def find_uv(self):
        """Find or bootstrap uv binary."""
        # Check bin/uv/ first (portable)
        if sys.platform == "win32":
            local_uv = BIN_DIR / "uv" / "uv.exe"
        else:
            local_uv = BIN_DIR / "uv" / "uv"

        if local_uv.exists():
            self.uv_path = str(local_uv)
            return True

        # Check PATH
        try:
            result = subprocess.run(
                ["uv", "--version"],
                capture_output=True, text=True, timeout=10
            )
            if result.returncode == 0:
                self.uv_path = "uv"
                return True
        except (FileNotFoundError, subprocess.TimeoutExpired):
            pass

        # Bootstrap
        self.log("uv not found, bootstrapping...")
        return self._bootstrap_uv()

    def _bootstrap_uv(self):
        """Run the uv bootstrap script."""
        bootstrap_script = BIN_DIR / "uv_bootstrap.py"
        if not bootstrap_script.exists():
            self.log("ERROR: bin/uv_bootstrap.py not found")
            return False

        try:
            result = subprocess.run(
                [sys.executable, str(bootstrap_script), "--uv-only"],
                capture_output=True, text=True, timeout=300,
            )
            if result.returncode != 0:
                self.log(f"uv bootstrap failed: {result.stderr[:500]}")
                return False

            # Now find it
            if sys.platform == "win32":
                local_uv = BIN_DIR / "uv" / "uv.exe"
            else:
                local_uv = BIN_DIR / "uv" / "uv"

            if local_uv.exists():
                self.uv_path = str(local_uv)
                return True

            self.log("uv installed but not found in expected location")
            return False

        except Exception as e:
            self.log(f"uv bootstrap error: {e}")
            return False

    def run_uv(self, args, timeout=600):
        """Run a uv command."""
        if not self.uv_path:
            if not self.find_uv():
                return False

        cmd = [self.uv_path] + list(args)
        self.log(f"Running: {' '.join(cmd)}")

        try:
            result = subprocess.run(
                cmd,
                cwd=str(OPENMEM_ROOT),
                capture_output=True,
                text=True,
                timeout=timeout,
            )
            # Print last few lines of output
            for line in (result.stdout or "").strip().split("\n")[-3:]:
                if line.strip():
                    self.log(f"  {line}")
            if result.returncode != 0 and result.stderr:
                for line in result.stderr.strip().split("\n")[-3:]:
                    if line.strip():
                        self.log(f"  stderr: {line}")
            return result.returncode == 0
        except subprocess.TimeoutExpired:
            self.log("  Command timed out")
            return False
        except Exception as e:
            self.log(f"  Command failed: {e}")
            return False

    def uv_pip_install_requirements(self):
        """Install requirements using uv pip."""
        # Try venv first, then system
        venv_path = OPENMEM_ROOT / ".venv"
        if not venv_path.exists():
            self.log("Creating virtual environment...")
            self.run_uv(["venv", str(venv_path)])

        if venv_path.exists():
            if sys.platform == "win32":
                venv_python = str(venv_path / "Scripts" / "python.exe")
            else:
                venv_python = str(venv_path / "bin" / "python")

            success = self.run_uv([
                "pip", "install",
                "--python", venv_python,
                "-r", str(REQUIREMENTS),
            ])
        else:
            # Fallback to system-wide
            self.log("Falling back to system-wide install...")
            success = self.run_uv([
                "pip", "install",
                "-r", str(REQUIREMENTS),
            ])

        return success

    # ------------------------------------------------------------------ #
    # Installation steps
    # ------------------------------------------------------------------ #

    def step_deps(self):
        """Install Python dependencies using uv."""
        self.log("Installing dependencies with uv...")

        if not REQUIREMENTS.exists():
            self.log("requirements.txt not found")
            self.steps_failed.append("deps")
            return False

        success = self.uv_pip_install_requirements()

        if success:
            self.log("Dependencies installed successfully")
            self.steps_completed.append("deps")
        else:
            self.log("Some dependencies may have failed (check output above)")
            self.steps_failed.append("deps")

        return success

    def step_dirs(self):
        """Create required data directories."""
        self.log("Creating data directories...")

        dirs = [
            DATA_DIR / "lancedb",
            DATA_DIR / "memory" / "daily",
            DATA_DIR / "memory" / "weekly",
            DATA_DIR / "memory" / "longterm",
            DATA_DIR / "optimizer",
            DATA_DIR / "evolution",
            DATA_DIR / "sessions",
            DATA_DIR / "usermodel",
            DATA_DIR / "reflections",
        ]

        for d in dirs:
            os.makedirs(d, exist_ok=True)

        self.log(f"Data directories: {DATA_DIR}")
        self.steps_completed.append("dirs")
        return True

    def step_init_db(self):
        """Initialize LanceDB database."""
        self.log("Initializing vector database...")

        try:
            sys.path.insert(0, str(OPENMEM_ROOT))
            from memory_store.vector_db import LanceDBVectorStore
            db = LanceDBVectorStore()
            self.log(f"Vector DB initialized at {db.db_path}")
            self.steps_completed.append("init_db")
            return True
        except Exception as e:
            self.log(f"DB init skipped (LanceDB not yet installed): {e}")
            self.steps_failed.append("init_db")
            return False

    def step_skills(self):
        """Generate skill files for all agents."""
        self.log("Generating skill files...")

        gen_script = BIN_DIR / "generate_skills.py"
        if not gen_script.exists():
            self.log("generate_skills.py not found, skipping")
            self.steps_failed.append("skills")
            return False

        try:
            result = subprocess.run(
                [sys.executable, str(gen_script)],
                capture_output=True, text=True, timeout=60,
                cwd=str(OPENMEM_ROOT),
            )
            if result.returncode == 0:
                self.log("Skills generated successfully")
                self.steps_completed.append("skills")
                return True
            else:
                self.log(f"Skill generation failed: {result.stderr[:300]}")
                self.steps_failed.append("skills")
                return False
        except Exception as e:
            self.log(f"Skill generation error: {e}")
            self.steps_failed.append("skills")
            return False

    def step_config(self):
        """Generate default configuration."""
        self.log("Generating configuration...")

        config = {
            "version": "2.0.0",
            "agent": "auto-detect",
            "llm": {
                "provider": "auto",
                "model": "auto",
                "api_key_env": "AUTO",
            },
            "memory": {
                "db_path": str(DATA_DIR / "lancedb"),
                "embedding_model": "all-MiniLM-L6-v2",
                "max_memories": 1000,
                "consolidation_schedule": "auto",
            },
            "learning": {
                "auto_learn": True,
                "interval_hours": 2,
                "enable_skill_generation": True,
                "enable_evolution": True,
            },
            "agents": {a: {"enabled": True} for a in [
                "qwen_code", "claude_code", "codex_cli", "opencode",
                "antigravity_ide", "kilo_cli", "vscode", "windsurf",
                "cursor", "openclaw",
            ]},
            "installed_at": datetime.now().isoformat(),
        }

        config_path = OPENMEM_ROOT / "config.json"
        try:
            with open(config_path, "w", encoding="utf-8") as f:
                json.dump(config, f, indent=2)
            self.log(f"Configuration written to {config_path}")
            self.steps_completed.append("config")
            return True
        except OSError as e:
            self.log(f"Config write failed: {e}")
            self.steps_failed.append("config")
            return False

    def step_gitignore(self):
        """Ensure data directories are gitignored."""
        self.log("Checking .gitignore...")

        gitignore_path = OPENMEM_ROOT / ".gitignore"
        entries = [
            "# OpenMem runtime data (auto-generated)",
            "data/",
            "generated_skills/",
            ".venv/",
            "__pycache__/",
            "*.pyc",
            ".pytest_cache/",
        ]

        try:
            existing = ""
            if gitignore_path.exists():
                with open(gitignore_path, "r", encoding="utf-8") as f:
                    existing = f.read()

            missing = [e for e in entries if e not in existing]
            if not missing:
                self.log(".gitignore already up to date")
                return True

            with open(gitignore_path, "a", encoding="utf-8") as f:
                f.write("\n" + "\n".join(missing) + "\n")
            self.log(f".gitignore updated ({len(missing)} entries added)")
            self.steps_completed.append("gitignore")
            return True
        except OSError as e:
            self.log(f".gitignore update failed: {e}")
            return False

    # ------------------------------------------------------------------ #
    # Main entry
    # ------------------------------------------------------------------ #

    def run_all(self):
        """Run all installation steps."""
        self.log("=" * 60)
        self.log("OpenMem — Autonomous Memory System Installer (uv)")
        self.log("=" * 60)
        self.log("")

        self.step_deps()
        self.step_dirs()
        self.step_init_db()
        self.step_skills()
        self.step_config()
        self.step_gitignore()

        self.log("")
        self.log("=" * 60)
        self.log("Installation Summary")
        self.log("=" * 60)
        self.log(f"Completed: {', '.join(self.steps_completed) or 'none'}")
        if self.steps_failed:
            self.log(f"Failed: {', '.join(self.steps_failed)}")
        self.log("")
        self.log("Next steps:")
        venv = OPENMEM_ROOT / ".venv"
        if venv.exists():
            if sys.platform == "win32":
                self.log(f"  .venv\\Scripts\\activate  # activate venv")
            else:
                self.log(f"  source .venv/bin/activate")
        self.log("  python main.py status       # Check system status")
        self.log("  python main.py run-cycle    # Run first learning cycle")
        self.log("  python bin/launcher.py      # Use unified launcher")
        self.log("")

        return len(self.steps_failed) == 0


def main():
    import argparse

    parser = argparse.ArgumentParser(description="OpenMem Installer (uv)")
    parser.add_argument("--uv-only", action="store_true", help="Only bootstrap uv")
    parser.add_argument("--deps", action="store_true", help="Install dependencies only")
    parser.add_argument("--init-db", action="store_true", help="Initialize DB only")
    parser.add_argument("--skills", action="store_true", help="Generate skills only")
    parser.add_argument("--all", action="store_true", help="Full installation")

    args = parser.parse_args()
    installer = OpenMemInstaller()

    if args.uv_only:
        installer.find_uv()
    elif args.deps:
        installer.step_deps()
    elif getattr(args, "init_db", False):
        installer.step_init_db()
    elif args.skills:
        installer.step_skills()
    elif args.all or (not args.uv_only and not args.deps and not getattr(args, "init_db", False) and not args.skills):
        installer.run_all()


if __name__ == "__main__":
    main()
