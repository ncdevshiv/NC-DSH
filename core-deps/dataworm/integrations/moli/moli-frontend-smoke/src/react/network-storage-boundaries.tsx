import { useEffect, useRef, useState } from "react";

import { assertFixture, captureFrame, markReady } from "../shared/harness";
import {
  runNetworkStorageBoundaryCase,
  type NetworkStorageBoundaryResult,
} from "../shared/network-storage-boundaries";
import { mountReact, type ReactCaseProps, useFrameUpdate } from "./support";

function NetworkStorageBoundaryCase({ meta, spec }: ReactCaseProps) {
  const host = useRef<HTMLDivElement>(null);
  const [result, setResult] = useState<NetworkStorageBoundaryResult>();

  useFrameUpdate(
    meta,
    async () => {
      assertFixture(host.current, "React network/storage host exists");
      setResult(
        await runNetworkStorageBoundaryCase(host.current, meta, spec, (name) =>
          captureFrame(meta, name),
        ),
      );
    },
    [meta, spec],
  );

  useEffect(() => {
    if (result?.status === "ready") {
      markReady(meta, ["mounted", "react-network-storage-ready"]);
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
  mountReact(NetworkStorageBoundaryCase, meta, spec);
}
