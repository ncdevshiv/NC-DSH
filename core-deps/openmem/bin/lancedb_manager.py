#!/usr/bin/env python3
"""
LanceDB Binary Manager for Autonomous Agents.
Downloads, installs, and manages LanceDB for any agent platform.

Usage:
    python lancedb_manager.py install     # Download and install LanceDB
    python lancedb_manager.py start       # Start LanceDB server
    python lancedb_manager.py stop        # Stop LanceDB server
    python lancedb_manager.py status      # Check status
    python lancedb_manager.py init        # Initialize default database
"""

import os
import sys
import json
import logging
import subprocess
import platform
import urllib.request
import zipfile
import tarfile
import shutil
from pathlib import Path
from datetime import datetime
from typing import Optional

logger = logging.getLogger("openmem.lancedb_manager")

# Binary directory (relative to this script)
BIN_DIR = Path(__file__).parent
DATA_DIR = BIN_DIR.parent / "data" / "lancedb_storage"
LANCE_VERSION = "0.12.0"

# System detection
SYSTEM = platform.system().lower()  # 'linux', 'darwin', 'windows'
ARCH = platform.machine().lower()  # 'x86_64', 'arm64', 'aarch64'


class LanceDBManager:
    """
    Manages LanceDB binary installation and lifecycle.
    Designed for autonomous agents that need to self-host LanceDB.
    """

    def __init__(self, install_dir: Path = None):
        self.install_dir = install_dir or BIN_DIR
        self.install_dir.mkdir(parents=True, exist_ok=True)
        
        self.lance_bin = self.install_dir / "lance"
        self.db_dir = DATA_DIR
        self.db_dir.mkdir(parents=True, exist_ok=True)
        
        self.pid_file = self.install_dir / "lance_server.pid"
        self.log_file = self.install_dir / "lance_server.log"

    def is_installed(self) -> bool:
        """Check if LanceDB is installed."""
        if self.lance_bin.exists():
            return True
        # Check if we can import lancedb
        try:
            import lancedb
            return True
        except ImportError:
            return False

    def get_lance_download_url(self) -> str:
        """Get LanceDB download URL for current platform."""
        base_url = f"https://github.com/lancedb/lance/releases/download/v{LANCE_VERSION}"
        
        if SYSTEM == "linux":
            if ARCH in ["x86_64", "amd64"]:
                return f"{base_url}/lance-{LANCE_VERSION}-x86_64-unknown-linux-gnu.tar.gz"
            elif ARCH in ["aarch64", "arm64"]:
                return f"{base_url}/lance-{LANCE_VERSION}-aarch64-unknown-linux-gnu.tar.gz"
        elif SYSTEM == "darwin":
            if ARCH in ["x86_64", "amd64"]:
                return f"{base_url}/lance-{LANCE_VERSION}-x86_64-apple-darwin.tar.gz"
            elif ARCH in ["arm64", "aarch64"]:
                return f"{base_url}/lance-{LANCE_VERSION}-aarch64-apple-darwin.tar.gz"
        elif SYSTEM == "windows":
            return f"{base_url}/lance-{LANCE_VERSION}-x86_64-pc-windows-msvc.tar.gz"
        
        raise ValueError(f"Unsupported platform: {SYSTEM}/{ARCH}")

    def install(self, force: bool = False) -> bool:
        """
        Download and install LanceDB binary.
        
        Args:
            force: Force reinstall even if exists
            
        Returns:
            True if successful
        """
        if self.is_installed() and not force:
            print(f"[LanceDB Manager] Already installed at {self.lance_bin}")
            return True

        print(f"[LanceDB Manager] Installing LanceDB v{LANCE_VERSION} for {SYSTEM}/{ARCH}")
        
        try:
            url = self.get_lance_download_url()
            archive_path = self.install_dir / f"lance-{LANCE_VERSION}.tar.gz"
            
            print(f"[LanceDB Manager] Downloading from {url}...")
            urllib.request.urlretrieve(url, archive_path)
            
            print("[LanceDB Manager] Extracting...")
            with tarfile.open(archive_path, "r:gz") as tar:
                tar.extractall(self.install_dir)
            
            # Make executable (Linux/Mac)
            if SYSTEM != "windows":
                os.chmod(self.lance_bin, 0o755)
            
            # Cleanup
            archive_path.unlink(missing_ok=True)
            
            print(f"[LanceDB Manager] Installed to {self.lance_bin}")
            return True
            
        except Exception as e:
            print(f"[LanceDB Manager] Installation failed: {e}")
            return False

    def start_server(
        self,
        host: str = "localhost",
        port: int = 8080,
        storage_path: str = None
    ) -> bool:
        """
        Start LanceDB server in background.
        
        Args:
            host: Server host
            port: Server port
            storage_path: Path for data storage
            
        Returns:
            True if started successfully
        """
        if not self.is_installed():
            print("[LanceDB Manager] LanceDB not installed. Run 'install' first.")
            return False

        if self.is_server_running():
            print("[LanceDB Manager] Server already running")
            return True

        storage_path = storage_path or str(self.db_dir)
        
        cmd = [
            str(self.lance_bin),
            "server",
            "--host", host,
            "--port", str(port),
            "--storage-path", storage_path
        ]
        
        try:
            log_file = open(self.log_file, "w")
            process = subprocess.Popen(
                cmd,
                stdout=log_file,
                stderr=subprocess.STDOUT
            )
            
            # Save PID
            with open(self.pid_file, "w") as f:
                f.write(str(process.pid))
            
            print(f"[LanceDB Manager] Server started on {host}:{port}")
            print(f"[LanceDB Manager] PID: {process.pid}, Log: {self.log_file}")
            return True
            
        except Exception as e:
            print(f"[LanceDB Manager] Failed to start server: {e}")
            return False

    def stop_server(self) -> bool:
        """Stop LanceDB server."""
        if not self.pid_file.exists():
            print("[LanceDB Manager] Server PID file not found")
            return False
        
        try:
            with open(self.pid_file, "r") as f:
                pid = int(f.read().strip())
            
            if SYSTEM == "windows":
                subprocess.run(["taskkill", "/F", "/PID", str(pid)], check=True)
            else:
                os.kill(pid, 9)
            
            self.pid_file.unlink(missing_ok=True)
            print("[LanceDB Manager] Server stopped")
            return True
        except Exception as e:
            print(f"[LanceDB Manager] Failed to stop server: {e}")
            # Clean up PID file anyway
            self.pid_file.unlink(missing_ok=True)
            return False

    def is_server_running(self) -> bool:
        """Check if server is running."""
        if not self.pid_file.exists():
            return False
        
        try:
            with open(self.pid_file, "r") as f:
                pid = int(f.read().strip())
            
            # Check if process exists
            if SYSTEM == "windows":
                result = subprocess.run(
                    ["tasklist", "/FI", f"PID eq {pid}"],
                    capture_output=True,
                    text=True
                )
                return str(pid) in result.stdout
            else:
                os.kill(pid, 0)
                return True
        except Exception:
            return False

    def status(self) -> dict:
        """Get server status."""
        return {
            "installed": self.is_installed(),
            "server_running": self.is_server_running(),
            "install_dir": str(self.install_dir),
            "data_dir": str(self.db_dir),
            "lance_bin": str(self.lance_bin) if self.lance_bin.exists() else None,
            "pid_file": str(self.pid_file) if self.pid_file.exists() else None
        }

    def init_database(self, db_path: str = None) -> bool:
        """
        Initialize a new LanceDB database.
        
        Args:
            db_path: Path for database (default: data/lancedb_storage)
            
        Returns:
            True if successful
        """
        db_path = db_path or str(self.db_dir)
        
        try:
            import lancedb
            db = lancedb.connect(db_path)
            print(f"[LanceDB Manager] Database initialized at {db_path}")
            return True
        except Exception as e:
            print(f"[LanceDB Manager] Database init failed: {e}")
            return False


