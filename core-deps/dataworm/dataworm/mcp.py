"""Model Context Protocol (MCP) stdio server for DataWorm.

Any MCP client (Claude Desktop, Cursor, ...) can launch this as a child
process and drive the DataWorm graph through tools. The whole point of the
tool surface is the agent edit-loop: **call ``worm_impact`` BEFORE editing a
file** — an empty direct/transitive blast radius means the edit is safe.

Protocol era support (dual-era per rev 2026-07-28)
---------------------------------------------------
* **Modern clients** (rev 2026-07-28, stateless): every request declares its
  protocol version via ``params._meta["io.modelcontextprotocol/protocolVersion"]``.
  Unsupported versions get ``UnsupportedProtocolVersionError`` (-32022) with
  our supported list. ``server/discover`` is implemented as the revision
  mandates (and doubles as the stdio era probe). List results carry
  ``resultType`` plus ``ttlMs``/``cacheScope`` caching hints; tool order is
  deterministic.
* **Legacy clients** (handshake-based, <= 2025-11-25 — what current client
  releases ship): full ``initialize`` negotiation, echoing the client's
  version when we know it.
Spec sources: modelcontextprotocol.io/specification/2026-07-28 (versioning,
server/discover, transports/stdio, server/tools).

Design constraints
------------------
- stdlib only: JSON-RPC 2.0 over stdin/stdout, no MCP SDK, no new deps.
- Framing: newline-delimited UTF-8 JSON (one message per line, no embedded
  newlines). We are liberal and also accept the legacy ``Content-Length: N``
  header framing if a client sends it.
- Nothing is ever written to stdout except protocol messages; diagnostics go
  to stderr via :mod:`logging`.
- ALL outbound traffic — synchronous responses AND asynchronously pushed
  subscription notifications — flows through a single ordered writer thread
  draining a queue, so a push arriving while the reader is blocked on stdin
  can never wedge behind ``readline()``; EOF drains the queue before exit.
- The server never crashes on bad input: malformed lines get a parse-error
  response (id null) and the loop keeps serving; EOF exits 0.

Testability: :meth:`McpServer._handle` turns one decoded JSON-RPC message into
a response dict (or ``None`` for notifications), so tests drive the server
in-process without a subprocess.
"""

from __future__ import annotations

import json
import logging
import queue
import sys
import threading
import uuid
from typing import Any, Callable

log = logging.getLogger("dataworm.mcp")

# ---- JSON-RPC / MCP error codes --------------------------------------------
ERR_PARSE = -32700
ERR_INVALID_REQUEST = -32600
ERR_METHOD_NOT_FOUND = -32601
ERR_INVALID_PARAMS = -32602
ERR_INTERNAL = -32603

# MCP spec reserved range (2026-07-28 allocated -32020..-32099 for the spec;
# -32000..-32019 stay implementation-defined).
ERR_UNSUPPORTED_PROTOCOL_VERSION = -32022

# ---- protocol eras (MCP "Versioning and Compatibility", rev 2026-07-28) -----
#
# Revision 2026-07-28 removed the initialize handshake: MCP is stateless and
# every request carries its protocol version in
# ``params._meta["io.modelcontextprotocol/protocolVersion"]``. This server is
# DUAL-ERA per the spec's compatibility model:
#
#   * modern clients (any request carrying a modern ``_meta`` version) are
#     served statelessly under revision 2026-07-28 semantics;
#   * legacy clients (initialize handshake, <= 2025-11-25 — what current
#     Claude Desktop / Cursor releases ship) get legacy semantics, including
#     the echo-the-client's-version negotiate;
#   * every server implements ``server/discover`` (mandatory in this
#     revision), which doubles as the stdio backward-compatibility probe.
#
# Only revisions we can genuinely serve STATELESSLY are advertised to modern
# clients — advertising a handshake-era version there would invite a modern
# client to speak it statelessly, which it cannot.
LATEST_VERSION = "2026-07-28"
MODERN_SUPPORTED_VERSIONS: list[str] = [LATEST_VERSION]
# Legacy initialize negotiates against everything handshake-era we understand.
LEGACY_SUPPORTED_VERSIONS: list[str] = [
    "2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05",
]
DEFAULT_LEGACY_VERSION = "2025-06-18"

META_VERSION_KEY = "io.modelcontextprotocol/protocolVersion"
META_SERVERINFO_KEY = "io.modelcontextprotocol/serverInfo"

# tools/list caching hint (CacheableResult, rev 2026-07-28): the catalog is
# static for the process lifetime, so clients may cache it aggressively.
TOOLS_TTL_MS = 300_000

