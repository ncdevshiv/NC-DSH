#!/usr/bin/env python3
from __future__ import annotations

import argparse
import asyncio
import json
import os
import signal
import socket
import subprocess
import sys
import threading
import time
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

try:
    import websockets
    from websockets.asyncio.client import ClientConnection
except ImportError as error:  # pragma: no cover - CLI setup error.
    raise SystemExit(
        "missing dependency: websockets\n"
        "run: uv run --with websockets python bilibili_cdp_demo.py"
    ) from error


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_KEYWORD = "猛禽峡谷"
DEFAULT_HOME_URL = "https://www.bilibili.com/"
DEFAULT_USER_AGENT = (
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/136.0 Safari/537.36"
)


class DemoError(RuntimeError):
    pass


def reserve_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def moli_binary(override: str | None) -> Path:
    if override:
        return Path(override).expanduser().resolve()
    candidates = [
        REPO_ROOT / "target" / "debug" / "moli",
        REPO_ROOT / "target" / "release" / "moli",
    ]
    existing = [candidate for candidate in candidates if candidate.exists()]
    if not existing:
        raise DemoError("missing moli binary; run `cargo build -p moli` or pass --moli-bin")
    return max(existing, key=lambda path: path.stat().st_mtime)


def read_json_url_no_proxy(url: str) -> dict[str, Any]:
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(url, timeout=2) as response:
        payload = json.loads(response.read().decode("utf-8"))
    if not isinstance(payload, dict):
        raise DemoError(f"unexpected JSON response from {url}: {payload!r}")
    return payload


@dataclass
class MoliServe:
    process: subprocess.Popen[str]
    endpoint: str
    logs: list[str]
    threads: list[threading.Thread]


def _drain_stream(stream: Any, label: str, logs: list[str]) -> None:
    if stream is None:
        return
    for line in stream:
        text = line.rstrip()
        logs.append(f"{label}: {text}")
        if os.environ.get("MOLI_PLAYGROUND_TRACE_BG") == "1":
            print(f"[moli serve {label}] {text}", file=sys.stderr, flush=True)


def start_moli(args: argparse.Namespace) -> MoliServe:
    port = args.port if args.port else reserve_port()
    endpoint = f"http://{args.host}:{port}"
    binary = moli_binary(args.moli_bin)
    command = [
        str(binary),
        "serve",
        "--host",
        args.host,
        "--port",
        str(port),
        "--user-agent",
        args.user_agent,
    ]
    if args.http_proxy:
        command.extend(["--http-proxy", args.http_proxy])
    if args.profile_dir:
        command.extend(["--profile-dir", str(Path(args.profile_dir).expanduser())])

    process = subprocess.Popen(
        command,
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )
    logs: list[str] = []
    threads = [
        threading.Thread(target=_drain_stream, args=(process.stdout, "stdout", logs), daemon=True),
        threading.Thread(target=_drain_stream, args=(process.stderr, "stderr", logs), daemon=True),
    ]
    for thread in threads:
        thread.start()

    deadline = time.monotonic() + args.startup_timeout
    last_error: BaseException | None = None
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise DemoError(f"moli serve exited rc={process.returncode}: {'; '.join(logs[-20:])}")
        try:
            read_json_url_no_proxy(endpoint + "/json/version")
            return MoliServe(process=process, endpoint=endpoint, logs=logs, threads=threads)
        except BaseException as error:  # noqa: BLE001 - reported if startup times out.
            last_error = error
            time.sleep(0.05)
    stop_moli(MoliServe(process=process, endpoint=endpoint, logs=logs, threads=threads))
    raise DemoError(f"timed out waiting for {endpoint}/json/version; last_error={last_error!r}")


def stop_moli(serve: MoliServe | None) -> None:
    if serve is None:
        return
    if serve.process.poll() is None:
        try:
            os.killpg(serve.process.pid, signal.SIGTERM)
        except OSError:
            pass
        try:
            serve.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(serve.process.pid, signal.SIGKILL)
            except OSError:
                pass
            serve.process.wait(timeout=3)
    for thread in serve.threads:
        thread.join(timeout=0.2)


