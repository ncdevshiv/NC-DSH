import { runParserDocumentBoundaryCase } from "./parser-document-boundaries";
import { runDomObserverReactionCase } from "./dom-observer-reactions";
import { runFormValidationAssociationCase } from "./form-validation-association";
import { runWorkerMessagingBoundaryCase } from "./worker-messaging-boundaries";
import { runHistoryNavigationBoundaryCase } from "./history-navigation-boundaries";
import { runStreamsEncodingBoundaryCase } from "./streams-encoding-boundaries";
import { runUrlFileBinaryBoundaryCase } from "./url-file-binary-boundaries";
import { runEventShadowFocusBoundaryCase } from "./event-shadow-focus-boundaries";
import { runServiceWorkerCacheBoundaryCase } from "./service-worker-cache-boundaries";
import { runCorsCredentialsBoundaryCase } from "./cors-credentials-boundaries";
import { runCustomElementsFormBoundaryCase } from "./custom-elements-form-boundaries";
import { runStorageLifecycleBoundaryCase } from "./storage-lifecycle-boundaries";
import { runScriptModuleLifecycleCase } from "./script-module-lifecycle";
import { runRealtimeTransportBoundaryCase } from "./realtime-transport-boundaries";
import type {
  CapturePlatformFrame,
  PlatformBoundaryResult,
} from "./platform-boundary";
import type { CaseSpec, SmokeMeta } from "./types";

export type AdvancedPlatformResult = PlatformBoundaryResult;

export async function runAdvancedPlatformCase(
  host: HTMLElement,
  meta: SmokeMeta,
  spec: CaseSpec,
  capture: CapturePlatformFrame,
): Promise<AdvancedPlatformResult> {
  switch (meta.family) {
    case "parser-document-boundaries":
      return runParserDocumentBoundaryCase(host, meta, spec, capture);
    case "dom-observer-reactions":
      return runDomObserverReactionCase(host, meta, spec, capture);
    case "form-validation-association":
      return runFormValidationAssociationCase(host, meta, spec, capture);
    case "worker-messaging-boundaries":
      return runWorkerMessagingBoundaryCase(host, meta, spec, capture);
    case "history-navigation-boundaries":
      return runHistoryNavigationBoundaryCase(host, meta, spec, capture);
    case "streams-encoding-boundaries":
      return runStreamsEncodingBoundaryCase(host, meta, spec, capture);
    case "url-file-binary-boundaries":
      return runUrlFileBinaryBoundaryCase(host, meta, spec, capture);
    case "event-shadow-focus-boundaries":
      return runEventShadowFocusBoundaryCase(host, meta, spec, capture);
    case "service-worker-cache-boundaries":
      return runServiceWorkerCacheBoundaryCase(host, meta, spec, capture);
    case "cors-credentials-boundaries":
      return runCorsCredentialsBoundaryCase(host, meta, spec, capture);
    case "custom-elements-form-boundaries":
      return runCustomElementsFormBoundaryCase(host, meta, spec, capture);
    case "storage-lifecycle-boundaries":
      return runStorageLifecycleBoundaryCase(host, meta, spec, capture);
    case "script-module-lifecycle":
      return runScriptModuleLifecycleCase(host, meta, spec, capture);
    case "realtime-transport-boundaries":
      return runRealtimeTransportBoundaryCase(host, meta, spec, capture);
    default:
      throw new Error(`unsupported advanced platform family: ${meta.family}`);
  }
}
