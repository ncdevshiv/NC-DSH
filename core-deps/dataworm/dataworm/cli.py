"""Command-line interface for DataWorm (v2: thin client over the daemon).

Every command routes through the centralized daemon via JSON-RPC, unless
``--no-daemon`` is given (then it runs in-process via ``Core`` directly). The
daemon is auto-spawned on the first call if not already running.

Commands:
  init       Crawl <dir> once, start watching it, open the live dashboard.
             This is the "launch the worm" entrypoint: the daemon stays alive
             in the background, watches the dir for changes, and re-crawls
             live. Subsequent CLI calls (impact/summary/search) query it.
  crawl      Traverse a dir and build the graph. ``--watch`` keeps watching.
  impact     Blast radius: what depends on a file?
  context    Full context bundle for a node.
  neighbors  Nodes within N hops.
  search     Substring search over node paths.
  summary    Graph stats + convergence info.
  status     Daemon liveness + backend + watched roots.
  stop       Shut the daemon down (and all its watchers).
  mcp        Run the MCP stdio server (for Claude Desktop / Cursor / any MCP client).
  update     Upgrade DataWorm to the latest distribution (restarts the worm).
  up         Self-update: reinstall the latest build from the LOCAL source dir
             (`uv tool install --force --editable <source>`; git comes later).

Bare invocation: `dataworm` or `dw` with NO arguments is `init` on the current
directory — crawl cwd once, watch it, ensure the daemon, open the dashboard.
(`dw` is a second console-script alias of this same entrypoint.)

Flags:
  --no-daemon   Run in-process (no server). Implies direct Core.call.
  --no-rust     Force the Python backend (ignore dataworm._rust).
  --json        Machine-readable output.
  --web         Open the live dashboard (served by the daemon).
  --watch       Start a filesystem watcher on the crawled root (live re-crawl).
  --live        Stream crawl events to the terminal.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
import tempfile
import time
import webbrowser
from pathlib import Path

from dataworm.core import DEFAULT_DB
from dataworm.logging_setup import log_file_hint, setup_logging
from dataworm.models import EdgeType


# ---- dw up: self-update from the local source dir --------------------------

def _resolve_source() -> str:
    """Where to reinstall from: env > marker file > editable metadata > default."""
    env = os.environ.get("DATAWORM_SOURCE")
    if env:
        return os.path.abspath(env)
    try:
        marker = Path.home() / ".dataworm" / "source.txt"
        if marker.exists():
            val = marker.read_text(encoding="utf-8").strip()
            if val:
                return os.path.abspath(val)
    except OSError:
        pass
    try:
        from importlib.metadata import distribution
        from urllib.parse import urlparse
        from urllib.request import url2pathname
        du = distribution("dataworm").read_text("direct_url.json")
        import json as _json
        info = _json.loads(du or "{}") or {}
        url = info.get("url", "")
        if url.startswith("file:"):
            parsed = urlparse(url)
            p = url2pathname(parsed.path)
            if parsed.netloc:  # rare file://host/share UNC form
                p = "\\\\" + parsed.netloc + p
            if p and os.path.exists(p):
                return os.path.abspath(p)
    except Exception:
        pass
    return r"F:\dataworm"


def _resolve_repo() -> str:
    """The GitHub repo `dw up` syncs from: env > marker file > well-known."""
    env = os.environ.get("DATAWORM_REPO")
    if env:
        return env
    try:
        marker = Path.home() / ".dataworm" / "repo.txt"
        if marker.exists():
            val = marker.read_text(encoding="utf-8").strip()
            if val:
                return val
    except OSError:
        pass
    return "https://github.com/ncdevshiv/dataworm.git"


def _sync_branch(repo: str, branch: str) -> str:
    """Shallow-clone/sync ``repo`` at ``branch`` into ~/.dataworm/src/<branch>.

    Returns the synced source dir. Keeps ignored files (rust/target build
    cache survives between updates → incremental compiles stay fast).
    """
    import subprocess as _sp

    git = shutil.which("git")
    if not git:
        print("error: 'git' is required for branch updates "
              "(or use: dw up --from <local-dir>)", file=sys.stderr)
        raise SystemExit(1)

    def _run(*argv: str) -> None:
        rc = _sp.run([git, *argv]).returncode
        if rc != 0:
            print(f"error: git {' '.join(argv[:3])}… failed ({rc})",
                  file=sys.stderr)
            raise SystemExit(rc)

    base = Path.home() / ".dataworm" / "src"
    dst = base / branch
    if (dst / ".git").exists():
        _run("-C", str(dst), "fetch", "origin", "--prune", "--tags")
        _run("-C", str(dst), "checkout", "-B", branch,
             f"origin/{branch}")
        _run("-C", str(dst), "reset", "--hard", f"origin/{branch}")
        _run("-C", str(dst), "clean", "-fd")
    else:
        base.mkdir(parents=True, exist_ok=True)
        _run("clone", "--depth", "1", "--branch", branch, repo, str(dst))
    return str(dst)


def cmd_up(args) -> None:
    r"""`dw up [main|dev|<branch>]`: update + reinstall the latest build.

    Default mode is the GitHub repo: the requested branch is shallow-synced
    into ~/.dataworm/src/<branch> and installed from there (`dw up`,
    `dw up main`, `dw up dev`). Use --from <dir> to install a LOCAL checkout
    instead (git skipped). Never touches user data (<dir>/.dataworm graphs).
    """
    import shutil
    import subprocess

    branch = (getattr(args, "branch", None) or "main").strip("/")
    if args.source:
        src = os.path.abspath(args.source)
        print(f"dw up: local source = {src} (git skipped)")
    else:
        repo = _resolve_repo()
        print(f"dw up: syncing {repo}@{branch} …")
        src = _sync_branch(repo, branch)
        print(f"dw up: synced to {src}")
    if not os.path.isdir(src):
        print(f"error: source dir not found: {src}", file=sys.stderr)
        raise SystemExit(1)

    uv = shutil.which("uv")
    if uv is None:
        print("error: 'uv' was not found on PATH — install it first "
              "(https://docs.astral.sh/uv/getting-started/), then retry.",
              file=sys.stderr)
        raise SystemExit(1)

    # Non-editable ON PURPOSE: an editable rebuild copies the fresh _rust.pyd
    # INTO the workspace tree, where AV scanners (or any running worm process)
    # transiently lock it → maturin dies with os error 32. A real wheel build
    # installs into the tool env instead — no workspace file is touched.
    cmd = [uv, "tool", "install", "--no-cache", "--force", src]
    # Child builds need cargo for the maturin/Rust extension; some minimal
    # environments (cmd shims, detached daemons) lack it on PATH. Resolve it
    # the way THIS process did (shutil.which) plus the rustup default.
    child_env = dict(os.environ)
    extra: list[str] = [os.path.expanduser(r"~\.cargo\bin")]
    cargo = shutil.which("cargo")
    if cargo:
        extra.insert(0, os.path.dirname(cargo))
    existing = child_env.get("PATH", "")
    for d in extra:
        if d and os.path.isdir(d) and d.lower() not in existing.lower():
            existing += os.pathsep + d
    child_env["PATH"] = existing

    # A running daemon holds engine files locked on Windows; free it first so
    # the reinstall lands cleanly. (User graph data is never touched.)
    try:
        from dataworm.server import stop_daemon
        stop_daemon(DEFAULT_DB)
    except Exception:
        pass

    # Self-update paradox: when this very process runs from the tool env that
    # uv must replace, Windows denies removing its Scripts dir. Hand off to a
    # DETACHED helper that waits for us to exit, then performs the swap.
    tool_marker = os.path.join("uv", "tools", "dataworm")
    running_from_tool = tool_marker in sys.executable.lower().replace("/", "\\")
    if running_from_tool:
        # In-place update of THIS env: `uv tool install` wants to DELETE the
        # Scripts dir (denied while any worm binary lives here), whereas
        # `uv pip install --python <this env> --reinstall <src>` swaps only
        # site-packages content — no removal, no paradox.
        tool_python = sys.executable
        cmd = [uv, "pip", "install", "--python", tool_python,
               "--reinstall", src, "--no-deps"]
        log_path = os.path.join(tempfile.gettempdir(), "dw-up.log")
        bat = os.path.join(tempfile.gettempdir(), "dw-up.bat")
        install_line = " ".join(cmd) + ' >> "' + log_path + '" 2>&1'
        with open(bat, "w", encoding="utf-8") as fh:
            fh.write("@echo off\r\n")
            fh.write("timeout /t 3 /nobreak >nul\r\n")
            fh.write(install_line + "\r\n")
            fh.write('echo DW_UP_EXIT %ERRORLEVEL% >> "' + log_path + '"\r\n')
        flags = 0x00000008 | 0x00000200  # DETACHED_PROCESS | NEW_PROCESS_GROUP
        subprocess.Popen(["cmd", "/c", bat], creationflags=flags,
                         stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, env=child_env, close_fds=True)
        print("dw up: this command runs from the installation being replaced —")
        print("  the update continues detached after dw exits.")
        print(f"  watch it:      type {log_path}")
        print("  then confirm:  dw status   (run 'dw' in your project dir)")
        return

    print("DataWorm self-update plan:")
    print(f"  source: {src}")
    print(f"  uv:     {uv}")
    print(f"  cmd:    {' '.join(cmd)}")

    def _kill_stragglers() -> int:
        r"""Kill processes whose IMAGE lives inside the tool env (they hold
        Scripts\ locked even when their command line mentions nothing useful)
        plus any dataworm-named python anywhere."""
        killed = 0
        try:
            if os.name == "nt":
                ps = (
                    "Get-Process | Where-Object { try { $_.Path -like "
                    "'*uv*tools*dataworm*' } catch { $false } } | "
                    "Select-Object -ExpandProperty Id"
                )
                out = subprocess.run(["powershell", "-NoProfile", "-Command", ps],
                                     capture_output=True, text=True,
                                     timeout=20).stdout
                for pid in out.split():
                    if pid.strip().isdigit():
                        subprocess.run(["taskkill", "/PID", pid.strip(), "/F"],
                                       capture_output=True)
                        killed += 1
                out2 = subprocess.run(
                    ["powershell", "-NoProfile", "-Command",
                     "Get-CimInstance Win32_Process -Filter \"Name like "
                     "'python%'\" | Where-Object {$_.CommandLine -match "
                     "'dataworm'} | Select-Object -ExpandProperty ProcessId"],
                    capture_output=True, text=True, timeout=20).stdout
                for pid in out2.split():
                    if pid.strip().isdigit():
                        subprocess.run(["taskkill", "/PID", pid.strip(), "/F"],
                                       capture_output=True)
                        killed += 1
            else:
                subprocess.run(["pkill", "-f", "dataworm"], capture_output=True)
                killed = 1
        except Exception:
            pass
        return killed

    _kill_stragglers()

    proc = subprocess.run(cmd, env=child_env)  # no capture -> streams live
    if proc.returncode != 0 and _kill_stragglers():
        print("dw up: cleared running worm processes; retrying once…")
        time.sleep(1.5)
        proc = subprocess.run(cmd, env=child_env)
    if proc.returncode != 0:
        print(f"dw up: uv exited with {proc.returncode} — install unchanged. "
              "Close running worm processes and retry.", file=sys.stderr)
        raise SystemExit(proc.returncode)

    # Post-flight: the freshly installed distribution must still resolve.
    try:
        from importlib.metadata import version
        ver = version("dataworm")
    except Exception as exc:
        print(f"error: install finished but 'dataworm' no longer resolves "
              f"({exc}); recover with scripts/install-dw.ps1", file=sys.stderr)
        raise SystemExit(1)
    print(f"dw up: done — dataworm {ver} installed from {src}")
    print("Running daemons keep using the old build until restarted:")
    print("  dataworm stop   # then run 'dw' in your project dir again")


# ---- output helpers -------------------------------------------------------

def _print(data, as_json: bool) -> None:
    if as_json:
        print(json.dumps(data, indent=2, default=str))
    else:
        _pretty(data)


def _pretty(data, indent: int = 0) -> None:
    pad = "  " * indent
    if isinstance(data, dict):
        for key, value in data.items():
            if isinstance(value, (dict, list)) and value:
                print(f"{pad}{key}:")
                _pretty(value, indent + 1)
            else:
                print(f"{pad}{key}: {value}")
    elif isinstance(data, list):
        if not data:
            print(f"{pad}[]")
        for item in data:
            if isinstance(item, (dict, list)):
                _pretty(item, indent)
            else:
                print(f"{pad}- {item}")
    else:
        print(f"{pad}{data}")


# ---- in-process fallback (for --no-daemon) --------------------------------

def _db_path_for(dir_arg: str, out: str | None) -> str:
    """Resolve the graph DB path. Default: <dir>/.dataworm/graph.db — so each
    directory's data lives *in* that directory (federated per-dir storage)."""
    if out:
        return out
    root = Path(dir_arg).resolve()
    return str(root / ".dataworm" / "graph.db")


