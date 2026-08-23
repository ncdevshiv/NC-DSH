from __future__ import annotations


DOM_CASES = (
    "custom-elements-shadow",
    "message-channel",
    "mutation-observer",
)


def _html(body: str, script: str = "") -> bytes:
    return (
        "<!doctype html><meta charset=utf-8>"
        "<title>moli benchmark</title>"
        f"<body>{body}<script>{script}</script></body>"
    ).encode("utf-8")


def _case_response(case: str) -> tuple[str, bytes, float]:
    if case == "custom-elements-shadow":
        script = """
        customElements.define('bench-card', class extends HTMLElement {
          connectedCallback() {
            const root = this.attachShadow({ mode: 'open' });
            root.innerHTML = '<slot></slot><span id="marker">ready</span>';
          }
        });
        const card = document.createElement('bench-card');
        card.textContent = 'payload';
        document.body.appendChild(card);
        const marker = card.shadowRoot && card.shadowRoot.querySelector('#marker');
        document.body.dataset.benchmarkStatus = marker && marker.textContent === 'ready' ? 'ok' : 'bad';
        """
        return "text/html; charset=utf-8", _html("<main data-benchmark-case=custom-elements-shadow></main>", script), 0.0
    if case == "message-channel":
        script = """
        const channel = new MessageChannel();
        channel.port1.onmessage = event => {
          document.body.dataset.benchmarkStatus = event.data && event.data.value === 7 ? 'ok' : 'bad';
        };
        channel.port2.postMessage({ value: 7 });
        """
        return "text/html; charset=utf-8", _html("<main data-benchmark-case=message-channel></main>", script), 0.0
    if case == "mutation-observer":
        script = """
        const target = document.querySelector('main');
        const observer = new MutationObserver(records => {
          document.body.dataset.benchmarkStatus =
            records.length === 1 && target.dataset.changed === 'yes' ? 'ok' : 'bad';
          observer.disconnect();
        });
        observer.observe(target, { attributes: true });
        target.dataset.changed = 'yes';
        """
        return "text/html; charset=utf-8", _html("<main data-benchmark-case=mutation-observer></main>", script), 0.0
    raise KeyError(case)


def response_for_dom_path(path: str) -> tuple[str, bytes, float] | None:
    case = path.strip("/")
    if case in DOM_CASES:
        return _case_response(case)
    return None
