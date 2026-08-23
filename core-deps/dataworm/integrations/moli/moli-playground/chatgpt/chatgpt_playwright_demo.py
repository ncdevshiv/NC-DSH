#!/usr/bin/env python3
from __future__ import annotations

import argparse
import asyncio
import getpass
import os
import sys
import time

from chatgpt_cdp_demo import (
    DEFAULT_ANSWER_TIMEOUT,
    DEFAULT_MOLI_HTTP_TIMEOUT_MS,
    DEFAULT_LOGIN_TIMEOUT,
    DEFAULT_STARTUP_TIMEOUT,
    DEFAULT_URL,
    DemoError,
)
from chatgpt_playwright_core import PlaywrightChatGPTSession


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Drive chatgpt.com through Playwright.")
    parser.add_argument("--backend", choices=("moli", "chromium"), default="moli", help="browser backend, default: moli")
    parser.add_argument("--url", default=DEFAULT_URL, help=f"initial URL, default: {DEFAULT_URL}")
    parser.add_argument("--email", default=os.environ.get("CHATGPT_EMAIL", ""), help="ChatGPT account email")
    parser.add_argument("--password-stdin", action="store_true", help="read password from stdin")
    parser.add_argument("--auth-code", default=os.environ.get("CHATGPT_AUTH_CODE", ""), help="verification code for email auth fallback")
    parser.add_argument("--auth-code-stdin", action="store_true", help="read verification code from stdin when requested")
    parser.add_argument("--auth-code-prompt", action="store_true", help="prompt for verification code when requested")
    parser.add_argument("--try-email-verification", action="store_true", help="click Try with email on device approval pages")
    parser.add_argument("--use-existing-session", action="store_true", help="require an already logged-in profile instead of reading credentials")
    parser.add_argument("--prompt", help="send one prompt after login and print the response")
    parser.add_argument("--login-only", action="store_true", help="stop after login succeeds")
    parser.add_argument(
        "--no-reload-recovery",
        action="store_true",
        help="do not reload the conversation to recover persisted answers; exposes live DOM render failures",
    )
    parser.add_argument(
        "--live-trace",
        action="store_true",
        help="record in-page fetch/WebSocket/stream/DOM trace events for ChatGPT live-render debugging",
    )
    parser.add_argument(
        "--live-trace-output",
        help="append sanitized live-trace summaries as JSON Lines to this path",
    )
    parser.add_argument("--moli-bin", help="path to the moli binary; alternatively set MOLI_BIN")
    parser.add_argument("--chromium-bin", help="path to a Chromium/Chrome executable for --backend chromium")
    parser.add_argument("--headful", action="store_true", help="run Chromium with a visible browser window")
    parser.add_argument("--profile-dir", help="optional Moli profile dir for cookies/localStorage")
    parser.add_argument("--user-agent", help="optional user agent passed to moli serve")
    parser.add_argument("--http-proxy", help="optional proxy passed to moli serve")
    parser.add_argument("--http-no-proxy", help="optional no-proxy list passed to moli serve")
    parser.add_argument(
        "--http-timeout",
        type=int,
        default=DEFAULT_MOLI_HTTP_TIMEOUT_MS,
        help=f"Moli request timeout in milliseconds, default: {DEFAULT_MOLI_HTTP_TIMEOUT_MS}",
    )
    parser.add_argument("--http-max-concurrent", type=int, help="optional max active fetch transfers passed to moli serve")
    parser.add_argument("--http-max-host-open", type=int, help="optional per-host fetch transfer cap passed to moli serve")
    parser.add_argument("--startup-timeout", type=float, default=DEFAULT_STARTUP_TIMEOUT)
    parser.add_argument("--login-timeout", type=float, default=DEFAULT_LOGIN_TIMEOUT)
    parser.add_argument("--answer-timeout", type=float, default=DEFAULT_ANSWER_TIMEOUT)
    parser.add_argument("--debug-snapshot", action="store_true", help="accepted for parity with the raw-CDP demo")
    parser.add_argument("--timestamps", action="store_true", help="prefix status output with elapsed seconds")
    return parser


def read_password(args: argparse.Namespace) -> str:
    if args.password_stdin:
        password = sys.stdin.readline().rstrip("\n")
    else:
        password = os.environ.get("CHATGPT_PASSWORD")
        if password is None:
            password = getpass.getpass("ChatGPT password: ")
    if not password:
        raise DemoError("password is required")
    return password


async def async_main(args: argparse.Namespace) -> int:
    if args.use_existing_session:
        password = ""
    else:
        if not args.email:
            raise DemoError("email is required")
        password = read_password(args)
    started = time.monotonic()

    def report(message: str) -> None:
        if args.timestamps:
            print(f"[{time.monotonic() - started:8.3f}s] {message}", flush=True)
        else:
            print(message, flush=True)

    session = PlaywrightChatGPTSession(args, reporter=report)
    try:
        await session.start()
        if args.use_existing_session:
            await session.require_existing_session()
        else:
            await session.login(args.email, password)
        print("login ok")
        if args.login_only:
            return 0
        if args.prompt:
            result = await session.ask_once(args.prompt)
            print(f"answer source: {result.source}")
            print("\n--- ChatGPT ---")
            print(result.text)
            return 0
        return 0
    finally:
        await session.close()


def main() -> int:
    args = build_parser().parse_args()
    try:
        return asyncio.run(async_main(args))
    except KeyboardInterrupt:
        return 130
    except DemoError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