# The graph DB path whose daemon THIS invocation actually used (set by the
# command funcs once they resolve --db/--out). The Ctrl-C soft close stops
# THAT daemon instead of blindly stopping whatever DEFAULT_DB points at.
_SESSION_DB: str | None = None


def _remember_session_db(db_path: str | None) -> None:
    global _SESSION_DB
    if db_path:
        _SESSION_DB = db_path


def _crawl_via_daemon(handle, params: dict, timeout: float = 1800.0) -> dict:
    """Call crawl on the daemon with a long timeout + friendly timeout handling.

    A large-tree crawl can take many minutes; the default 1800s (30 min) beats
    the old 300s. On timeout we print a friendly message (the daemon keeps
    running in the background — watch the dashboard) instead of a traceback.
    """
    try:
        return handle.call("crawl", params, timeout=timeout)
    except Exception as exc:
        # urllib timeout / connection error — the daemon is still running.
        msg = str(exc).lower()
        if "timeout" in msg or "timed out" in msg:
            print(f"\nThe crawl is still running in the background (it's a large tree).")
            print(f"Watch the dashboard at http://127.0.0.1:{handle.port}/ — it'll")
            print(f"show live progress and converge on its own. Run 'dataworm status' to check.")
            # 124 = the conventional "timed out" exit code: the RPC gave up,
            # even though the daemon-side crawl keeps going.
            sys.exit(124)
        raise


