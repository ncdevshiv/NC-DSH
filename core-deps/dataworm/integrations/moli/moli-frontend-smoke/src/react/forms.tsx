import { useEffect, useState } from "react";

import { markReady } from "../shared/harness";
import { mountReact, type ReactCaseProps, useFrameUpdate } from "./support";

function FormsCase({ meta, spec }: ReactCaseProps) {
  const [name, setName] = useState("Initial");
  const [notes, setNotes] = useState("Draft");
  const [checked, setChecked] = useState<string[]>(["dom"]);
  const [priority, setPriority] = useState("normal");
  const [status, setStatus] = useState("new");
  const [regions, setRegions] = useState(["eu"]);
  useFrameUpdate(meta, () => {
    setName(`User ${spec.seed}`);
    setNotes(`Updated notes ${spec.variant}`);
    setChecked(["dom", "runtime"]);
    setPriority("high");
    setStatus("done");
    setRegions(["us", "apac"]);
  }, [spec.seed, spec.variant]);
  useEffect(() => {
    if (name.startsWith("User ")) markReady(meta, ["mounted", "react-controlled-form-update"]);
  }, [meta, name]);

  const check = (value: string) => checked.includes(value);
  let body;
  if (spec.variant === 0) body = <label>Name <input value={name} readOnly /></label>;
  else if (spec.variant === 1) body = <label>Notes <textarea value={notes} readOnly /></label>;
  else if (spec.variant === 2) body = <fieldset><legend>Scopes</legend>{["dom", "runtime", "network"].map((value) => <label key={value}><input type="checkbox" checked={check(value)} readOnly />{value}</label>)}</fieldset>;
  else if (spec.variant === 3) body = <fieldset><legend>Priority</legend>{["low", "normal", "high"].map((value) => <label key={value}><input type="radio" name="priority" checked={priority === value} readOnly />{value}</label>)}</fieldset>;
  else if (spec.variant === 4) body = <label>Status <select value={status} onChange={() => {}}><option value="new">New</option><option value="active">Active</option><option value="done">Done</option></select></label>;
  else if (spec.variant === 5) body = <label>Regions <select multiple value={regions} onChange={() => {}}><option value="eu">Europe</option><option value="us">Americas</option><option value="apac">Asia Pacific</option></select></label>;
  else if (spec.variant === 6) body = <div><input value={name} readOnly /><input value="locked" disabled /><button disabled>Save</button></div>;
  else if (spec.variant === 7) body = <label>Email <input type="email" value={`${name.replace(" ", ".").toLowerCase()}@example.test`} readOnly aria-invalid={false} /><small role="status">Valid address</small></label>;
  else if (spec.variant === 8) body = <fieldset><legend>Profile editor</legend><label>Name <input value={name} readOnly /></label><label>Notes <textarea value={notes} readOnly /></label><label>Status <select value={status} onChange={() => {}}><option value="done">Done</option></select></label></fieldset>;
  else body = <form><h2>Checkout</h2><label>Customer <input value={name} readOnly /></label><label>Delivery <select value={priority} onChange={() => {}}><option value="high">Express</option></select></label><fieldset><legend>Extras</legend>{["dom", "runtime", "network"].map((value) => <label key={value}><input type="checkbox" checked={check(value)} readOnly />{value}</label>)}</fieldset><output>Total fields: 6</output><button type="submit">Place order</button></form>;
  return <main id="smoke-root" data-framework="react" data-family={meta.family}><h1>{meta.title}</h1><section data-case-body>{body}</section></main>;
}

export function mount(meta: ReactCaseProps["meta"], spec: ReactCaseProps["spec"]): void {
  mountReact(FormsCase, meta, spec);
}