@dataclass
class RawCdpClient:
    websocket: ClientConnection
    next_id: int = 1

    async def send(self, method: str, params: dict[str, Any] | None = None, *, session_id: str | None = None) -> int:
        message_id = self.next_id
        self.next_id += 1
        message: dict[str, Any] = {"id": message_id, "method": method}
        if params is not None:
            message["params"] = params
        if session_id is not None:
            message["sessionId"] = session_id
        await self.websocket.send(json.dumps(message, separators=(",", ":")))
        return message_id

    async def recv(self) -> dict[str, Any]:
        raw = await self.websocket.recv()
        if isinstance(raw, bytes):
            raw = raw.decode("utf-8")
        payload = json.loads(raw)
        if not isinstance(payload, dict):
            raise DemoError(f"unexpected CDP payload: {payload!r}")
        return payload

    async def recv_until_id(self, message_id: int, *, timeout: float) -> tuple[dict[str, Any], list[dict[str, Any]]]:
        seen: list[dict[str, Any]] = []
        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise DemoError(f"timed out waiting for CDP response id={message_id}; seen={seen[-10:]}")
            message = await asyncio.wait_for(self.recv(), timeout=remaining)
            seen.append(message)
            if message.get("id") != message_id:
                continue
            if "error" in message:
                raise DemoError(f"CDP command {message_id} failed: {message['error']}")
            return message, seen

    async def command(
        self,
        method: str,
        params: dict[str, Any] | None = None,
        *,
        session_id: str | None = None,
        timeout: float = 10.0,
    ) -> tuple[dict[str, Any], list[dict[str, Any]]]:
        message_id = await self.send(method, params, session_id=session_id)
        return await self.recv_until_id(message_id, timeout=timeout)


async def connect_cdp(endpoint: str) -> RawCdpClient:
    payload = await asyncio.to_thread(read_json_url_no_proxy, endpoint.rstrip("/") + "/json/version")
    websocket_url = payload.get("webSocketDebuggerUrl")
    if not isinstance(websocket_url, str) or not websocket_url:
        raise DemoError(f"missing webSocketDebuggerUrl in discovery payload: {payload!r}")
    try:
        websocket = await websockets.connect(websocket_url, open_timeout=5, max_size=None, proxy=None)
    except TypeError:
        websocket = await websockets.connect(websocket_url, open_timeout=5, max_size=None)
    return RawCdpClient(websocket=websocket)


