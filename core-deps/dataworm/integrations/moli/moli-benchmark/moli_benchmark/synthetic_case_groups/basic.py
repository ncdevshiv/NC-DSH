from __future__ import annotations


BASIC_CASES = (
    "static-html",
    "js-xhr-fetch",
    "dynamic-script",
    "dom-heavy",
    "storage-cookie",
    "forms-events",
)


def _html(body: str, script: str = "") -> bytes:
    return (
        "<!doctype html><meta charset=utf-8>"
        "<title>moli benchmark</title>"
        f"<body>{body}<script>{script}</script></body>"
    ).encode("utf-8")


def _case_response(case: str) -> tuple[str, bytes, float]:
    if case == "static-html":
        body = "<main data-benchmark-case=static-html data-benchmark-status=ok>" + "".join(
            f"<p>row {index}</p>" for index in range(200)
        ) + "</main>"
        return "text/html; charset=utf-8", _html(body), 0.0
    if case == "js-xhr-fetch":
        script = """
        Promise.all([
          fetch('/api/a.json').then(r => r.json()),
          new Promise((resolve, reject) => {
            const xhr = new XMLHttpRequest();
            xhr.open('GET', '/api/b.json');
            xhr.onload = () => resolve(JSON.parse(xhr.responseText));
            xhr.onerror = reject;
            xhr.send();
          })
        ]).then(([a, b]) => {
          document.body.dataset.benchmarkStatus = a.ok && b.ok ? 'ok' : 'bad';
        });
        """
        return "text/html; charset=utf-8", _html("<main data-benchmark-case=js-xhr-fetch></main>", script), 0.0
    if case == "dynamic-script":
        script = """
        const s = document.createElement('script');
        s.src = '/dynamic-script.js';
        document.head.appendChild(s);
        """
        return "text/html; charset=utf-8", _html("<main data-benchmark-case=dynamic-script></main>", script), 0.0
    if case == "dom-heavy":
        script = """
        const root = document.querySelector('main');
        for (let i = 0; i < 2000; i++) {
          const item = document.createElement('button');
          item.textContent = 'item-' + i;
          item.dataset.index = String(i);
          root.appendChild(item);
        }
        document.body.dataset.benchmarkStatus = String(root.children.length) === '2000' ? 'ok' : 'bad';
        """
        return "text/html; charset=utf-8", _html("<main data-benchmark-case=dom-heavy></main>", script), 0.0
    if case == "storage-cookie":
        script = """
        document.cookie = 'lm_bench_cookie=1; path=/';
        localStorage.setItem('lm_bench_local', '1');
        sessionStorage.setItem('lm_bench_session', '1');
        document.body.dataset.benchmarkStatus =
          document.cookie.includes('lm_bench_cookie=1') &&
          localStorage.getItem('lm_bench_local') === '1' &&
          sessionStorage.getItem('lm_bench_session') === '1' ? 'ok' : 'bad';
        """
        return "text/html; charset=utf-8", _html("<main data-benchmark-case=storage-cookie></main>", script), 0.0
    if case == "forms-events":
        script = """
        const form = document.querySelector('form');
        const input = document.querySelector('input');
        let changed = false;
        let submitted = false;
        input.addEventListener('change', () => changed = true);
        form.addEventListener('submit', event => {
          event.preventDefault();
          submitted = true;
          document.body.dataset.benchmarkStatus = changed && submitted ? 'ok' : 'bad';
        });
        input.value = 'updated';
        input.dispatchEvent(new Event('change', { bubbles: true }));
        form.requestSubmit();
        """
        return (
            "text/html; charset=utf-8",
            _html("<form data-benchmark-case=forms-events><input name=q value=initial><button>send</button></form>", script),
            0.0,
        )
    raise KeyError(case)


def response_for_basic_path(path: str) -> tuple[str, bytes, float] | None:
    case = path.strip("/")
    if path == "/api/a.json" or path == "/api/b.json":
        return "application/json", b'{"ok":true}', 0.0
    if path == "/dynamic-script.js":
        return "application/javascript", b"document.body.dataset.benchmarkStatus='ok';", 0.0
    if case in BASIC_CASES:
        return _case_response(case)
    return None