# ---- Reflex Arc: reverse channel (push) --------------------------------------
# Advertised in DiscoverResult.capabilities.extensions; lets an AI agent RECEIVE
# change reports instead of only polling for them.
SUBSCRIPTION_TYPE_CHANGE = "io.dataworm/change"
NOTIFICATION_METHOD_CHANGE = "notifications/dataworm/change"
META_SUBSCRIPTION_ID_KEY = "io.modelcontextprotocol/subscriptionId"
EXTENSION_CHANGES = "io.dataworm/changes"

# Kinds a change REPORT carries (the report's own ``kind`` field — distinct
# from the bus envelope's ``"change"`` topic and from the watcher's ``fs_*``
# signal kinds).
CHANGE_REPORT_KINDS = frozenset({"created", "modified", "deleted", "moved"})

# Outbound backpressure: once more than this many messages sit queued for a
# slow client, pushed notifications drop OLDEST-first. Producers (the crawler
# thread emitting on the bus) never block.
MAX_PENDING_NOTIFICATIONS = 1000

SERVER_NAME = "dataworm"


def _package_version() -> str:
    try:
        from dataworm import __version__

        return __version__
    except Exception:  # pragma: no cover - defensive only
        return "0.0.0"


# ---- tool argument helpers --------------------------------------------------

class InvalidParams(ValueError):
    """Raised when a tools/call request is missing/malforming arguments."""


def _require(args: dict[str, Any], key: str) -> Any:
    value = args.get(key)
    if value is None or (isinstance(value, str) and not value.strip()):
        raise InvalidParams(f"missing required argument '{key}'")
    return value


# ---- tool dispatch (name -> op on Core) ------------------------------------
# Each entry maps an MCP tool onto an existing Core.call() method name.

def _op_crawl(server: McpServer, args: dict[str, Any]) -> dict:
    root = _require(args, "root")
    params: dict[str, Any] = {"root": str(root)}
    if args.get("max_cycles") is not None:
        params["max_cycles"] = int(args["max_cycles"])
    return server.core.call("crawl", params)


def _op_impact(server: McpServer, args: dict[str, Any]) -> dict:
    return server.core.call("impact", {"path": str(_require(args, "path"))})


def _op_context(server: McpServer, args: dict[str, Any]) -> dict:
    return server.core.call("context", {"path": str(_require(args, "path"))})


def _op_neighbors(server: McpServer, args: dict[str, Any]) -> dict:
    params: dict[str, Any] = {"path": str(_require(args, "path"))}
    depth = args.get("depth")
    if depth is not None:
        params["depth"] = int(depth)
    edge_type = args.get("edge_type")
    if edge_type is not None:
        # Core's neighbors takes a list of EdgeType value strings.
        params["types"] = [str(edge_type)]
    return server.core.call("neighbors", params)


def _op_search(server: McpServer, args: dict[str, Any]) -> dict:
    # Core's search param is called "text"; MCP callers say "query".
    query = args.get("query", args.get("text"))
    if query is None or (isinstance(query, str) and not query.strip()):
        raise InvalidParams("missing required argument 'query'")
    params: dict[str, Any] = {"text": str(query)}
    limit = args.get("limit")
    if limit is not None:
        params["limit"] = int(limit)
    return server.core.call("search", params)


def _op_summary(server: McpServer, args: dict[str, Any]) -> dict:
    return server.core.call("summary", {})


def _op_watch(server: McpServer, args: dict[str, Any]) -> dict:
    return server.core.call("watch", {"root": str(_require(args, "root"))})


def _op_unwatch(server: McpServer, args: dict[str, Any]) -> dict:
    return server.core.call("unwatch", {"root": str(_require(args, "root"))})


def _op_changes(server: McpServer, args: dict[str, Any]) -> dict:
    # Reflex Arc polling: straight pass-through to Core's "changes" op
    # (-> {"changes": [<report>...], "last_seq": int}; pagination is taught
    # in the tool description).
    params: dict[str, Any] = {}
    if args.get("since_seq") is not None:
        params["since_seq"] = int(args["since_seq"])
    if args.get("limit") is not None:
        params["limit"] = int(args["limit"])
    return server.core.call("changes", params)


def _op_plan_edit(server: McpServer, args: dict[str, Any]) -> dict:
    # What-if simulator: dry-run an edit's blast radius without touching disk.
    content = args.get("content")
    if not isinstance(content, str) or not content.strip():
        raise InvalidParams("missing required argument 'content' (string)")
    return server.core.call("plan_edit", {
        "path": str(_require(args, "path")),
        "content": content,
    })