def _run_inprocess(method: str, params: dict, db: str, prefer_rust: bool) -> dict:
    """Run an op directly via Core, no daemon."""
    from dataworm.core import Core
    core = Core(db_path=db, prefer_rust=prefer_rust)
    return core.call(method, params)


# ---- commands -------------------------------------------------------------

def cmd_init(args) -> None:
    """Launch the worm: crawl <dir>, watch it, open the dashboard, exit.

    The daemon stays alive in the background after the CLI returns — it keeps
    watching the dir for changes and re-crawls live. Subsequent `dataworm`
    calls (impact/summary/search/...) hit the same warm daemon.
    """
    if args.no_daemon:
        # In-process mode we can't keep a background watcher alive after the
        # CLI exits; fall back to a one-shot crawl + dashboard note.
        print("init: --no-daemon mode; crawling once without a background watcher.")
        from dataworm.core import Core
        db_path = _db_path_for(args.dir, args.out)
        core = Core(db_path=db_path, prefer_rust=not args.no_rust)
        result = core.call("crawl", {
            "root": str(Path(args.dir).resolve()),
            "max_cycles": args.max_cycles,
            "enable_semantic": not args.no_semantic,
            "enable_hashing": not args.no_hashing,
            "similarity_threshold": args.threshold,
        })
        if "error" in result:
            sys.exit(f"error: {result['error']}")
        print(f"converged={result.get('converged')} cycles={result.get('cycles')} "
              f"nodes={result.get('nodes')} edges={result.get('edges')}")
        print(f"graph saved to {db_path} (--no-daemon: no live watcher)")
        return

    # Validate BEFORE touching daemon plumbing: a typo'd dir must not spawn
    # (or attach to) a daemon and litter <dir>/.dataworm.
    root = str(Path(args.dir).resolve())
    if not Path(root).is_dir():
        sys.exit(f"error: '{args.dir}' is not a directory")
    db_path = _db_path_for(args.dir, args.out)
    _remember_session_db(db_path)

    print("starting/connecting worm daemon…")
    from dataworm.server import ensure_daemon
    handle = ensure_daemon(db_path=db_path, prefer_rust=not args.no_rust)

    # 1. Crawl once (blocking) so the dashboard has a graph to render.
    params = {
        "root": root,
        "max_cycles": args.max_cycles,
        "enable_semantic": not args.no_semantic,
        "enable_hashing": not args.no_hashing,
        "similarity_threshold": args.threshold,
    }
    print(f"crawling {root} …")
    result = _crawl_via_daemon(handle, params)
    if "error" in result:
        sys.exit(f"error: {result['error']}")

    # 2. Start watching (unless --no-watch). The watcher runs in the daemon
    #    process, so it survives this CLI exiting.
    if not args.no_watch:
        w = handle.call("watch", {"root": root}, timeout=10)
        watch_status = w.get("status", "?")
        backend = w.get("backend", "?")
    else:
        watch_status = "disabled"
        backend = "-"

    # 3. Open the dashboard.
    url = f"http://127.0.0.1:{handle.port}/"
    if args.web:
        webbrowser.open(url)

    print(f"DataWorm worm launched.")
    print(f"  root:    {root}")
    print(f"  watch:  {watch_status} (backend: {backend})")
    print(f"  graph:  nodes={result.get('nodes')} edges={result.get('edges')} "
          f"(converged={result.get('converged')}, cycles={result.get('cycles')})")
    n_warnings = len(result.get("warnings") or [])
    print(f"  logs:   {log_file_hint()} ({n_warnings} crawl warning(s) recorded there)")
    print(f"  db:      {db_path}")
    print(f"  token:   {handle.token[:8]}...")
    print(f"  url:     {url}")
    print("The daemon is running in the background and watching for changes.")
    print("Use 'dataworm status' to check, 'dataworm stop' to shut down.")
    print("Tip: 'dw up' reinstalls this build from its local source anytime.")



