import { createContext, type ReactNode, useContext, useEffect, useState } from "react";
import { createPortal } from "react-dom";

import { stableItems, type StableItem } from "../shared/data";
import { markReady } from "../shared/harness";
import { mountReact, type ReactCaseProps, useFrameUpdate } from "./support";

const ThemeContext = createContext("unset");

function Badge({ children }: { children: ReactNode }) {
  return <span className="badge">{children}</span>;
}

function ContextCard() {
  return <article data-theme={useContext(ThemeContext)}><h2>Context consumer</h2></article>;
}

function TreeNode({ item, depth }: { item: StableItem; depth: number }) {
  return <li data-depth={depth}><span>{item.title}</span>{depth < 3 && <ul><TreeNode item={{ ...item, id: `${item.id}-${depth}` }} depth={depth + 1} /></ul>}</li>;
}

function ComponentsCase({ meta, spec }: ReactCaseProps) {
  const [active, setActive] = useState(0);
  const [updated, setUpdated] = useState(false);
  const items = stableItems(spec.seed, Math.max(5, spec.size));
  useFrameUpdate(meta, () => {
    setActive((spec.variant + 1) % 3);
    setUpdated(true);
  }, [spec.variant]);
  useEffect(() => {
    if (updated) markReady(meta, ["mounted", "react-component-composition"]);
  }, [meta, updated]);

  let body;
  if (spec.variant === 0) body = <Badge>Input value {spec.seed}</Badge>;
  else if (spec.variant === 1) body = <article><h2>Parent</h2><section><h3>Child</h3><Badge>Grandchild</Badge></section></article>;
  else if (spec.variant === 2) body = <ThemeContext.Provider value="ocean"><ContextCard /></ThemeContext.Provider>;
  else if (spec.variant === 3) body = <section><header><Badge>Header slot</Badge></header><main>{items.slice(0, 3).map((item) => <p key={item.id}>{item.title}</p>)}</main><footer>Footer slot</footer></section>;
  else if (spec.variant === 4) body = active === 1 ? <article data-component="alpha">Alpha component</article> : <aside data-component="beta">Beta component</aside>;
  else if (spec.variant === 5) body = <ul><TreeNode item={items[0]} depth={0} /></ul>;
  else if (spec.variant === 6) {
    const host = document.getElementById("portal-host");
    body = <section><p>Local content</p>{host && createPortal(<aside id="portal-content">Portal {spec.seed}</aside>, host)}</section>;
  } else if (spec.variant === 7) body = <div role="group"><Badge>Prefix</Badge><button>Action</button><Badge>Suffix</Badge></div>;
  else if (spec.variant === 8) body = <section><div role="tablist">{["Overview", "Network", "Runtime"].map((label, index) => <button key={label} role="tab" aria-selected={active === index}>{label}</button>)}</div><article role="tabpanel"><h2>{["Overview", "Network", "Runtime"][active]}</h2><p>Selected panel {active}</p></article></section>;
  else body = <div className="app-shell"><header><h2>Moli Console</h2><Badge>online</Badge></header><nav>{items.slice(0, 5).map((item) => <a key={item.id} href={`#${item.id}`}>{item.title}</a>)}</nav><section>{items.slice(5).map((item) => <article key={item.id}><h3>{item.title}</h3><p>{item.owner}</p></article>)}</section><footer>Build {spec.seed}</footer></div>;
  return <><main id="smoke-root" data-framework="react" data-family={meta.family}><h1>{meta.title}</h1><section data-case-body>{body}</section></main><div id="portal-host" /></>;
}

export function mount(meta: ReactCaseProps["meta"], spec: ReactCaseProps["spec"]): void {
  mountReact(ComponentsCase, meta, spec);
}
