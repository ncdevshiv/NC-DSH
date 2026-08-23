import { Fragment, type ReactNode, useEffect, useState } from "react";

import { deterministicWords } from "../shared/data";
import { markReady } from "../shared/harness";
import { mountReact, type ReactCaseProps, useFrameUpdate } from "./support";

function deepTree(depth: number, child: ReactNode): ReactNode {
  let current = child;
  for (let index = depth; index > 0; index -= 1) {
    current = <div data-depth={index}>{current}</div>;
  }
  return current;
}

function StructureCase({ meta, spec }: ReactCaseProps) {
  const [expanded, setExpanded] = useState(false);
  useFrameUpdate(meta, () => setExpanded(true), []);
  useEffect(() => {
    if (expanded) markReady(meta, ["mounted", "react-structure-committed"]);
  }, [expanded, meta]);

  const body = (() => {
    switch (spec.variant) {
      case 0:
        return expanded ? <p data-branch="present">The conditional branch is present.</p> : null;
      case 1:
        return expanded ? <aside data-mode="expanded">Expanded details</aside> : <p>Compact</p>;
      case 2:
        return <><span>Alpha</span><Fragment><b>Beta</b><i>Gamma</i></Fragment><span>Delta</span></>;
      case 3:
        return <template data-kind="card"><article><h2>Template content</h2><p>Retained subtree</p></article></template>;
      case 4:
        return <div data-comment-host dangerouslySetInnerHTML={{ __html: "<!--react-marker--><span>After marker</span>" }} />;
      case 5:
        return <details open={expanded}><summary>Compatibility details</summary><p>{deterministicWords(spec.seed, 12)}</p></details>;
      case 6:
        return <dl><div><dt>Engine</dt><dd>Moli</dd></div><div><dt>Reference</dt><dd>Chromium</dd></div></dl>;
      case 7:
        return <article><header><h2>Semantic article</h2></header><nav aria-label="Article"><a href="#intro">Intro</a></nav><section id="intro"><p>Body</p></section><footer>End</footer></article>;
      case 8:
        return deepTree(14, <strong id="deep-leaf">Deep leaf {spec.seed}</strong>);
      default:
        return <>{Array.from({ length: 12 }, (_, index) => <section key={index} aria-labelledby={`heading-${index}`}><h2 id={`heading-${index}`}>Section {index + 1}</h2><p>{deterministicWords(spec.seed + index, 18)}</p></section>)}</>;
    }
  })();

  return <main id="smoke-root" data-framework="react" data-family={meta.family} data-expanded={String(expanded)}><h1>{meta.title}</h1><section data-case-body>{body}</section></main>;
}

export function mount(meta: ReactCaseProps["meta"], spec: ReactCaseProps["spec"]): void {
  mountReact(StructureCase, meta, spec);
}
