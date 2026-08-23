import defaultValue, {
  answer,
  suffix as renamed,
  depMetaUrl,
  depResolvedDotSegment,
  depResolvedSelf,
  double,
  Box,
} from "./worker-module-http-dep.js" with {};
import * as re from "./worker-module-http-reexport.js" with {};
import { answer as starAnswer } from "./worker-module-http-star.js" with {};

const box = new Box(9);
postMessage({
  defaultValue,
  answer,
  renamed,
  doubled: double(21),
  boxValue: box.value,
  importedAnswer: re.importedAnswer,
  importedDefault: re.importedDefault,
  namespaceAnswer: re.depNamespace.answer,
  starAnswer,
  importScriptsType: typeof importScripts,
  metaUrl: import.meta.url,
  depMetaUrl,
  resolvedDep: import.meta.resolve("./worker-module-http-dep.js"),
  resolvedDotSegment: import.meta.resolve("./nested/../worker-module-http-dep.js"),
  depResolvedSelf,
  depResolvedDotSegment,
});
