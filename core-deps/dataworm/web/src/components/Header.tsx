import { Show, type JSX } from "solid-js";
import type { Ping, Summary } from "../types";

export default function Header(props: {
  conn: () => "connecting" | "live" | "reconnecting";
  ping: () => Ping | null;
  summary: () => Summary | null;
  cycles: () => number;
  converged: () => boolean | null;
}): JSX.Element {
  const dotClass = () =>
    props.conn() === "live" ? "dot live" : props.conn() === "connecting" ? "dot connecting" : "dot down";
  const sum = () => props.summary();
  const federated = () => {
    const t = sum()?.total;
    const n = sum()?.nodes;
    return typeof t?.nodes === "number" && typeof n === "number" && t.nodes !== n;
  };

  return (
    <header class="app-header glass">
      <div class="brand">
        <span class="logo">DataWorm 🪱</span>
        <span class="sub">self-aligning discovery graph</span>
      </div>
      <div class="chips">
        <span class={dotClass()} title={`SSE ${props.conn()}`}>
          {props.conn()}
        </span>
        <Show when={props.ping()?.backend}>
          <span class="chip" title="execution backend">
            ⚙ {props.ping()!.backend}
          </span>
        </Show>
        <Show when={props.summary()?.root}>
          <span class="chip mono" title="crawl root">
            📂 {props.summary()!.root}
          </span>
        </Show>
        <Show
          when={federated()}
          fallback={
            <Show when={typeof sum()?.nodes === "number"}>
              <span class="chip">{sum()!.nodes} nodes</span>
            </Show>
          }
        >
          <span class="chip mono" title="across all fragments">
            🌐 {sum()!.total!.nodes} nodes
          </span>
          <span class="chip" title="active root fragment">
            {sum()!.nodes} active
          </span>
        </Show>
        <Show when={typeof props.summary()?.edges === "number"}>
          <span class="chip">{props.summary()!.edges} edges</span>
        </Show>
        <Show when={props.converged() !== null}>
          <span class={`chip ${props.converged() ? "ok" : "warn"}`}>
            {props.converged() ? `converged · ${props.cycles()} cycle(s)` : `diverged · ${props.cycles()} cycles`}
          </span>
        </Show>
      </div>
    </header>
  );
}
