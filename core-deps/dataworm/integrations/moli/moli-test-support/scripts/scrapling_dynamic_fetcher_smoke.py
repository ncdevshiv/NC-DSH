import faulthandler
import json
import os
import signal
import sys


def _alarm_handler(signum, frame):
    raise TimeoutError("scrapling dynamic fetcher smoke timed out")


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit("usage: scrapling_dynamic_fetcher_smoke.py <url> <cdp_url>")

    scrapling_repo = os.environ.get("MOLI_SCRAPLING_REPO")
    if scrapling_repo:
        sys.path.insert(0, scrapling_repo)

    if os.environ.get("MOLI_SCRAPLING_SMOKE_DEBUG") == "1":
        faulthandler.enable()
        faulthandler.dump_traceback_later(10, repeat=True)

    from scrapling import DynamicFetcher

    url = sys.argv[1]
    cdp_url = sys.argv[2]
    signal.signal(signal.SIGALRM, _alarm_handler)
    signal.alarm(30)
    try:
        response = DynamicFetcher.fetch(
            url,
            cdp_url=cdp_url,
            google_search=False,
            headless=True,
            timeout=5_000,
            retries=1,
            load_dom=False,
            network_idle=False,
        )
    finally:
        signal.alarm(0)
        faulthandler.cancel_dump_traceback_later()

    body = response.body or b""
    if isinstance(body, str):
        body = body.encode()

    main_nodes = response.css("main")
    payload = {
        "body_contains_fixture_static": b"fixture static" in body,
        "main_text": main_nodes[0].text if main_nodes else None,
        "status": response.status,
        "url": response.url,
    }
    print(json.dumps(payload, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