_TOOL_OPS: dict[str, Callable[[McpServer, dict[str, Any]], dict]] = {
    "worm_crawl": _op_crawl,
    "worm_impact": _op_impact,
    "worm_context": _op_context,
    "worm_neighbors": _op_neighbors,
    "worm_search": _op_search,
    "worm_summary": _op_summary,
    "worm_watch": _op_watch,
    "worm_unwatch": _op_unwatch,
    # Deterministic position: appended last.
    "worm_changes": _op_changes,
    "worm_plan_edit": _op_plan_edit,
}


def _schema(
    properties: dict[str, dict],
    required: list[str],
) -> dict:
    return {
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": False,
    }


def build_tool_catalog() -> list[dict]:
    """The advertised tool catalog (name/description/inputSchema per tool)."""
    return [
        {
            "name": "worm_crawl",
            "description": (
                "Build or refresh the DataWorm directory graph for a root dir "
                "(dependency/reference/duplicate/similarity edges). Call once before any "
                "query tool, and again after large structural changes."
            ),
            "inputSchema": _schema(
                {
                    "root": {"type": "string", "description": "Directory to crawl."},
                    "max_cycles": {
                        "type": "integer",
                        "description": "Optional realignment cycle cap (default 5).",
                    },
                },
                ["root"],
            ),
        },
        {
            "name": "worm_impact",
            "description": (
                "Blast-radius check: call BEFORE every edit. Returns files that depend on "
                "`path` directly + transitively. Empty direct/transitive means safe to edit; "
                "non-empty means those dependents may break — read them or confirm first."
            ),
            "inputSchema": _schema(
                {
                    "path": {
                        "type": "string",
                        "description": "File path to check (absolute, root-relative, or bare id).",
                    },
                },
                ["path"],
            ),
        },
        {
            "name": "worm_context",
            "description": (
                "Full context bundle for one file: metadata, links across all dimensions, "
                "and its impact. Call before editing to understand what a file is and touches."
            ),
            "inputSchema": _schema(
                {
                    "path": {"type": "string", "description": "File path to inspect."},
                },
                ["path"],
            ),
        },
        {
            "name": "worm_neighbors",
            "description": (
                "Graph neighborhood: nodes within N hops of `path`, optionally filtered by "
                "edge type (references | similar_to | duplicate_of | contains)."
            ),
            "inputSchema": _schema(
                {
                    "path": {"type": "string", "description": "File path to start from."},
                    "depth": {"type": "integer", "description": "Hops to traverse (default 1)."},
                    "edge_type": {
                        "type": "string",
                        "description": "Optional dimension filter: references | similar_to | "
                                       "duplicate_of | contains.",
                    },
                },
                ["path"],
            ),
        },
        {
            "name": "worm_search",
            "description": (
                "Substring search over indexed node paths; find candidate files before "
                "running impact/context/neighbors on them."
            ),
            "inputSchema": _schema(
                {
                    "query": {"type": "string", "description": "Substring to match against paths."},
                    "limit": {"type": "integer", "description": "Max hits (default 50)."},
                },
                ["query"],
            ),
        },
        {
            "name": "worm_summary",
            "description": (
                "Whole-graph stats: nodes, edges per dimension, convergence. Cheap sanity "
                "check that a graph exists/is populated for this workspace."
            ),
            "inputSchema": _schema({}, []),
        },
        {
            "name": "worm_watch",
            "description": (
                "Start live watching of a root: filesystem changes trigger incremental "
                "re-crawls so impact results stay fresh while you work."
            ),
            "inputSchema": _schema(
                {
                    "root": {"type": "string", "description": "Directory to watch."},
                },
                ["root"],
            ),
        },
        {
            "name": "worm_unwatch",
            "description": "Stop live watching of a previously watched root.",
            "inputSchema": _schema(
                {
                    "root": {"type": "string", "description": "Directory to stop watching."},
                },
                ["root"],
            ),
        },
        {
            "name": "worm_changes",
            "description": (
                "Change journal (pull half of the Reflex Arc): file change reports "
                "(created/modified/deleted/moved) since `since_seq`. Returns "
                '{"changes": [...], "last_seq": N}; paginate by passing last_seq '
                "back as since_seq until a page comes back empty. For live updates "
                "prefer subscribing via subscriptions/listen instead."
            ),
            "inputSchema": _schema(
                {
                    "since_seq": {
                        "type": "integer",
                        "description": "Return reports strictly newer than this journal "
                                       "sequence (default 0).",
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max reports per page.",
                    },
                },
                [],
            ),
        },
        {
            "name": "worm_plan_edit",
            "description": (
                "What-if edit simulator (dry run — NEVER writes disk): given a path and "
                "PROPOSED content, returns the links it would gain/lose, new dangling "
                "references, dependents count, and duplication radar (exact + near twins "
                "of the proposed bytes). Call BEFORE worm_crawl-ing an edit to preview its "
                "graph consequences; unchanged content short-circuits."
            ),
            "inputSchema": _schema(
                {
                    "path": {
                        "type": "string",
                        "description": "Target file path (existing or brand-new).",
                    },
                    "content": {
                        "type": "string",
                        "description": "The full PROPOSED file content to simulate.",
                    },
                },
                ["path", "content"],
            ),
        },
    ]


