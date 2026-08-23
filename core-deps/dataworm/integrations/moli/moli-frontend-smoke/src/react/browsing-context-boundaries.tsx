import { useEffect, useRef, useState } from "react";

import { assertFixture, captureFrame, markReady } from "../shared/harness";
import {
  runBrowsingContextBoundaryCase,
  type BrowsingContextBoundaryResult,
} from "../shared/browsing-context-boundaries";
import { mountReact, type ReactCaseProps, useFrameUpdate } from "./support";

function BrowsingContextBoundaryCase({ meta, spec }: ReactCaseProps) {
  const host = useRef<HTMLDivElement>(null);
  const [result, setResult] = useState<BrowsingContextBoundaryResult>();

  useFrameUpdate(
    meta,
    async () => {
      assertFixture(host.current, "React browsing-context host exists");
      setResult(
        await runBrowsingContextBoundaryCase(host.current, meta, spec, (name) =>
          captureFrame(meta, name),
        ),
      );
    },
    [meta, spec],
  );

  useEffect(() => {
    if (result?.status === "ready") {
      markReady(meta, ["mounted", "react-browsing-context-ready"]);
    }
  }, [meta, result]);

  return (
    <main
      id="smoke-root"
      data-framework="react"
      data-family={meta.family}
      data-mode={result?.status ?? "loading"}
    >
      <h1>{meta.title}</h1>
      <section data-case-body>
        <div ref={host} data-boundary-host />
        {result && (
          <dl data-boundary-facts>
            {result.facts.map((item) => (
              <div key={item.name} data-fact={item.name}>
                <dt>{item.name}</dt>
                <dd>{item.value}</dd>
              </div>
            ))}
          </dl>
        )}
      </section>
    </main>
  );
}

export function mount(meta: ReactCaseProps["meta"], spec: ReactCaseProps["spec"]): void {
  mountReact(BrowsingContextBoundaryCase, meta, spec);
}
