from __future__ import annotations


MODULE_CASES = (
    "module-graph",
    "importmap-module",
)


def _html(body: str, script: str = "") -> bytes:
    return (
        "<!doctype html><meta charset=utf-8>"
        "<title>moli benchmark</title>"
        f"<body>{body}<script>{script}</script></body>"
    ).encode("utf-8")


def _case_response(case: str) -> tuple[str, bytes, float]:
    if case == "module-graph":
        body = "<main data-benchmark-case=module-graph></main><script type=module src=/module-main.mjs></script>"
        return "text/html; charset=utf-8", _html(body), 0.0
    if case == "importmap-module":
        body = """
        <main data-benchmark-case=importmap-module></main>
        <script type=importmap>{"imports":{"bench-alias":"/module-alias.mjs"}}</script>
        <script type=module>
        import { value } from 'bench-alias';
        document.body.dataset.benchmarkStatus = value === 42 ? 'ok' : 'bad';
        </script>
        """
        return "text/html; charset=utf-8", _html(body), 0.0
    raise KeyError(case)


def response_for_module_path(path: str) -> tuple[str, bytes, float] | None:
    case = path.strip("/")
    if path == "/module-main.mjs":
        return (
            "application/javascript",
            b"import { value } from '/module-dep.mjs'; document.body.dataset.benchmarkStatus = value === 42 ? 'ok' : 'bad';",
            0.0,
        )
    if path == "/module-dep.mjs" or path == "/module-alias.mjs":
        return "application/javascript", b"export const value = 42;", 0.0
    if case in MODULE_CASES:
        return _case_response(case)
    return None