def cmd_crawl(args) -> None:
    root = Path(args.dir).resolve()
    if not root.is_dir():
        sys.exit(f"error: '{args.dir}' is not a directory")
    db_path = _db_path_for(args.dir, args.out)
    params = {
        "root": str(root),
        "max_cycles": args.max_cycles,
        "enable_semantic": not args.no_semantic,
        "enable_hashing": not args.no_hashing,
        "similarity_threshold": args.threshold,
    }
    # Early feedback: the crawl can block for minutes — say so up front.
    print(f"crawling {root} ...")
    if args.no_daemon:
        result = _run_inprocess("crawl", params, db_path, not args.no_rust)
    else:
        from dataworm.server import ensure_daemon
        handle = ensure_daemon(db_path=db_path, prefer_rust=not args.no_rust)
        _remember_session_db(db_path)
        if args.web:
            webbrowser.open(f"http://127.0.0.1:{handle.port}/")
        if args.live:
            # Stream events from the daemon's SSE endpoint while the crawl runs.
            import threading
            done = threading.Event()
            t = threading.Thread(
                target=_stream_events,
                args=(handle, done),
                daemon=True,
            )
            t.start()
            result = _crawl_via_daemon(handle, params)
            done.set()
            t.join(timeout=2.0)
        else:
            result = _crawl_via_daemon(handle, params)
    if "error" in result:
        sys.exit(f"error: {result['error']}")
    print(f"converged={result.get('converged')} cycles={result.get('cycles')}")
    print(f"nodes={result.get('nodes')} edges={result.get('edges')}")
    for key in ("edges_contains", "edges_references", "edges_duplicate_of", "edges_similar_to"):
        print(f"  {key.replace('edges_', '')}: {result.get(key)}")
    print(f"graph saved to {db_path}")
    n_warnings = len(result.get("warnings") or [])
    print(f"logs: {log_file_hint()} ({n_warnings} warning(s) recorded there)")
    # Optionally start watching after the crawl completes.
    if args.watch and not args.no_daemon:
        w = handle.call("watch", {"root": str(root)}, timeout=10)
        print(f"watch: {w.get('status')} (backend: {w.get('backend', '?')}) "
              f"— daemon will re-crawl on file changes. Ctrl+C won't stop the daemon.")



