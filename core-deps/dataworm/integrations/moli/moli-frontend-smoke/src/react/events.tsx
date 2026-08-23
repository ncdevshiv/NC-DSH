import { useEffect, useRef, useState } from "react";

import { assertFixture, markReady } from "../shared/harness";
import { mountReact, type ReactCaseProps, useFrameUpdate } from "./support";

function setNativeValue(element: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
  assertFixture(setter, "native input value setter exists");
  setter.call(element, value);
}

function EventsCase({ meta, spec }: ReactCaseProps) {
  const rootRef = useRef<HTMLElement>(null);
  const [log, setLog] = useState<string[]>([]);
  const append = (value: string) => setLog((current) => [...current, value]);

  useFrameUpdate(meta, () => {
    const host = rootRef.current;
    assertFixture(host, "event host mounted");
    const trigger = host.querySelector<HTMLElement>("[data-trigger]");
    assertFixture(trigger, "event trigger mounted");
    if (spec.variant === 3) {
      setNativeValue(trigger as HTMLInputElement, `typed-${spec.seed}`);
      trigger.dispatchEvent(new InputEvent("input", { bubbles: true, data: "x" }));
    } else if (spec.variant === 4) {
      (trigger as HTMLSelectElement).value = "done";
      trigger.dispatchEvent(new Event("change", { bubbles: true }));
    } else if (spec.variant === 5) {
      trigger.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    } else if (spec.variant === 6 || spec.variant === 9) {
      trigger.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: spec.variant === 9 ? "k" : "Enter", ctrlKey: spec.variant === 9 }));
    } else {
      trigger.click();
    }
  }, [spec.seed, spec.variant]);
  useEffect(() => {
    if (log.length > 0) markReady(meta, ["mounted", "react-synthetic-event"]);
  }, [log, meta]);

  let trigger;
  if (spec.variant === 3) trigger = <input data-trigger defaultValue="initial" onInput={(event) => append(`input:${event.currentTarget.value}`)} />;
  else if (spec.variant === 4) trigger = <select data-trigger defaultValue="new" onChange={(event) => append(`change:${event.currentTarget.value}`)}><option value="new">New</option><option value="done">Done</option></select>;
  else if (spec.variant === 5) trigger = <form data-trigger onSubmit={(event) => { event.preventDefault(); append("submit:prevented"); }}><button>Submit</button></form>;
  else if (spec.variant === 6 || spec.variant === 9) trigger = <input data-trigger aria-label="Command" onKeyDown={(event) => append(`key:${event.key}:${event.ctrlKey}`)} />;
  else if (spec.variant === 1) trigger = <div onClick={() => append("parent")}><button data-trigger onClick={() => append("child")}>Bubble</button></div>;
  else if (spec.variant === 2) trigger = <div onClick={() => append("parent")}><button data-trigger onClick={(event) => { event.stopPropagation(); append("child-stopped"); }}>Stop</button></div>;
  else if (spec.variant === 7) trigger = <button data-trigger onClick={() => append(`output:${spec.seed}`)}>Emit output</button>;
  else if (spec.variant === 8) trigger = <button data-trigger onClick={() => { append("first"); append("second"); append("third"); }}>Handlers</button>;
  else trigger = <button data-trigger onClick={() => append("click:updated")}>Click</button>;

  return <main ref={rootRef} id="smoke-root" data-framework="react" data-family={meta.family}><h1>{meta.title}</h1><section data-case-body>{trigger}<output><ol>{log.map((entry, index) => <li key={`${entry}-${index}`}>{entry}</li>)}</ol></output></section></main>;
}

export function mount(meta: ReactCaseProps["meta"], spec: ReactCaseProps["spec"]): void {
  mountReact(EventsCase, meta, spec);
}
