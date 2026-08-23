import { useEffect, useMemo, useState } from "react";

import { deterministicWords, stableItems } from "../shared/data";
import { assertFixture, markReady } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";
import { mountReact, useFrameUpdate } from "./support";

interface Props {
  meta: SmokeMeta;
  spec: CaseSpec;
}

function TextAndAttributesCase({ meta, spec }: Props) {
  const [updated, setUpdated] = useState(false);
  const items = useMemo(() => stableItems(spec.seed, Math.min(spec.size, 12)), [spec.seed, spec.size]);

  useFrameUpdate(meta, () => {
    setUpdated(true);
  }, []);

  useEffect(() => {
    if (!updated) {
      return;
    }
    assertFixture(document.querySelector("[data-case-body]") !== null, "case body mounted");
    markReady(meta, ["mounted", "react-effect-update"]);
  }, [meta, updated]);

  const variantBody = (() => {
    switch (spec.variant) {
      case 0:
        return <p data-value={spec.seed}>Hello {spec.title}; value {spec.seed * 3}</p>;
      case 1:
        return <p>{"<strong>literal & safe</strong> \"quoted\" 'single'"}</p>;
      case 2:
        return <label>Boolean projection <input type="text" disabled={!updated} hidden={false} required={updated} readOnly /></label>;
      case 3:
        return <section aria-label={`Panel ${spec.seed}`} aria-busy={!updated} data-seed={spec.seed}>ARIA dataset</section>;
      case 4:
        return <div className={updated ? "card active selected" : "card pending"}>Class tokens</div>;
      case 5:
        return <div style={{ color: updated ? "rgb(12, 34, 56)" : "black", marginTop: `${spec.variant + 2}px` }}>Style map</div>;
      case 6:
        return <p lang="zh-Hans" dir="auto">你好，世界 — مرحبا — café — 😀 — {updated ? "更新" : "初始"}</p>;
      case 7:
        return <dl><dt>Present</dt><dd>{updated ? "value" : null}</dd><dt>Missing</dt><dd>{undefined}</dd></dl>;
      case 8:
        return <dl>{items.map((item) => <div key={item.id}><dt>{item.title}</dt><dd data-status={item.status}>{item.owner}</dd></div>)}</dl>;
      default:
        return (
          <article>
            <header><p className="eyebrow">Engineering / Browser</p><h2>{spec.title}</h2><p>{deterministicWords(spec.seed, 28)}</p></header>
            <div>{items.map((item) => <span key={item.id} data-tag={item.tags[0]}>{item.title}</span>)}</div>
          </article>
        );
    }
  })();

  return (
    <main id="smoke-root" data-framework="react" data-family={meta.family} data-updated={String(updated)}>
      <h1>{meta.title}</h1>
      <section data-case-body>{variantBody}</section>
    </main>
  );
}

export function mount(meta: SmokeMeta, spec: CaseSpec): void {
  mountReact(TextAndAttributesCase, meta, spec);
}
