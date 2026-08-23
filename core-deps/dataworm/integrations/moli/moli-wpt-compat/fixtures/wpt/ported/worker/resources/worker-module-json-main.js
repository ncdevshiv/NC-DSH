import httpConfig from "./worker-module-json-config.json" with { type: "json" };
import * as httpNamespace from "./worker-module-json-config.json" with { type: "json" };
import dataConfig from "data:application/json,%7B%22name%22%3A%22data-json%22%2C%22flag%22%3Atrue%7D" with { type: "json" };

postMessage({
  httpName: httpConfig.name,
  httpNested: httpConfig.nested.value,
  httpArray: httpConfig.items.join("|"),
  dataName: dataConfig.name,
  dataFlag: dataConfig.flag,
  namespaceDefaultName: httpNamespace.default.name,
  importScriptsType: typeof importScripts,
});