def _stream_events(handle, done) -> None:
    """Connect to the daemon's /events SSE stream and print events to the terminal."""
    import urllib.request
    from dataworm.live import TerminalReporter
    reporter = TerminalReporter()
    url = f"http://127.0.0.1:{handle.port}/events"
    req = urllib.request.Request(url, headers={"Authorization": f"Bearer {handle.token}"})
    try:
        with urllib.request.urlopen(req, timeout=300) as resp:
            buf = b""
            while not done.is_set():
                chunk = resp.read(1)
                if not chunk:
                    break
                buf += chunk
                while b"\n\n" in buf:
                    raw, buf = buf.split(b"\n\n", 1)
                    line = raw.decode("utf-8", errors="ignore")
                    if line.startswith("data: "):
                        try:
                            ev = json.loads(line[6:])
                            reporter(ev)
                        except Exception:
                            pass
    except Exception:
        pass


def cmd_impact(args) -> None:
    _query(args, "impact", {"path": args.path})


def cmd_context(args) -> None:
    _query(args, "context", {"path": args.path})


def cmd_neighbors(args) -> None:
    types = args.type if args.type else None
    _query(args, "neighbors", {"path": args.path, "types": types, "depth": args.depth})


def cmd_search(args) -> None:
    _query(args, "search", {"text": args.text, "limit": args.limit})


def cmd_summary(args) -> None:
    _query(args, "summary", {})


def cmd_status(args) -> None:
    from dataworm.server import _read_port_file, _is_alive
    info = _read_port_file(args.out)
    if not info:
        print("daemon: not running")
        return
    port = int(info.get("port", 0))
    token = info.get("token", "")
    pid = int(info.get("pid", 0))
    alive = _is_alive(port, token)
    print(f"daemon: {'running' if alive else 'stale'} pid={pid} port={port}")
    if alive:
        from dataworm.server import DaemonHandle
        h = DaemonHandle(pid, port, token, args.out)
        ping = h.call("ping")
        print(f"  backend: {ping.get('backend')}")
        print(f"  db:      {ping.get('db')}")
        # Report watched roots (the worm's active eyes).
        watched = h.call("watched")
        roots = watched.get("roots", [])
        backends = watched.get("backends", {})
        if roots:
            print(f"  watching ({len(roots)}):")
            for r in roots:
                print(f"    - {r}  [{backends.get(r, '?')}]")
        else:
            print("  watching: (none)")



def cmd_stop(args) -> None:
    from dataworm.server import stop_daemon
    result = stop_daemon(args.out)
    print(f"daemon: {result.get('status')} (pid={result.get('pid')})")


def cmd_watch(args) -> None:
    """Watch a directory: the daemon incrementally re-crawls on every change.

    With ``--webhook URL`` the watch op params carry ``webhook_url`` so Core
    can POST each change report to your endpoint as it happens — this CLI only
    forwards the URL; consumption lives core-side.
    """
    params: dict = {"root": str(Path(args.dir).resolve())}
    if args.webhook:
        params["webhook_url"] = args.webhook
    _query(args, "watch", params)


def cmd_mcp(args) -> None:
    """Run the Model Context Protocol stdio server until stdin closes.

    Speaks newline-delimited JSON-RPC 2.0 (the current MCP stdio transport) on
    stdin/stdout; diagnostics go to stderr only. Any MCP client (Claude
    Desktop, Cursor, ...) can launch `dataworm mcp --db <graph.db>` as a child
    process and get the worm_* tools.
    """
    from dataworm.mcp import run_mcp
    sys.exit(run_mcp(db_path=args.db, prefer_rust=not args.no_rust))


# ---- update ---------------------------------------------------------------

def _get_installed_version() -> str:
    """Return the installed dataworm version via `pip show` (reads from disk)."""
    import subprocess
    try:
        proc = subprocess.run(
            [sys.executable, "-m", "pip", "show", "dataworm"],
            capture_output=True, text=True, timeout=60,
        )
        for line in (proc.stdout or "").splitlines():
            if line.lower().startswith("version:"):
                return line.split(":", 1)[1].strip()
    except Exception:
        pass
    return "unknown"


