from __future__ import annotations


IO_CASES = (
    "abort-fetch",
    "blob-filereader",
)


def _html(body: str, script: str = "") -> bytes:
    return (
        "<!doctype html><meta charset=utf-8>"
        "<title>moli benchmark</title>"
        f"<body>{body}<script>{script}</script></body>"
    ).encode("utf-8")


def _case_response(case: str) -> tuple[str, bytes, float]:
    if case == "abort-fetch":
        script = """
        const controller = new AbortController();
        const promise = fetch('/slow.json', { signal: controller.signal });
        controller.abort();
        promise.then(
          () => { document.body.dataset.benchmarkStatus = 'bad'; },
          error => { document.body.dataset.benchmarkStatus = error && error.name === 'AbortError' ? 'ok' : 'bad'; }
        );
        """
        return "text/html; charset=utf-8", _html("<main data-benchmark-case=abort-fetch></main>", script), 0.0
    if case == "blob-filereader":
        script = """
        const blob = new Blob(['hello ', 'world'], { type: 'text/plain' });
        const reader = new FileReader();
        reader.onload = () => {
          document.body.dataset.benchmarkStatus =
            reader.result === 'hello world' && blob.type === 'text/plain' ? 'ok' : 'bad';
        };
        reader.onerror = () => { document.body.dataset.benchmarkStatus = 'bad'; };
        reader.readAsText(blob);
        """
        return "text/html; charset=utf-8", _html("<main data-benchmark-case=blob-filereader></main>", script), 0.0
    raise KeyError(case)


def response_for_io_path(path: str) -> tuple[str, bytes, float] | None:
    case = path.strip("/")
    if path == "/slow.json":
        return "application/json", b'{"ok":true}', 0.2
    if case in IO_CASES:
        return _case_response(case)
    return None
