import { useEffect, useRef, useState } from "react";

import { assertFixture, markReady } from "../shared/harness";
import {
  runWebPlatformIntegrationCase,
  type WebPlatformIntegrationResult,
} from "../shared/web-platform-integration";
import { mountReact, type ReactCaseProps, useFrameUpdate } from "./support";

function WebPlatformIntegrationCase({ meta, spec }: ReactCaseProps) {
  const host = useRef<HTMLDivElement>(null);
  const [result, setResult] = useState<WebPlatformIntegrationResult>();

  useFrameUpdate(
    meta,
    async () => {
      assertFixture(host.current, "React web-platform host exists");
      setResult(await runWebPlatformIntegrationCase(host.current, meta, spec));
    },
    [meta, spec],
  );

  useEffect(() => {
    if (result?.status === "ready") {
      markReady(meta, ["mounted", "react-web-platform-ready"]);
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
        <div ref={host} data-platform-host />
        {result && (
          <dl data-platform-facts>
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
  mountReact(WebPlatformIntegrationCase, meta, spec);
}
