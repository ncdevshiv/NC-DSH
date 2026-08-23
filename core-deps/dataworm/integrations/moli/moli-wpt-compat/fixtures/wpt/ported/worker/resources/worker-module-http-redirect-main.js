import {
  entryMetaUrl,
  leafMetaUrl,
  leafValue,
} from "/wpt/runtime/worker/module-redirect/start.js";
import {
  entryMetaUrl as fragmentEntryMetaUrl,
  leafMetaUrl as fragmentLeafMetaUrl,
} from "/wpt/runtime/worker/module-redirect/fragment-start.js#request-fragment";

Promise.all([
  import("/wpt/runtime/worker/module-redirect/start.js"),
  import("/wpt/runtime/worker/module-redirect/fragment-start.js#request-fragment"),
]).then(function ([dynamicEntry, dynamicFragmentEntry]) {
  postMessage({
    dynamicEntryMetaUrl: dynamicEntry.entryMetaUrl,
    dynamicFragmentEntryMetaUrl: dynamicFragmentEntry.entryMetaUrl,
    dynamicFragmentLeafMetaUrl: dynamicFragmentEntry.leafMetaUrl,
    dynamicLeafMetaUrl: dynamicEntry.leafMetaUrl,
    dynamicLeafValue: dynamicEntry.leafValue,
    entryMetaUrl,
    fragmentEntryMetaUrl,
    fragmentLeafMetaUrl,
    leafMetaUrl,
    leafValue,
    importScriptsType: typeof importScripts,
    mainMetaUrl: import.meta.url,
  });
}, function (error) {
  postMessage({ error: String(error && error.message || error) });
});