@dataclass
class CdpPage:
    client: RawCdpClient
    target_id: str
    session_id: str

    @classmethod
    async def create(cls, client: RawCdpClient) -> "CdpPage":
        response, _ = await client.command("Target.createTarget", {"url": "about:blank"}, timeout=10)
        target_id = str(response["result"]["targetId"])
        attach, _ = await client.command("Target.attachToTarget", {"targetId": target_id, "flatten": True}, timeout=10)
        session_id = str(attach["result"]["sessionId"])
        page = cls(client=client, target_id=target_id, session_id=session_id)
        for method in ("Page.enable", "Runtime.enable", "Network.enable"):
            await page.command(method, timeout=10)
        try:
            await page.command("Page.setLifecycleEventsEnabled", {"enabled": True}, timeout=10)
        except DemoError:
            pass
        return page

    async def command(
        self,
        method: str,
        params: dict[str, Any] | None = None,
        *,
        timeout: float = 10.0,
    ) -> tuple[dict[str, Any], list[dict[str, Any]]]:
        return await self.client.command(method, params, session_id=self.session_id, timeout=timeout)

    async def evaluate(self, expression: str, *, timeout: float = 10.0, await_promise: bool = False) -> Any:
        response, _ = await self.command(
            "Runtime.evaluate",
            {
                "expression": expression,
                "returnByValue": True,
                "awaitPromise": await_promise,
            },
            timeout=timeout,
        )
        result = response.get("result", {})
        if "exceptionDetails" in result:
            raise DemoError(f"Runtime.evaluate exception: {result['exceptionDetails']}")
        remote = result.get("result", {})
        if isinstance(remote, dict) and "value" in remote:
            return remote["value"]
        return remote

    async def navigate(self, url: str, *, timeout: float) -> None:
        response, seen = await self.command("Page.navigate", {"url": url}, timeout=timeout)
        frame_id = response.get("result", {}).get("frameId")
        await self.wait_domcontentloaded(str(frame_id) if frame_id is not None else None, seen, timeout=min(timeout, 30.0))
        await self.wait_ready(timeout=min(timeout, 15.0))

    def is_domcontentloaded_event(self, message: dict[str, Any], frame_id: str | None) -> bool:
        if message.get("sessionId") != self.session_id:
            return False
        method = message.get("method")
        if method == "Page.domContentEventFired":
            return True
        if method != "Page.lifecycleEvent":
            return False
        params = message.get("params")
        if not isinstance(params, dict):
            return False
        if frame_id is not None and params.get("frameId") != frame_id:
            return False
        return params.get("name") in {"DOMContentLoaded", "domContentLoaded"}

    async def wait_domcontentloaded(
        self,
        frame_id: str | None,
        seen: list[dict[str, Any]],
        *,
        timeout: float,
    ) -> None:
        if any(self.is_domcontentloaded_event(message, frame_id) for message in seen):
            return
        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise DemoError("timed out waiting for DOMContentLoaded")
            message = await asyncio.wait_for(self.client.recv(), timeout=remaining)
            if self.is_domcontentloaded_event(message, frame_id):
                return

    async def wait_ready(self, *, timeout: float) -> None:
        deadline = time.monotonic() + timeout
        last_error: BaseException | None = None
        while time.monotonic() < deadline:
            try:
                state = await self.evaluate("document.readyState", timeout=2)
                if state in ("interactive", "complete"):
                    return
            except BaseException as error:  # noqa: BLE001 - navigation may swap documents.
                last_error = error
            await asyncio.sleep(0.2)
        raise DemoError(f"timed out waiting for document readiness; last_error={last_error!r}")

    async def wait_until(
        self,
        expression: str,
        predicate: Callable[[Any], bool],
        *,
        timeout: float,
        label: str,
    ) -> Any:
        deadline = time.monotonic() + timeout
        last_value: Any = None
        last_error: BaseException | None = None
        while time.monotonic() < deadline:
            try:
                value = await self.evaluate(expression, timeout=3, await_promise=False)
                last_value = value
                if predicate(value):
                    return value
            except BaseException as error:  # noqa: BLE001 - retry transient navigation/eval errors.
                last_error = error
            await asyncio.sleep(0.25)
        raise DemoError(f"timed out waiting for {label}; last_value={last_value!r}; last_error={last_error!r}")

    async def close(self) -> None:
        try:
            await self.client.command("Target.closeTarget", {"targetId": self.target_id}, timeout=2)
        except Exception:
            pass


def search_url(keyword: str) -> str:
    encoded = urllib.parse.quote(keyword)
    return f"https://search.bilibili.com/all?keyword={encoded}"


def js_call(function_source: str, *args: Any) -> str:
    encoded_args = ",".join(json.dumps(arg, ensure_ascii=False) for arg in args)
    return f"({function_source})({encoded_args})"


