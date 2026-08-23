import { useEffect, useMemo, useState } from "react";

import { deterministicWords, money, stableItems } from "../shared/data";
import { markReady } from "../shared/harness";
import { mountReact, type ReactCaseProps, useFrameUpdate } from "./support";

function AppCompositionCase({ meta, spec }: ReactCaseProps) {
  const allItems = useMemo(() => stableItems(spec.seed, 24), [spec.seed]);
  const [filter, setFilter] = useState("all");
  const [page, setPage] = useState(0);
  useFrameUpdate(meta, () => {
    setFilter(spec.variant % 2 === 0 ? "active" : "done");
    setPage(1);
  }, [spec.variant]);
  useEffect(() => {
    if (page === 1) markReady(meta, ["mounted", "react-composed-app-update"]);
  }, [meta, page]);
  const visible = allItems.filter((item) => filter === "all" || item.status === filter);
  const cards = visible.length > 0 ? visible : allItems.slice(0, 6);
  const commonHeader = <header><a className="brand" href="#home">Moli</a><nav aria-label="Primary"><a href="#overview">Overview</a><a href="#activity">Activity</a><a href="#settings">Settings</a></nav><button>Account</button></header>;
  let body;
  if (spec.variant === 0) body = <>{commonHeader}<section><h2>Welcome</h2><p>{deterministicWords(spec.seed, 24)}</p></section></>;
  else if (spec.variant === 1) body = <div className="sidebar-layout"><aside><h2>Workspace</h2>{allItems.slice(0, 8).map((item) => <a key={item.id} href={`#${item.id}`}>{item.title}</a>)}</aside><section><h2>Selected view</h2>{deterministicWords(spec.seed, 40)}</section></div>;
  else if (spec.variant === 2) body = <div className="stats-grid">{allItems.slice(0, 8).map((item, index) => <article key={item.id}><h2>{item.title}</h2><strong>{item.amount + index * 17}</strong><small>{item.status}</small></article>)}</div>;
  else if (spec.variant === 3) body = <><label>Search <input value={filter} readOnly /></label><table><thead><tr><th>Name</th><th>Owner</th><th>Status</th></tr></thead><tbody>{cards.map((item) => <tr key={item.id}><td>{item.title}</td><td>{item.owner}</td><td>{item.status}</td></tr>)}</tbody></table></>;
  else if (spec.variant === 4) body = <><section>{allItems.slice(page * 6, page * 6 + 6).map((item) => <article key={item.id}><h2>{item.title}</h2></article>)}</section><nav aria-label="Pages">{[1, 2, 3, 4].map((value) => <button key={value} aria-current={page + 1 === value ? "page" : undefined}>{value}</button>)}</nav></>;
  else if (spec.variant === 5) body = <><div className="chips">{["all", "new", "active", "done"].map((value) => <button key={value} aria-pressed={filter === value}>{value}</button>)}</div><ul>{cards.map((item) => <li key={item.id}>{item.title}</li>)}</ul></>;
  else if (spec.variant === 6) body = <article className="profile"><header><div aria-hidden="true">ML</div><div><h2>Moli Light</h2><p>Browser runtime engineer</p></div></header><dl><dt>Projects</dt><dd>18</dd><dt>Open reviews</dt><dd>7</dd><dt>Compatibility</dt><dd>92%</dd></dl><section><h3>About</h3><p>{deterministicWords(spec.seed, 60)}</p></section></article>;
  else if (spec.variant === 7) body = <section className="notifications"><header><h2>Notifications</h2><button>Mark all read</button></header>{allItems.slice(0, 12).map((item, index) => <article key={item.id} data-unread={index < 4}><strong>{item.owner}</strong><p>{item.title}</p><time>09:{String(index * 4).padStart(2, "0")}</time></article>)}</section>;
  else if (spec.variant === 8) body = <form className="settings"><nav><button type="button">General</button><button type="button">Network</button><button type="button">Privacy</button></nav><section><h2>General settings</h2><label>Workspace name <input value="Moli Lab" readOnly /></label><label>Theme <select value="system" onChange={() => {}}><option value="system">System</option></select></label>{["Tracing", "Caching", "Diagnostics"].map((label, index) => <label key={label}><input type="checkbox" checked={index !== 1} readOnly />{label}</label>)}</section></form>;
  else body = <div className="admin"><header><h2>Administration</h2><button>Add member</button></header><section className="stats">{allItems.slice(0, 4).map((item) => <article key={item.id}><span>{item.status}</span><strong>{money(item.amount)}</strong></article>)}</section><table><thead><tr><th>User</th><th>Role</th><th>Status</th><th>Actions</th></tr></thead><tbody>{allItems.slice(0, 16).map((item) => <tr key={item.id}><td>{item.owner}</td><td>{item.tags[0]}</td><td>{item.status}</td><td><button>Edit</button></td></tr>)}</tbody></table></div>;
  return <main id="smoke-root" data-framework="react" data-family={meta.family} data-filter={filter}><h1>{meta.title}</h1><section data-case-body>{body}</section></main>;
}

export function mount(meta: ReactCaseProps["meta"], spec: ReactCaseProps["spec"]): void {
  mountReact(AppCompositionCase, meta, spec);
}
