from __future__ import annotations

import html
import json
import socket
import struct
import threading
import time
import urllib.parse
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any


FIXTURE_VERSION = "agent-episode-fixture-v1"


def _document(title: str, body: str, script: str = "") -> bytes:
    script_tag = f"<script>{script}</script>" if script else ""
    return (
        "<!doctype html><html><head><meta charset='utf-8'>"
        f"<title>{html.escape(title)}</title>"
        "<style>"
        "body{font-family:sans-serif}"
        "input,button,a{display:block;width:220px;min-height:32px;margin:8px}"
        "output{display:block;white-space:pre-wrap}"
        "</style></head><body>"
        f"{body}{script_tag}</body></html>"
    ).encode("utf-8")


def response_for_agent_path(path: str, query: dict[str, list[str]]) -> bytes | None:
    if path == "/agent/observe-static":
        return _document(
            "observe static",
            "<main>Agent static observation ready</main>"
            "<label>Search <input id='search' placeholder='Search'></label>"
            "<button id='noop'>No operation</button>"
            "<a href='#details'>Details</a>",
        )
    if path == "/agent/fill-reactive-form":
        return _document(
            "reactive form",
            "<main>Reactive form ready</main>"
            "<label>Search <input id='query' placeholder='Search'></label>"
            "<output id='state'>value:;events:</output>",
            """
            globalThis.__agentEpisodeState = { events: [] };
            const input = document.querySelector('#query');
            const output = document.querySelector('#state');
            const render = () => {
              output.textContent = `value:${input.value};events:${__agentEpisodeState.events.join(',')}`;
            };
            input.addEventListener('input', () => {
              __agentEpisodeState.events.push('input');
              queueMicrotask(render);
            });
            input.addEventListener('change', () => {
              __agentEpisodeState.events.push('change');
              render();
            });
            """,
        )
    if path == "/agent/click-same-document":
        return _document(
            "same document click",
            "<main>Same document click ready</main>"
            "<button id='activate'>Activate</button>"
            "<output id='events'>events:</output>",
            """
            globalThis.__agentEpisodeState = { events: [] };
            const button = document.querySelector('#activate');
            const output = document.querySelector('#events');
            for (const type of ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click']) {
              button.addEventListener(type, () => {
                __agentEpisodeState.events.push(type);
                output.textContent = `events:${__agentEpisodeState.events.join(',')}`;
              });
            }
            """,
        )
    if path == "/agent/click-cross-document":
        return _document(
            "cross document click",
            "<main>Cross document source</main>"
            "<button id='navigate'>Open result</button>",
            """
            globalThis.__agentOldRealmMarker = 'cross-document-source';
            document.querySelector('#navigate').addEventListener('click', () => {
              location.href = '/agent/cross-document-result?token=cross-document-result';
            });
            """,
        )
    if path == "/agent/cross-document-result":
        token = html.escape(query.get("token", ["missing"])[0])
        return _document(
            "cross document result",
            f"<main>Cross document result {token}</main>"
            "<button id='result-button'>Result control</button>",
            "globalThis.__agentNewRealmMarker = 'cross-document-result';",
        )
    if path == "/agent/dynamic-controls":
        return _document(
            "dynamic controls",
            "<main>Dynamic controls ready</main>"
            "<section id='controls'><button id='replace'>Replace controls</button></section>"
            "<output id='dynamic-state'>phase:initial</output>",
            """
            const controls = document.querySelector('#controls');
            const output = document.querySelector('#dynamic-state');
            document.querySelector('#replace').addEventListener('click', () => {
              controls.replaceChildren();
              const input = document.createElement('input');
              input.id = 'dynamic-input';
              input.placeholder = 'Dynamic value';
              controls.appendChild(input);
              output.textContent = 'phase:replaced;value:';
              input.addEventListener('input', () => {
                queueMicrotask(() => {
                  output.textContent = `phase:replaced;value:${input.value}`;
                });
              });
            });
            """,
        )
    if path == "/agent/idle-resume":
        return _document(
            "idle resume",
            "<main>Idle episode remains available</main>"
            "<button id='idle-control'>Idle control</button>",
            """
            globalThis.__agentIdleBoot = Date.now();
            globalThis.__agentIdleProbe = () => ({
              boot: globalThis.__agentIdleBoot,
              now: Date.now(),
              href: location.href,
            });
            """,
        )
    if path == "/agent/failed-navigation":
        return _document(
            "failed navigation source",
            "<main>Failure source document</main>"
            "<button id='old-control'>Old document control</button>",
            "globalThis.__agentOldRealmMarker = 'must-retire';",
        )
    if path == "/agent/isolation":
        token = query.get("token", ["missing"])[0]
        escaped = html.escape(token)
        token_json = json.dumps(token).replace("</", "<\\/")
        return _document(
            f"isolation {token}",
            f"<main>Isolation token {escaped}</main>"
            "<label>Value <input id='isolation-input' placeholder='Isolation value'></label>"
            f"<output id='isolation-output'>token:{escaped};value:</output>",
            f"""
            globalThis.__agentIsolationToken = {token_json};
            const input = document.querySelector('#isolation-input');
            const output = document.querySelector('#isolation-output');
            input.addEventListener('input', () => {{
              queueMicrotask(() => {{
                output.textContent = `token:${{__agentIsolationToken}};value:${{input.value}}`;
              }});
            }});
            """,
        )
    return None


class AgentEpisodeFixtureServer:
    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._requests: list[dict[str, Any]] = []
        handler = self._handler_class()
        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.httpd.daemon_threads = True
        self.thread = threading.Thread(
            target=self.httpd.serve_forever,
            name="moli-agent-episode-fixture",
            daemon=True,
        )

    @property
    def base_url(self) -> str:
        host, port = self.httpd.server_address
        return f"http://{host}:{port}"

    @property
    def requests(self) -> list[dict[str, Any]]:
        with self._lock:
            return list(self._requests)

    def url(self, path: str) -> str:
        return f"{self.base_url}/{path.lstrip('/')}"

    def start(self) -> None:
        self.thread.start()

    def stop(self) -> None:
        self.httpd.shutdown()
        self.httpd.server_close()
        self.thread.join(timeout=5)

    def __enter__(self) -> "AgentEpisodeFixtureServer":
        self.start()
        return self

    def __exit__(self, exc_type: object, exc: object, tb: object) -> None:
        self.stop()

    def _record(self, method: str, path: str) -> None:
        with self._lock:
            self._requests.append(
                {"timestamp": time.time(), "method": method, "path": path}
            )

    def _handler_class(self) -> type[BaseHTTPRequestHandler]:
        outer = self

        class Handler(BaseHTTPRequestHandler):
            server_version = "MoliAgentEpisodeFixture/1"

            def log_message(self, _format: str, *_args: object) -> None:
                return

            def do_GET(self) -> None:
                parsed = urllib.parse.urlparse(self.path)
                outer._record("GET", self.path)
                if parsed.path == "/agent/reset-before-response":
                    self.connection.setsockopt(
                        socket.SOL_SOCKET,
                        socket.SO_LINGER,
                        struct.pack("ii", 1, 0),
                    )
                    self.close_connection = True
                    self.connection.close()
                    return
                body = response_for_agent_path(
                    parsed.path,
                    urllib.parse.parse_qs(parsed.query),
                )
                if body is None:
                    self.send_error(HTTPStatus.NOT_FOUND)
                    return
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Cache-Control", "no-store")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                try:
                    self.wfile.write(body)
                except (BrokenPipeError, ConnectionResetError):
                    return

        return Handler