FILL_AND_CLICK_SEARCH_JS = r"""
function(keyword) {
  function fire(el, name, init) {
    let event;
    try {
      if (name === 'input' && typeof InputEvent !== 'undefined') {
        event = new InputEvent('input', Object.assign({ bubbles: true, inputType: 'insertText', data: keyword }, init || {}));
      } else if (name.startsWith('mouse') || name === 'click') {
        event = new MouseEvent(name, Object.assign({ bubbles: true, cancelable: true }, init || {}));
      } else {
        event = new Event(name, Object.assign({ bubbles: true, cancelable: true }, init || {}));
      }
    } catch (_) {
      event = document.createEvent('Event');
      event.initEvent(name, true, true);
    }
    el.dispatchEvent(event);
  }

  function setInputValue(input, value) {
    input.focus && input.focus();
    const descriptor = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
    if (descriptor && descriptor.set) {
      descriptor.set.call(input, value);
    } else {
      input.value = value;
    }
    fire(input, 'beforeinput');
    fire(input, 'input');
    fire(input, 'change');
  }

  const input = document.querySelector('input.nav-search-input') ||
    document.querySelector('input.search-input-el') ||
    document.querySelector('input[type="search"]') ||
    document.querySelector('input[type="text"]');
  if (!input) {
    return { ok: false, reason: 'missing search input', url: location.href, title: document.title };
  }
  setInputValue(input, keyword);

  const form = input.closest('form');
  const button = document.querySelector('.nav-search-btn') ||
    document.querySelector('.search-button') ||
    (form && form.querySelector('button, input[type="submit"]'));
  const before = location.href;
  if (button) {
    button.scrollIntoView && button.scrollIntoView({ block: 'center', inline: 'center' });
    for (const name of ['mouseover', 'mousedown', 'mouseup', 'click']) {
      fire(button, name);
    }
    if (typeof button.click === 'function') {
      button.click();
    }
  } else if (form && typeof form.requestSubmit === 'function') {
    form.requestSubmit();
  } else if (form && typeof form.submit === 'function') {
    form.submit();
  } else {
    return { ok: false, reason: 'missing search button/form', url: location.href, inputValue: input.value };
  }
  return {
    ok: true,
    before,
    after: location.href,
    inputValue: input.value,
    clickedSelector: button ? (button.className || button.tagName) : 'form'
  };
}
"""


LOCATION_STATE_JS = r"""
(() => ({
  url: location.href,
  host: location.hostname,
  title: document.title || '',
  readyState: document.readyState,
  isSearchDocument: !!document.querySelector('.search-layout, input.search-input-el'),
  hasSearchResults: document.querySelectorAll('a[data-mod="search-card"][href*="/video/"] h3.bili-video-card__info--tit').length
}))()
"""


EXTRACT_RESULTS_JS = r"""
function(limit) {
  function clean(text) {
    return String(text || '').replace(/\s+/g, ' ').trim();
  }

  function decodeHtml(text) {
    const textarea = document.createElement('textarea');
    textarea.innerHTML = text;
    return textarea.value;
  }

  const isSearchDocument = !!document.querySelector('.search-layout, input.search-input-el');
  if (!isSearchDocument) {
    return {
      url: location.href,
      title: document.title || '',
      readyState: document.readyState,
      count: 0,
      items: []
    };
  }

  const seen = new Set();
  const items = [];
  const selectors = [
    'a[data-mod="search-card"][href*="/video/"] h3.bili-video-card__info--tit',
    'h3.bili-video-card__info--tit[title]',
    '.video-list h3[title]',
    '.search-page h3[title]'
  ];

  for (const selector of selectors) {
    for (const titleNode of document.querySelectorAll(selector)) {
      const title = clean(titleNode.getAttribute('title') || titleNode.textContent);
      if (!title || seen.has(title)) continue;
      const link = titleNode.closest('a[href*="/video/"]') ||
        titleNode.parentElement?.querySelector?.('a[href*="/video/"]') ||
        titleNode.closest('.bili-video-card')?.querySelector?.('a[href*="/video/"]');
      seen.add(title);
      items.push({ title, url: link ? link.href : '' });
      if (items.length >= limit) break;
    }
    if (items.length >= limit) break;
  }

  if (items.length < limit) {
    const html = document.documentElement ? document.documentElement.innerHTML : '';
    const pattern = /<h3[^>]*class="[^"]*bili-video-card__info--tit[^"]*"[^>]*title="([^"]+)"/g;
    for (const match of html.matchAll(pattern)) {
      const title = clean(decodeHtml(match[1]));
      if (!title || seen.has(title)) continue;
      seen.add(title);
      items.push({ title, url: '' });
      if (items.length >= limit) break;
    }
  }

  return {
    url: location.href,
    title: document.title || '',
    readyState: document.readyState,
    count: items.length,
    items: items.slice(0, limit)
  };
}
"""


