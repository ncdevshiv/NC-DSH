import { useEffect, type ComponentType, type DependencyList } from "react";
import { createRoot } from "react-dom/client";

import { assertFixture, captureFrame, failCase } from "../shared/harness";
import type { CaseSpec, SmokeMeta } from "../shared/types";

export interface ReactCaseProps {
  meta: SmokeMeta;
  spec: CaseSpec;
}

export function useFrameUpdate(
  meta: SmokeMeta,
  update: () => void | Promise<void>,
  dependencies: DependencyList,
): void {
  useEffect(() => {
    let active = true;
    void (async () => {
      await captureFrame(meta, "mounted");
      if (active) {
        await update();
      }
    })().catch(failCase);
    return () => {
      active = false;
    };
  }, dependencies);
}

export function mountReact(Component: ComponentType<ReactCaseProps>, meta: SmokeMeta, spec: CaseSpec): void {
  const container = document.getElementById("app");
  assertFixture(container, "React root exists");
  createRoot(container).render(<Component meta={meta} spec={spec} />);
}