TOOL_CATALOG: list[dict] = build_tool_catalog()


# ---- the server -------------------------------------------------------------

class McpServer:
    """MCP stdio front-end wrapping a lazily-created :class:`dataworm.core.Core`.

    Parameters match CLI conventions: ``db_path`` defaults to the project's
    ``DEFAULT_DB`` (``.dataworm/graph.db`` relative to cwd) and ``prefer_rust``
    to True (pass False for ``--no-rust`` behaviour). The Core (and its DB
    load) is created lazily on first tool call, so ``initialize``/``tools/list``
    handshakes are instant and side-effect free.
    """

    def __init__(self, db_path: str | None = None, prefer_rust: bool = True) -> None:
        if db_path is None:
            from dataworm.core import DEFAULT_DB

            db_path = DEFAULT_DB
        self.db_path = db_path
        self.prefer_rust = prefer_rust
        self._core: Any = None
        # ---- outbound writer state (lazily initialized) -----------------------
        self._outbound: "queue.Queue | None" = None
        self._outbound_lock = threading.Lock()
        self._writer_thread: threading.Thread | None = None
        self._output_dead = threading.Event()
        # ---- Reflex Arc subscription state -------------------------------------
        self._subscriptions: dict[str, set[str]] = {}
        self._subs_lock = threading.Lock()
        self._bus_lock = threading.Lock()
        self._bus_hooked = False

    @property
    def core(self):
        """The wrapped Core, built on first use."""
        if self._core is None:
            from dataworm.core import Core

            log.info("creating Core db=%s prefer_rust=%s", self.db_path, self.prefer_rust)
            self._core = Core(db_path=self.db_path, prefer_rust=self.prefer_rust)
        return self._core

    # -- single-message handler (pure; the unit-test seam) -------------------

    def _handle(self, msg: Any) -> dict | None:
        """Handle one decoded JSON-RPC message. Returns a response dict, or
        ``None`` when the message was a notification (no reply allowed)."""
        if not isinstance(msg, dict):
            return self._error(None, ERR_INVALID_REQUEST, "invalid request: not an object")

        method = msg.get("method")
        has_id = "id" in msg
        msg_id = msg.get("id")

        # Notifications (no id) never get a reply — including unknown ones.
        if not has_id:
            log.debug("notification %s (no response)", method)
            return None

        try:
            if method == "server/discover":
                # Mandatory in rev 2026-07-28; also the stdio era probe. It
                # advertises regardless of _meta so probes always succeed.
                return self._ok(msg_id, self._on_discover())

            era, requested = self._requested_version(msg)
            if era == "modern" and requested not in MODERN_SUPPORTED_VERSIONS:
                return {
                    "jsonrpc": "2.0",
                    "id": msg_id,
                    "error": {
                        "code": ERR_UNSUPPORTED_PROTOCOL_VERSION,
                        "message": f"Unsupported protocol version: {requested}",
                        "data": {
                            "supported": list(MODERN_SUPPORTED_VERSIONS),
                            "requested": requested,
                        },
                    },
                }

            if method == "initialize":
                # Legacy-era handshake (<= 2025-11-25 clients). Kept exactly
                # legacy-shaped so old clients see nothing new.
                return self._ok(msg_id, self._on_initialize(msg.get("params")))
            if method == "tools/list":
                return self._ok(msg_id, {
                    "tools": TOOL_CATALOG,          # static order == deterministic
                    "resultType": "complete",
                    "ttlMs": TOOLS_TTL_MS,
                    "cacheScope": "private",
                })
            if method == "tools/call":
                return self._ok(msg_id, self._on_tools_call(msg.get("params")))
            if method == "subscriptions/listen":
                # Reflex Arc push channel (advertised via server/discover
                # capabilities.extensions): registers this connection for bus
                # "change" events. Legacy-era clients may use it too — the
                # extension is era-agnostic on our side.
                return self._ok(
                    msg_id, self._on_subscriptions_listen(msg.get("params"))
                )
            # Known-but-notification-only methods arriving WITH an id are still
            # treated as notifications-style no-ops? No: with an id they are
            # requests we don't implement -> -32601 below covers everything.
            raise _UnknownMethod(method)
        except _UnknownMethod as exc:
            return self._error(msg_id, ERR_METHOD_NOT_FOUND, f"method not found: {exc}")
        except InvalidParams as exc:
            return self._error(msg_id, ERR_INVALID_PARAMS, str(exc))
        except Exception as exc:  # noqa: BLE001 - never crash the loop
            log.exception("handler failed for %s", method)
            return self._error(msg_id, ERR_INTERNAL, f"{type(exc).__name__}: {exc}")

    # -- request bodies -------------------------------------------------------

    @staticmethod
    def _requested_version(msg: dict) -> tuple[str, str | None]:
        """Classify a message as ("modern", version) or ("legacy", None).

        Modern = the request itself declares its protocol version via
        ``params._meta["io.modelcontextprotocol/protocolVersion"]`` (rev
        2026-07-28 stateless model). Anything else is treated as legacy-era
        traffic and routed to handshake semantics; being liberal here keeps
        un-versioned simple clients working while never mis-serving a modern
        one.
        """
        params = msg.get("params")
        if isinstance(params, dict):
            meta = params.get("_meta")
            if isinstance(meta, dict):
                version = meta.get(META_VERSION_KEY)
                if isinstance(version, str) and version.strip():
                    return "modern", version.strip()
        return "legacy", None

    def _on_discover(self) -> dict:
        """DiscoverResult (rev 2026-07-28): identity, capabilities, versions."""
        return {
            "resultType": "complete",
            "supportedVersions": list(MODERN_SUPPORTED_VERSIONS),
            "capabilities": {
                "tools": {},
                # Spec extensions field: advertises the Reflex Arc push channel.
                "extensions": {EXTENSION_CHANGES: {}},
            },
            "_meta": {
                META_SERVERINFO_KEY: {
                    "name": SERVER_NAME,
                    "version": _package_version(),
                },
            },
            "instructions": (
                "DataWorm indexes a code tree into a dependency graph. Before editing any "
                "file, call worm_impact on it: empty direct/transitive blast radius means "
                "the edit is safe. worm_crawl builds/refreshes the graph. For live updates "
                "subscribe via subscriptions/listen with types [\"io.dataworm/change\"] to "
                "receive pushed notifications/dataworm/change messages as watched files "
                "change (unsubscribe with the returned subscriptionId), or poll "
                "worm_changes and paginate via last_seq."
            ),
            "ttlMs": 3_600_000,
            "cacheScope": "private",
        }

    @staticmethod
    def _on_initialize(params: Any) -> dict:
        if params is None:
            params = {}
        if not isinstance(params, dict):
            raise InvalidParams("initialize.params must be an object")
        client_version = params.get("protocolVersion")
        if (
            isinstance(client_version, str)
            and client_version.strip()
            and client_version not in LEGACY_SUPPORTED_VERSIONS
        ):
            # A legacy-era client proposed a handshake revision we don't know.
            # Echoing would promise semantics we can't verify; naming our
            # supported versions gives it the only diagnostic it can surface
            # (legacy clients have no fall-forward mechanism per the spec).
            raise InvalidParams(
                f"unsupported protocolVersion {client_version}; "
                f"supported: {', '.join(LEGACY_SUPPORTED_VERSIONS)}"
            )
        version = (
            client_version
            if isinstance(client_version, str) and client_version.strip()
            else DEFAULT_LEGACY_VERSION
        )
        return {
            "protocolVersion": version,
            "capabilities": {"tools": {}},
            "serverInfo": {
                "name": SERVER_NAME,
                "version": _package_version(),
            },
            "instructions": (
                "DataWorm indexes a code tree into a dependency graph. Before editing any "
                "file, call worm_impact on it: empty direct/transitive blast radius means "
                "the edit is safe. worm_crawl builds/refreshes the graph."
            ),
        }

    def _on_tools_call(self, params: Any) -> dict:
        if params is None:
            params = {}
        if not isinstance(params, dict):
            raise InvalidParams("tools/call params must be an object")
        name = params.get("name")
        if not isinstance(name, str) or not name:
            raise InvalidParams("tools/call requires a tool 'name' string")
        arguments = params.get("arguments") or {}
        if not isinstance(arguments, dict):
            raise InvalidParams("tools/call arguments must be an object")

        op = _TOOL_OPS.get(name)
        if op is None:
            raise InvalidParams(f"unknown tool: {name}")

        try:
            payload = op(self, arguments)
        except InvalidParams:
            raise
        except Exception as exc:  # noqa: BLE001 - tool failure != server crash
            payload = {"error": f"{type(exc).__name__}: {exc}"}

        if isinstance(payload, dict) and "error" in payload:
            text = json.dumps(payload, default=str)
            log.warning("tool %s failed: %s", name, payload["error"])
            return {"content": [{"type": "text", "text": text}], "isError": True,
                    "resultType": "complete"}
        text = json.dumps(payload, default=str)
        return {"content": [{"type": "text", "text": text}],
                "resultType": "complete"}

    # -- envelope helpers ------------------------------------------------------

    @staticmethod
    def _ok(msg_id: Any, result: Any) -> dict:
        return {"jsonrpc": "2.0", "id": msg_id, "result": result}

    @staticmethod
    def _error(msg_id: Any, code: int, message: str) -> dict:
        return {
            "jsonrpc": "2.0",
            "id": msg_id,
            "error": {"code": code, "message": message},
        }

    # -- line-level plumbing ----------------------------------------------------

    def _handle_line(self, line: str) -> dict | None:
        """Decode one raw input line and handle it."""
        try:
            msg = json.loads(line)
        except ValueError as exc:
            return self._error(None, ERR_PARSE, f"parse error: {exc}")
        return self._handle(msg)

    # -- outbound writer: one ordered queue -> stdout ----------------------------
    #
    # Pushed notifications originate on OTHER threads (the crawler emitting on
    # the bus), so responses can no longer be written inline by the reading
    # thread: interleaved writes would corrupt framing, and a notification
    # arriving while nobody is feeding stdin would deadlock behind readline().
    # Every outbound message therefore goes into ONE FIFO queue drained by ONE
    # writer thread — preserving ordering, newline framing per message, and
    # flush-per-message.

    def _activate_outbound(self) -> "queue.Queue":
        """Create the outbound queue on demand (tests may enqueue without a
        running writer thread; serve() always activates it)."""
        with self._outbound_lock:
            if self._outbound is None:
                self._outbound = queue.Queue()
            return self._outbound

    def _start_writer(self, writer: Any) -> None:
        q = self._activate_outbound()
        with self._outbound_lock:
            if self._writer_thread is not None and self._writer_thread.is_alive():
                return  # already serving an earlier connection
            self._writer_thread = threading.Thread(
                target=self._writer_loop, args=(writer, q),
                name="dataworm-mcp-outbound", daemon=True,
            )
            self._writer_thread.start()

    def _writer_loop(self, writer: Any, q: "queue.Queue") -> None:
        """Drain the queue onto the client. One JSON message per line, flushed."""
        while True:
            item = q.get()
            if item is None:  # sentinel: queue fully drained -> clean exit
                return
            try:
                # ensure_ascii keeps us safe on non-UTF8 Windows consoles.
                writer.write(item + "\n")
                writer.flush()
            except (BrokenPipeError, OSError, ValueError):
                # Client closed its end mid-write: stop writing, discard the
                # backlog silently, and keep consuming until the sentinel so
                # producers never block on their way out.
                self._output_dead.set()
                while q.get() is not None:
                    pass
                return

    def _shutdown_writer(self) -> None:
        """Signal EOF to the writer and wait until the queue is fully drained."""
        q = self._outbound
        thread = self._writer_thread
        if q is not None and thread is not None and thread.is_alive():
            q.put(None)
            thread.join(timeout=5.0)

    def _enqueue_response(self, line: str) -> None:
        """Queue a synchronous response (produced by the reading thread)."""
        if self._output_dead.is_set():
            return
        self._activate_outbound().put(line)

    def _enqueue_notification(self, line: str) -> None:
        """Queue a PUSHED notification (produced by bus/crawler threads).

        Never blocks, never raises: under backpressure (> MAX_PENDING already
        queued for a slow client) the OLDEST queued message is dropped to admit
        the newest — sampled delivery beats a stalled crawler.
        """
        if self._output_dead.is_set():
            return
        q = self._activate_outbound()
        try:
            while q.qsize() > MAX_PENDING_NOTIFICATIONS:
                try:
                    q.get_nowait()  # drop oldest
                    log.warning(
                        "outbound backlog exceeds %d messages; dropping the "
                        "oldest pushed notification", MAX_PENDING_NOTIFICATIONS,
                    )
                except queue.Empty:
                    break
            q.put(line)
        except Exception:  # pragma: no cover - unbounded put cannot fail today
            log.exception("dropping pushed notification: enqueue failed")

    # -- Reflex Arc subscriptions --------------------------------------------------

    @staticmethod
    def _as_change_report(ev: Any) -> dict | None:
        """Extract the change-report dict from a bus event, or ``None``.

        Core publishes change reports on the bus; depending on how the emitter
        works around the report carrying its own ``seq``/``kind`` (a literal
        ``bus.emit("change", **report)`` would raise ``TypeError``), the event
        arrives in one of three shapes, all accepted here:

        1. flattened      — ``bus.emit(**report)``: the dict IS the report;
        2. nested         — ``bus.emit("change", change=report)``;
        3. hybrid         — ``bus.emit("change", **rest_of_report)``: envelope
           keeps its own ``seq``/``kind="change"``, the remaining report keys
           sit beside it.

        Discriminators (``source == "fs_event"`` + string ``path``, plus a
        report ``kind`` or an explicit ``"change"`` envelope) collide with no
        other event emitted today: crawl-lifecycle events carry no ``source``,
        and the watcher signals use ``fs_*`` kinds.
        """
        if not isinstance(ev, dict):
            return None
        kind = ev.get("kind")
        if kind == "change":
            for envelope_key in ("change", "report"):
                # core.py emits emit("change", report=report) — the bus-level
                # kind collides with the report's own kind, so the payload
                # rides under "report"; accept both envelope spellings.
                nested = ev.get(envelope_key)
                if isinstance(nested, dict):
                    return nested
            if ev.get("source") == "fs_event" and isinstance(ev.get("path"), str):
                return ev  # hybrid: report body spread beside the envelope
            return None
        if (
            ev.get("source") == "fs_event"
            and kind in CHANGE_REPORT_KINDS
            and isinstance(ev.get("path"), str)
        ):
            return ev
        return None

    @staticmethod
    def _format_change_notification(subscription_id: str, report: dict) -> str:
        """One wire-ready notification line for one subscriber."""
        return json.dumps(
            {
                "jsonrpc": "2.0",
                "method": NOTIFICATION_METHOD_CHANGE,
                "params": {
                    "_meta": {META_SUBSCRIPTION_ID_KEY: subscription_id},
                    "change": report,
                },
            },
            separators=(",", ":"),
            default=str,
        )

    def _install_bus_hook(self) -> None:
        """Lazily subscribe to the wrapped Core's EventBus (first listen).

        Runs BEFORE the subscription becomes visible so a change emitted right
        after a client sees its ack can never fall into a hookless window.
        """
        with self._bus_lock:
            if self._bus_hooked:
                return
            try:
                bus = getattr(self.core, "bus", None)
            except Exception:
                log.exception("accessing core.bus failed; push stays silent")
                return
            if bus is None:
                log.warning("core exposes no event bus; push subscriptions stay silent")
                return
            bus.subscribe(self._on_bus_event)
            self._bus_hooked = True

    def _on_bus_event(self, ev: Any) -> None:
        """EventBus callback — runs on the crawler/watcher thread.

        Contract: never raises, never blocks. Fans out one formatted line per
        active matching subscription into the outbound queue; silently no-ops
        when there are no subscribers (or none of the matching type).
        """
        try:
            report = self._as_change_report(ev)
            if report is None:
                return
            with self._subs_lock:
                targets = [
                    sid for sid, types in self._subscriptions.items()
                    if SUBSCRIPTION_TYPE_CHANGE in types
                ]
            for sid in targets:
                self._enqueue_notification(
                    self._format_change_notification(sid, report)
                )
        except Exception:  # noqa: BLE001 - a push failure must never hit the crawl
            log.exception("delivering change notification failed")

    def _on_subscriptions_listen(self, params: Any) -> dict:
        """Handle a ``subscriptions/listen`` request (rev 2026-07-28 model)."""
        if params is None:
            params = {}
        if not isinstance(params, dict):
            raise InvalidParams("subscriptions/listen.params must be an object")

        # Unsubscribe semantics: a second call carrying {"unsubscribe": id}
        # tears that stream down.
        unsubscribe = params.get("unsubscribe")
        if unsubscribe is not None:
            if not isinstance(unsubscribe, str) or not unsubscribe:
                raise InvalidParams("'unsubscribe' must be a subscriptionId string")
            with self._subs_lock:
                if unsubscribe not in self._subscriptions:
                    raise InvalidParams(f"unknown subscriptionId: {unsubscribe}")
                del self._subscriptions[unsubscribe]
            log.info("subscription %s unsubscribed", unsubscribe)
            return {
                "resultType": "complete",
                "subscriptionId": unsubscribe,
                "unsubscribed": True,
            }

        types_param = params.get("types")
        if types_param is None:
            types = [SUBSCRIPTION_TYPE_CHANGE]
        else:
            if (
                not isinstance(types_param, list)
                or not all(isinstance(t, str) and t.strip() for t in types_param)
            ):
                raise InvalidParams("'types' must be a list of event-type strings")
            # Unknown types are ACKED, not rejected: acceptedTypes echoes the
            # set this connection will actually receive.
            types = list(dict.fromkeys(types_param))

        # Install the bus hook first (see its docstring), then publish the
        # registration under the lock.
        self._install_bus_hook()
        subscription_id = f"sub-{uuid.uuid4().hex[:16]}"
        with self._subs_lock:
            self._subscriptions[subscription_id] = set(types)
        log.info("subscription %s accepted types=%s", subscription_id, types)
        return {
            "resultType": "complete",
            "subscriptionId": subscription_id,
            "acceptedTypes": types,
        }

    # -- stdio loop --------------------------------------------------------------

    def serve(self, stdin: Any = None, stdout: Any = None) -> int:
        """Serve newline-delimited JSON-RPC until stdin closes. Returns 0.

        Inbound lines are read on the calling thread exactly as before; ALL
        outbound traffic — synchronous responses AND asynchronously pushed
        subscription notifications — funnels through a single ordered writer
        thread (see the outbound-writer notes above), so a push arriving while
        no request is in flight can never wedge behind ``readline()``.
        Diagnostics go to stderr only; stdout carries protocol messages, each
        flushed individually. On EOF the outbound queue is fully drained before
        exit; a broken pipe exits cleanly like a good child process.
        """
        if not logging.getLogger().handlers:
            logging.basicConfig(stream=sys.stderr, level=logging.WARNING)
        reader = stdin if stdin is not None else sys.stdin
        writer = stdout if stdout is not None else sys.stdout
        if reader is None or writer is None:
            return 0
        self._start_writer(writer)
        try:
            while True:
                line = self._read_message(reader)
                if line is None:
                    return 0  # EOF / stdin closed -> drain queue, exit 0
                resp = self._handle_line(line)
                if resp is not None:
                    self._enqueue_response(json.dumps(resp, separators=(",", ":")))
        except KeyboardInterrupt:  # pragma: no cover - interactive Ctrl+C
            return 0
        except (BrokenPipeError, OSError, ValueError):
            # Client vanished; the writer thread discards the rest quietly.
            return 0
        finally:
            self._shutdown_writer()

    @staticmethod
    def _read_message(reader: Any) -> str | None:
        """Read the next message from ``reader``.

        Primary framing: one JSON object per newline-delimited line. Liberal
        extra: LSP-style ``Content-Length: N`` headers followed by N chars of
        body (some early/exotic clients send this); detected trivially and
        consumed transparently. Returns ``None`` on EOF.
        """
        while True:
            line = reader.readline()
            if not line:
                return None
            stripped = line.strip()
            if not stripped:
                continue  # tolerate stray blank lines between messages
            if stripped.lower().startswith(b"content-length:" if isinstance(stripped, bytes)
                                           else "content-length:"):
                length = McpServer._consume_headers(reader, stripped)
                if length is None:
                    continue  # unparseable header block; skip and keep serving
                body = reader.read(length)
                if not body:
                    return None
                return body.strip() if isinstance(body, str) else body.decode("utf-8", "replace").strip()
            if isinstance(line, bytes):
                stripped = line.decode("utf-8", "replace").strip()
            return stripped

    @staticmethod
    def _consume_headers(reader: Any, first_line: str) -> int | None:
        """Parse a Content-Length header block; return the declared length."""
        length: int | None = None
        line = first_line
        while line:
            key, _, value = line.partition(":")
            if key.strip().lower() == "content-length":
                try:
                    length = int(value.strip())
                except ValueError:
                    return None
            line = reader.readline()
            if not line:
                break
            line = line.strip()
            if isinstance(line, bytes):
                line = line.decode("utf-8", "replace")
        return length


class _UnknownMethod(Exception):
    pass


def run_mcp(db_path: str | None = None, prefer_rust: bool = True) -> int:
    """Entry point used by the CLI: construct the server and serve stdio."""
    return McpServer(db_path=db_path, prefer_rust=prefer_rust).serve()


if __name__ == "__main__":  # pragma: no cover - manual smoke: python -m dataworm.mcp
    sys.exit(run_mcp())
