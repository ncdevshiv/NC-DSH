import { useEffect, useRef, useState } from "react";

import { assertFixture, captureFrame, markReady } from "../shared/harness";
import {
  runAdvancedPlatformCase,
  type AdvancedPlatformResult,
} from "../shared/advanced-platform-cases";
import { mountReact, type ReactCaseProps, useFrameUpdate } from "./support";

function AdvancedPlatformCase({ meta, spec }: ReactCaseProps) {
  const host = useRef<HTMLDivElement>(null);
  const [result, setResult] = useState<AdvancedPlatformResult>();

  useFrameUpdate(
    meta,
    async () => {
      assertFixture(host.current, "React advanced platform host exists");
      setResult(
        await runAdvancedPlatformCase(host.current, meta, spec, (name) =>
          captureFrame(meta, name),
        ),
      );
    },
    [meta, spec],
  );

  useEffect(() => {
    if (result?.status === "ready") {
      markReady(meta, ["mounted", "react-advanced-platform-ready"]);
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
      <section data-case-body="">
        <div ref={host} data-platform-host="" />
        {result && (
          <dl data-platform-facts="">
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
  mountReact(AdvancedPlatformCase, meta, spec);
}