def _pip_install(spec: str) -> tuple[int, str]:
    """Run `python -m pip install --upgrade --force-reinstall --no-deps <spec>`.

    `--force-reinstall` guarantees the newest source is used even when the
    version number is unchanged (e.g. a local path install). `--no-deps` keeps
    the upgrade offline-friendly since dependencies are already present.
    Returns (rc, combined output).
    """
    import subprocess
    cmd = [sys.executable, "-m", "pip", "install",
           "--upgrade", "--force-reinstall", "--no-deps", spec]
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=900)
    except Exception as exc:  # noqa: BLE001
        return -1, f"failed to launch pip: {exc}"
    out = (proc.stdout or "") + (proc.stderr or "")
    return proc.returncode, out


def _detect_update_source() -> str:
    """Best-effort: where did this dataworm build come from?

    Used as the default upgrade target so `dataworm update` pulls from the same
    place it was installed (a local path, a git URL, or PyPI) rather than always
    hitting PyPI. Falls back to 'dataworm' (PyPI) if it can't tell.
    """
    try:
        import dataworm, glob, os, json
        candidates = []
        try:
            import importlib.metadata as md
            d = md.distribution("dataworm")
            if getattr(d, "_path", None):
                candidates.append(str(d._path))
        except Exception:
            pass
        candidates.append(os.path.dirname(os.path.dirname(dataworm.__file__)))
        for sp in candidates:
            for cand in sorted(
                glob.glob(os.path.join(sp, "dataworm-*.dist-info", "direct_url.json")),
                reverse=True,
            ):
                try:
                    info = json.loads(open(cand, encoding="utf-8").read())
                except Exception:
                    continue
                if info.get("editable") and info.get("dir"):
                    return info["dir"]
                url = info.get("url", "")
                if url and (url.startswith("git+") or url.startswith("file:")
                            or (url.startswith("http") and "pypi.org" not in url)):
                    return url
                if info.get("dir"):
                    return info["dir"]
    except Exception:
        pass
    return "dataworm"


def cmd_update(args) -> None:
    """Upgrade DataWorm to the latest build/version and relaunch the worm.

    Stops any running daemon (so the relaunch uses the freshly installed code),
    runs `pip install --upgrade <spec>` (default: the `dataworm` distribution),
    reports the old -> new version, then re-launches the worm on the same root
    it was watching (unless --no-restart). Use --from to point at a custom
    source, e.g. a git URL, a local path, or an internal index.
    """
    # On Windows the running `dataworm.exe` locks its own image file, so pip
    # cannot overwrite the console script while we are executing it. Re-launch
    # the updater under python.exe (detached) and exit, which releases the lock
    # so the install can replace dataworm.exe. (When invoked as
    # `python -m dataworm.cli update`, we're already python.exe, so skip this.)
    import os as _os
    if _os.path.basename(sys.argv[0]).lower() in ("dataworm", "dataworm.exe"):
        import subprocess as _sp, tempfile as _tf
        log = _os.path.join(_tf.gettempdir(), "dataworm-update.log")
        try:
            with open(log, "w") as _lf:
                _sp.Popen(
                    [sys.executable, "-m", "dataworm.cli"] + sys.argv[1:],
                    stdout=_lf, stderr=_sp.STDOUT,
                    creationflags=_sp.DETACHED_PROCESS | _sp.CREATE_NEW_PROCESS_GROUP,
                    close_fds=True,
                )
            print("dataworm update is running in the background (the dataworm.exe "
                  "image is locked while it runs).")
            print(f"Live log: {log}")
            print("When it finishes, the worm is relaunched automatically if it was running.")
            return
        except Exception:
            print("note: could not detach updater; falling back to in-process update "
                  "(this may fail to replace dataworm.exe on Windows).")

    from dataworm.server import _read_port_file, stop_daemon
    from dataworm.core import DEFAULT_DB

    old_ver = _get_installed_version()
    print(f"DataWorm current version: {old_ver}")

    # Find + stop a running daemon so the relaunch picks up the new build.
    was_running = False
    root = None
    info = _read_port_file(args.out)
    if info:
        db = info.get("db", "")
        if db:
            # db path is <root>/.dataworm/graph.db -> root is two levels up.
            root = str(Path(db).parent.parent)
        try:
            stop_daemon(args.out)
            was_running = True
            print("stopped running daemon to free the old build")
        except Exception:  # noqa: BLE001
            print("note: could not stop a running daemon (continuing)")

    # Perform the upgrade. Default to the auto-detected install source.
    spec = args.source or _detect_update_source()
    src_note = "" if args.source else " (auto-detected source)"
    print(f"upgrading via pip: {spec}{src_note}")
    rc, out = _pip_install(spec)
    # Print pip output (trimmed) so the user sees what happened.
    for line in out.splitlines()[-25:]:
        if line.strip():
            print("  " + line)
    if rc != 0:
        sys.exit("update failed (pip exited non-zero) — your install is unchanged.")

    new_ver = _get_installed_version()
    print(f"DataWorm updated: {old_ver} -> {new_ver}")

    # Relaunch the worm on the same root so the new build is in use.
    if was_running and not args.no_restart and root:
        print(f"re-launching worm on {root} ...")
        synth = argparse.Namespace(
            dir=root, out=None, port=8765, max_cycles=5,
            no_semantic=False, no_hashing=False, threshold=0.35,
            no_watch=False, web=False, no_daemon=False, no_rust=False,
        )
        cmd_init(synth)
    elif was_running and args.no_restart:
        print("daemon left stopped. Re-run 'dataworm init <dir>' to relaunch.")