async def run_demo(args: argparse.Namespace) -> dict[str, Any]:
    serve = start_moli(args) if not args.cdp_endpoint else None
    endpoint = args.cdp_endpoint or (serve.endpoint if serve else "")
    client: RawCdpClient | None = None
    page: CdpPage | None = None
    try:
        client = await connect_cdp(endpoint)
        page = await CdpPage.create(client)

        print(f"moli CDP: {endpoint}")
        print(f"open: {args.home_url}")
        await page.navigate(args.home_url, timeout=args.timeout)

        direct_url = search_url(args.keyword)
        click_result = await page.evaluate(
            js_call(FILL_AND_CLICK_SEARCH_JS, args.keyword),
            timeout=10,
        )
        if not isinstance(click_result, dict) or not click_result.get("ok"):
            raise DemoError(f"failed to drive Bilibili search UI: {click_result!r}")
        print(f"typed keyword and clicked search: {args.keyword}")

        used_direct_fallback = False
        try:
            await page.wait_until(
                LOCATION_STATE_JS,
                lambda value: isinstance(value, dict)
                and bool(value.get("isSearchDocument"))
                and int(value.get("hasSearchResults") or 0) > 0,
                timeout=args.click_wait_timeout,
                label="search page after homepage click",
            )
        except DemoError:
            used_direct_fallback = True
            print(f"homepage click did not finish navigation in time; navigate directly: {direct_url}")
            await page.navigate(direct_url, timeout=args.timeout)

        result = await page.wait_until(
            js_call(EXTRACT_RESULTS_JS, args.limit),
            lambda value: isinstance(value, dict) and len(value.get("items") or []) >= args.limit,
            timeout=args.timeout,
            label=f"{args.limit} Bilibili video search results",
        )
        if not isinstance(result, dict):
            raise DemoError(f"unexpected result payload: {result!r}")
        result["clickResult"] = click_result
        result["usedDirectFallback"] = used_direct_fallback
        return result
    finally:
        if page is not None:
            await page.close()
        if client is not None:
            await client.websocket.close()
        if serve is not None:
            stop_moli(serve)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Search bilibili.com through Moli raw CDP and print video titles.")
    parser.add_argument("--keyword", default=DEFAULT_KEYWORD, help="Search keyword. Defaults to 猛禽峡谷.")
    parser.add_argument("--limit", type=int, default=5, help="Number of video titles to print.")
    parser.add_argument("--home-url", default=DEFAULT_HOME_URL, help="Initial page to open before using the search box.")
    parser.add_argument("--cdp-endpoint", help="Use an existing Moli CDP endpoint instead of starting moli serve.")
    parser.add_argument("--moli-bin", help="Path to the moli binary.")
    parser.add_argument("--host", default="127.0.0.1", help="Host for moli serve.")
    parser.add_argument("--port", type=int, default=0, help="Port for moli serve. Defaults to a free local port.")
    parser.add_argument("--profile-dir", help="Optional Moli profile directory.")
    parser.add_argument("--http-proxy", help="Optional proxy passed to moli serve, for example http://127.0.0.1:7890.")
    parser.add_argument("--user-agent", default=DEFAULT_USER_AGENT, help="User-Agent passed to moli serve.")
    parser.add_argument("--startup-timeout", type=float, default=10.0, help="Seconds to wait for moli serve startup.")
    parser.add_argument("--click-wait-timeout", type=float, default=2.0, help="Seconds to wait for search-button navigation.")
    parser.add_argument("--timeout", type=float, default=35.0, help="Seconds to wait for navigation and result extraction.")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.limit <= 0:
        raise SystemExit("--limit must be positive")
    try:
        result = asyncio.run(run_demo(args))
    except KeyboardInterrupt:
        return 130
    except DemoError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    print(f"result page: {result.get('url')}")
    if result.get("usedDirectFallback"):
        print("note: direct search URL fallback was used after attempting the homepage click")
    print("top video titles:")
    for index, item in enumerate(result.get("items") or [], 1):
        print(f"{index}. {item.get('title', '')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
