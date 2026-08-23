import { useEffect, useState } from "react";

import { stableItems } from "../shared/data";
import { captureFrame, failCase, markReady } from "../shared/harness";
import { mountReact, type ReactCaseProps, useFrameUpdate } from "./support";

function Child({ label }: { label: string }) {
  useEffect(() => () => {
    document.documentElement.dataset.childCleanup = "complete";
  }, []);
  return <aside id="async-child">{label}</aside>;
}

function AsyncCase({ meta, spec }: ReactCaseProps) {
  const [phase, setPhase] = useState("mounted");
  const [showChild, setShowChild] = useState(true);
  const [timeline, setTimeline] = useState(["render"]);
  useFrameUpdate(meta, async () => {
    const update = (label: string) => {
      setPhase(label);
      setTimeline((current) => [...current, label]);
      if (spec.variant === 7) setShowChild(false);
    };
    if (spec.variant === 9) {
      requestAnimationFrame(() => update("frame-1"));
    } else if (spec.variant === 4) setTimeout(() => update("timer"), 0);
    else if (spec.variant === 3 || spec.variant >= 5) Promise.resolve().then(() => update("promise"));
    else queueMicrotask(() => update(spec.variant === 2 ? "microtask" : "effect"));
  }, [spec.variant]);
  useEffect(() => {
    if (phase === "mounted") {
      return;
    }
    if (spec.variant !== 9) {
      markReady(meta, [`react-${phase}-settled`]);
      return;
    }
    const index = Number(phase.slice("frame-".length));
    void captureFrame(meta, `animation-frame-${index}`)
      .then(() => {
        if (index < 3) {
          requestAnimationFrame(() => {
            const label = `frame-${index + 1}`;
            setPhase(label);
            setTimeline((current) => [...current, label]);
          });
        } else {
          markReady(meta, ["react-animation-settled"]);
        }
      })
      .catch(failCase);
  }, [meta, phase, spec.variant]);
  const items = stableItems(spec.seed, spec.variant >= 8 ? 24 : 5);
  return <main id="smoke-root" data-framework="react" data-family={meta.family} data-phase={phase}><h1>{meta.title}</h1><section data-case-body><p role="status">Phase: {phase}</p>{showChild && <Child label={`Child ${spec.seed}`} />}{spec.variant >= 5 && <ul>{items.map((item) => <li key={item.id}>{item.title} — {phase}</li>)}</ul>}{spec.variant >= 6 && <ol>{timeline.map((entry, index) => <li key={`${entry}-${index}`}>{index}: {entry}</li>)}</ol>}{spec.variant === 9 && <div className="timeline">{Array.from({ length: 12 }, (_, index) => <article key={index}><h2>Stage {index + 1}</h2><p>{timeline.join(" → ")}</p></article>)}</div>}</section></main>;
}

export function mount(meta: ReactCaseProps["meta"], spec: ReactCaseProps["spec"]): void {
  mountReact(AsyncCase, meta, spec);
}