# ---- shared query helper --------------------------------------------------

def _query(args, method: str, params: dict) -> None:
    if args.no_daemon:
        result = _run_inprocess(method, params, args.db, not args.no_rust)
    else:
        from dataworm.server import ensure_daemon
        handle = ensure_daemon(db_path=args.db, prefer_rust=not args.no_rust)
        _remember_session_db(args.db)
        result = handle.call(method, params)
    if "error" in result:
        sys.exit(f"error: {result['error']}")
    _print(result, args.json)


# ---- parser ---------------------------------------------------------------

def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="dataworm", description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    # init
    p = sub.add_parser("init", help="launch the worm: crawl <dir>, watch it, open dashboard")
    p.add_argument("dir", nargs="?", default=".", help="directory to crawl and watch (default: current dir)")
    p.add_argument("--out", default=None, help="graph DB path (default: <dir>/.dataworm/graph.db)")
    p.add_argument("--port", type=int, default=8765)
    p.add_argument("--max-cycles", type=int, default=5)
    p.add_argument("--no-semantic", action="store_true")
    p.add_argument("--no-hashing", action="store_true", help="skip the near-duplicate (hashing) pass")
    p.add_argument("--threshold", type=float, default=0.35)
    p.add_argument("--no-watch", action="store_true", help="crawl once, don't start the watcher")
    p.add_argument("--no-web", action="store_false", dest="web", help="don't auto-open the dashboard")
    p.add_argument("--no-daemon", action="store_true")
    p.add_argument("--no-rust", action="store_true")
    p.set_defaults(func=cmd_init, web=True)

    # crawl
    p = sub.add_parser("crawl", help="traverse a dir and build the graph")
    p.add_argument("dir")
    p.add_argument("--max-cycles", type=int, default=5)
    p.add_argument("--no-semantic", action="store_true")
    p.add_argument("--no-hashing", action="store_true", help="skip the near-duplicate (hashing) pass")
    p.add_argument("--threshold", type=float, default=0.35)
    p.add_argument("--out", default=None, help="graph DB path (default: <dir>/.dataworm/graph.db)")
    p.add_argument("--json", default="", help="also write a JSON export here")
    p.add_argument("--live", action="store_true", help="live terminal stream of events")
    p.add_argument("--web", action="store_true", help="open the live dashboard")
    p.add_argument("--watch", action="store_true", help="after crawling, watch the root for changes")
    p.add_argument("--no-daemon", action="store_true", help="run in-process, no server")
    p.add_argument("--no-rust", action="store_true", help="force the Python backend")
    p.set_defaults(func=cmd_crawl)

    # watch
    p = sub.add_parser("watch",
                       help="keep a dir watched: live incremental re-crawls (+ optional webhook push)")
    p.add_argument("dir", nargs="?", default=".",
                   help="directory to watch (default: current dir)")
    p.add_argument("--webhook", default=None, metavar="URL",
                   help="POST every change report to this URL as changes happen "
                        "(forwarded to the daemon as webhook_url)")
    p.add_argument("--db", default=DEFAULT_DB)
    p.add_argument("--json", action="store_true")
    p.add_argument("--no-daemon", action="store_true", help="run in-process, no server")
    p.add_argument("--no-rust", action="store_true", help="force the Python backend")
    p.set_defaults(func=cmd_watch)

    # impact
    p = sub.add_parser("impact", help="blast radius: what depends on a file")
    p.add_argument("path")
    p.add_argument("--db", default=DEFAULT_DB)
    p.add_argument("--json", action="store_true")
    p.add_argument("--no-daemon", action="store_true")
    p.add_argument("--no-rust", action="store_true")
    p.set_defaults(func=cmd_impact)

    # context
    p = sub.add_parser("context", help="full context bundle for a node")
    p.add_argument("path")
    p.add_argument("--db", default=DEFAULT_DB)
    p.add_argument("--json", action="store_true")
    p.add_argument("--no-daemon", action="store_true")
    p.add_argument("--no-rust", action="store_true")
    p.set_defaults(func=cmd_context)

    # neighbors
    p = sub.add_parser("neighbors", help="nodes within N hops")
    p.add_argument("path")
    p.add_argument("--type", action="append", choices=[t.value for t in EdgeType])
    p.add_argument("--depth", type=int, default=1)
    p.add_argument("--db", default=DEFAULT_DB)
    p.add_argument("--json", action="store_true")
    p.add_argument("--no-daemon", action="store_true")
    p.add_argument("--no-rust", action="store_true")
    p.set_defaults(func=cmd_neighbors)

    # search
    p = sub.add_parser("search", help="substring search over node paths")
    p.add_argument("text")
    p.add_argument("--limit", type=int, default=50)
    p.add_argument("--db", default=DEFAULT_DB)
    p.add_argument("--json", action="store_true")
    p.add_argument("--no-daemon", action="store_true")
    p.add_argument("--no-rust", action="store_true")
    p.set_defaults(func=cmd_search)

    # summary
    p = sub.add_parser("summary", help="graph stats + convergence info")
    p.add_argument("--db", default=DEFAULT_DB)
    p.add_argument("--json", action="store_true")
    p.add_argument("--no-daemon", action="store_true")
    p.add_argument("--no-rust", action="store_true")
    p.set_defaults(func=cmd_summary)

    # status
    p = sub.add_parser("status", help="daemon liveness + backend")
    p.add_argument("--out", default=DEFAULT_DB)
    p.set_defaults(func=cmd_status)

    # stop
    p = sub.add_parser("stop", help="shut the daemon down")
    p.add_argument("--out", default=DEFAULT_DB)
    p.set_defaults(func=cmd_stop)

    # mcp
    p = sub.add_parser("mcp", help="run the MCP (Model Context Protocol) stdio server")
    p.add_argument("--db", default=DEFAULT_DB,
                   help="graph DB path (default: ./.dataworm/graph.db)")
    p.add_argument("--no-rust", action="store_true", help="force the Python backend")
    p.set_defaults(func=cmd_mcp)

    # update (legacy: latest distribution) + up (local source rebuild)
    p = sub.add_parser("update", help="upgrade DataWorm to the latest build/version")
    p.add_argument("--from", dest="source", default=None,
                   help="pip install spec (package name, git URL, local path, or index). "
                        "Default: 'dataworm' (latest distribution).")
    p.add_argument("--no-restart", action="store_true",
                   help="upgrade but don't re-launch the worm afterwards")
    p.add_argument("--out", default=DEFAULT_DB,
                   help="daemon port-file db path (default: <cwd>/.dataworm/graph.db)")
    p.set_defaults(func=cmd_update)

    # up — self-update; from GitHub branch (dw up main / dw up dev) or local dir
    p = sub.add_parser("up", help="update: rebuild + reinstall (default: main branch)")
    p.add_argument("branch", nargs="?", default="main",
                   help="git branch to update from when the source is a repo "
                        "(default: main). Examples: dw up main | dw up dev")
    p.add_argument("--from", dest="source", default=None,
                   help="LOCAL source dir override — skips git entirely "
                        "(default: DATAWORM_SOURCE env, ~/.dataworm/source.txt, "
                        "editable metadata, then the well-known checkout)")
    p.set_defaults(func=cmd_up)

    return parser