def main():
    if len(sys.argv) < 2:
        print("LanceDB Binary Manager for Autonomous Agents")
        print()
        print("Usage: python lancedb_manager.py <command>")
        print()
        print("Commands:")
        print("  install   - Download and install LanceDB binary")
        print("  start     - Start LanceDB server")
        print("  stop      - Stop LanceDB server")
        print("  status    - Check installation/server status")
        print("  init      - Initialize default database")
        print()
        sys.exit(1)

    manager = LanceDBManager()
    command = sys.argv[1].lower()

    if command == "install":
        force = "--force" in sys.argv or "-f" in sys.argv
        success = manager.install(force=force)
        sys.exit(0 if success else 1)

    elif command == "start":
        # Optional host/port args
        host = "localhost"
        port = 8080
        for i, arg in enumerate(sys.argv):
            if arg == "--host" and i + 1 < len(sys.argv):
                host = sys.argv[i + 1]
            if arg == "--port" and i + 1 < len(sys.argv):
                port = int(sys.argv[i + 1])
        
        success = manager.start_server(host=host, port=port)
        sys.exit(0 if success else 1)

    elif command == "stop":
        success = manager.stop_server()
        sys.exit(0 if success else 1)

    elif command == "status":
        status = manager.status()
        print(json.dumps(status, indent=2))
        sys.exit(0)

    elif command == "init":
        db_path = None
        for i, arg in enumerate(sys.argv):
            if arg == "--path" and i + 1 < len(sys.argv):
                db_path = sys.argv[i + 1]
        success = manager.init_database(db_path)
        sys.exit(0 if success else 1)

    else:
        print(f"Unknown command: {command}")
        sys.exit(1)


if __name__ == "__main__":
    main()
