import { useEffect, useMemo, useState } from "react";

import { money, stableItems, type StableItem } from "../shared/data";
import { markReady } from "../shared/harness";
import { mountReact, type ReactCaseProps, useFrameUpdate } from "./support";

function itemView(item: StableItem) {
  return <li key={item.id} data-id={item.id} data-status={item.status}><strong>{item.title}</strong><span>{item.owner}</span></li>;
}

function CollectionsCase({ meta, spec }: ReactCaseProps) {
  const source = useMemo(() => stableItems(spec.seed, spec.variant === 9 ? 48 : Math.max(5, spec.size)), [spec.seed, spec.size, spec.variant]);
  const [items, setItems] = useState(source);
  useFrameUpdate(meta, () => {
    if (spec.variant === 1) setItems([...source].reverse());
    else if (spec.variant === 2) setItems([{ ...source[0], id: `prepended-${spec.seed}`, title: "Prepended checkpoint" }, ...source]);
    else if (spec.variant === 3) setItems(source.filter((_, index) => index % 2 === 0));
    else setItems(source.map((item, index) => index === 1 ? { ...item, status: "done" } : item));
  }, [source, spec.seed, spec.variant]);
  useEffect(() => {
    if (items !== source) markReady(meta, ["mounted", "react-keyed-collection-update"]);
  }, [items, meta, source]);

  let body;
  if (spec.variant <= 3) {
    body = <ul data-count={items.length}>{items.map(itemView)}</ul>;
  } else if (spec.variant === 4) {
    body = <>{["new", "active", "paused", "done"].map((status) => <section key={status}><h2>{status}</h2><ul>{items.filter((item) => item.status === status).map(itemView)}</ul></section>)}</>;
  } else if (spec.variant === 5) {
    body = <ul>{items.slice(0, 6).map((item, index) => <li key={item.id}><span>{item.title}</span>{index < 3 && <ul><li>{item.owner}</li><li>{item.tags.join(" / ")}</li></ul>}</li>)}</ul>;
  } else if (spec.variant === 6 || spec.variant === 9) {
    body = <table><thead><tr><th>Item</th><th>Owner</th><th>Status</th><th>Amount</th></tr></thead><tbody>{items.map((item) => <tr key={item.id}><th scope="row">{item.title}</th><td>{item.owner}</td><td>{item.status}</td><td>{money(item.amount)}</td></tr>)}</tbody></table>;
  } else if (spec.variant === 7) {
    body = <div role="list" className="card-grid">{items.map((item) => <article role="listitem" key={item.id}><h2>{item.title}</h2><p>{item.owner}</p><div>{item.tags.map((tag) => <span key={tag}>{tag}</span>)}</div></article>)}</div>;
  } else {
    body = <ol className="activity-feed">{items.map((item, index) => <li key={item.id}><time dateTime={`2026-07-${String(index + 1).padStart(2, "0")}`}>Day {index + 1}</time><p><b>{item.owner}</b> moved {item.title} to {item.status}</p></li>)}</ol>;
  }
  return <main id="smoke-root" data-framework="react" data-family={meta.family}><h1>{meta.title}</h1><section data-case-body>{body}</section></main>;
}

export function mount(meta: ReactCaseProps["meta"], spec: ReactCaseProps["spec"]): void {
  mountReact(CollectionsCase, meta, spec);
}