def main(argv: list[str] | None = None) -> None:
    # Real logging, once per process: INFO to a rotating file (diagnostics for
    # bug reports), WARNING+ to the console (quiet by default).
    setup_logging()
    if argv is None:
        argv = list(sys.argv[1:])
    else:
        argv = [str(a) for a in argv]
    parser = build_parser()

    if not argv:
        # Bare `dw` / `dataworm` in ANY directory = full summon of that
        # directory (the old `init .`): crawl cwd once, watch it, ensure the
        # daemon, open the dashboard. Guard: a zero-args call over PIPED
        # stdin looks programmatic (e.g. JSON-RPC aimed at another
        # entrypoint) — don't silently launch a worm; show help instead.
        try:
            piped = sys.stdin is not None and not sys.stdin.isatty()
        except Exception:  # exotic stdin; treat as interactive
            piped = False
        if piped:
            parser.print_help()
            return
        args = parser.parse_args(["init"])  # exact init defaults, in sync
        args.dir = os.getcwd()
        args.func(args)
        return

    action = next(a for a in parser._actions
                  if isinstance(a, argparse._SubParsersAction))
    known = set(action.choices)
    first = argv[0]
    if first not in known and first not in ("-h", "--help"):
        # `dw .`, `dw <dir>`, `dw --no-web ...` — implicit `init` prefix.
        if first.startswith("-") or os.path.exists(first):
            argv.insert(0, "init")

    args = parser.parse_args(argv)
    try:
        args.func(args)
    except KeyboardInterrupt:
        # Soft close: stop the daemon THIS invocation actually started/used
        # (--db/--out aware), leave user data intact, exit with the
        # conventional 130 (SIGINT) code.
        print("\n[dw] interrupted — shutting down the worm…", file=sys.stderr)
        try:
            from dataworm.server import stop_daemon
            info = stop_daemon(_SESSION_DB or DEFAULT_DB)
            if isinstance(info, dict):
                print("[dw] daemon stopped (%s)" % info.get("pid", "?"),
                      file=sys.stderr)
        except Exception:
            pass
        raise SystemExit(130)


if __name__ == "__main__":
    main()
