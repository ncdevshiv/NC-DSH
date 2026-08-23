import { useEffect, useMemo, useReducer, useState } from "react";

import { money, stableItems } from "../shared/data";
import { markReady } from "../shared/harness";
import { mountReact, type ReactCaseProps, useFrameUpdate } from "./support";

function reducer(value: number, action: { amount: number }) {
  return value + action.amount;
}

function StateCase({ meta, spec }: ReactCaseProps) {
  const [count, setCount] = useState(0);
  const [enabled, setEnabled] = useState(false);
  const [reduced, dispatch] = useReducer(reducer, spec.seed);
  const items = useMemo(() => stableItems(spec.seed, Math.max(4, spec.size)), [spec.seed, spec.size]);
  const total = useMemo(() => items.reduce((sum, item) => sum + item.amount, 0) + count, [count, items]);
  useFrameUpdate(meta, () => {
    setCount((value) => value + 1);
    setCount((value) => value + 2);
    setCount((value) => value + spec.variant + 3);
    setEnabled(true);
    dispatch({ amount: 5 });
    dispatch({ amount: spec.variant });
  }, [spec.variant]);
  useEffect(() => {
    if (enabled && count > 0) markReady(meta, ["mounted", "react-batched-state-commit"]);
  }, [count, enabled, meta]);

  const summaries = [
    ["count", String(count)],
    ["enabled", String(enabled)],
    ["reduced", String(reduced)],
    ["total", money(total)],
  ];
  let body;
  if (spec.variant === 0) body = <output aria-label="count">{count}</output>;
  else if (spec.variant === 1) body = <p data-batch-result={count}>Three queued increments: {count}</p>;
  else if (spec.variant === 2) body = <p>Derived total <strong>{money(total)}</strong></p>;
  else if (spec.variant === 3) body = <div role="switch" aria-checked={enabled} data-state={enabled ? "on" : "off"}>Feature {enabled ? "enabled" : "disabled"}</div>;
  else if (spec.variant === 4) body = <p>Reducer value <b>{reduced}</b></p>;
  else if (spec.variant === 5) body = <dl><dt>Gross</dt><dd>{money(total)}</dd><dt>Net</dt><dd>{money(total - reduced)}</dd></dl>;
  else if (spec.variant === 6) body = <ol>{summaries.map(([name, value]) => <li key={name} data-key={name}>{name}: {value}</li>)}</ol>;
  else if (spec.variant === 7) body = <section><h2>Memo summary</h2>{items.slice(0, 8).map((item) => <p key={item.id}>{item.title}: {money(item.amount + count)}</p>)}</section>;
  else if (spec.variant === 8) body = <div data-machine-state={enabled ? "settled" : "booting"}><h2>{enabled ? "Settled" : "Booting"}</h2><p>Transition #{reduced}</p></div>;
  else body = <div className="metrics">{Array.from({ length: 16 }, (_, index) => <article key={index}><h2>Metric {index + 1}</h2><strong>{total + index * reduced}</strong><small>{enabled ? "live" : "idle"}</small></article>)}</div>;
  return <main id="smoke-root" data-framework="react" data-family={meta.family} data-count={count}><h1>{meta.title}</h1><section data-case-body>{body}</section></main>;
}

export function mount(meta: ReactCaseProps["meta"], spec: ReactCaseProps["spec"]): void {
  mountReact(StateCase, meta, spec);
}
