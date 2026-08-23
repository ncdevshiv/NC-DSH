async function run() {
  const awaited = await import("./worker-module-http-dynamic-import-awaited.js");
  const promise = import("./worker-module-http-dynamic-import-promised.js");
  const promised = await promise;
  const template = await import(`./worker-module-http-dynamic-import-awaited.js`);
  const concatenated = await import("./worker-module-http-dynamic-" + "import-awaited.js");
  const dotSegment = await import("./nested/../worker-module-http-dynamic-import-awaited.js");
  const withOptions = await import("./worker-module-http-dynamic-import-promised.js", {});
  const queryOne = await import("./worker-module-http-query-dep.js?case=dynamic-one");
  const sameQueryOne = await import("./worker-module-http-query-dep.js?case=dynamic-one");
  const queryTwo = await import("./worker-module-http-query-dep.js?case=dynamic-two");
  const fragmentOne = await import("./worker-module-http-fragment-dep.js#dynamic-one");
  const sameFragmentOne = await import("./worker-module-http-fragment-dep.js#dynamic-one");
  const fragmentTwo = await import("./worker-module-http-fragment-dep.js#dynamic-two");
  const queryOneFirst = queryOne.bump();
  const queryOneSecond = sameQueryOne.bump();
  const queryTwoFirst = queryTwo.bump();
  const fragmentOneFirst = fragmentOne.bump();
  const fragmentOneSecond = sameFragmentOne.bump();
  const fragmentTwoFirst = fragmentTwo.bump();
  postMessage({
    awaitedAnswer: awaited.answer,
    awaitedDefault: awaited.default,
    promisedAnswer: promised.answer,
    promisedDefault: promised.default,
    templateAnswer: template.answer,
    concatenatedDefault: concatenated.default,
    dotSegmentAnswer: dotSegment.answer,
    dotSegmentUrl: dotSegment.moduleUrl,
    optionsAnswer: withOptions.answer,
    promisedUrl: promised.moduleUrl,
    resolved: import.meta.resolve("./worker-module-http-dynamic-import-awaited.js"),
    resolvedTemplate: import.meta.resolve(`./worker-module-http-dynamic-import-awaited.js`),
    resolvedConcatenated: import.meta.resolve("./worker-module-http-dynamic-" + "import-awaited.js"),
    resolvedDotSegment: import.meta.resolve("./nested/../worker-module-http-dynamic-import-awaited.js"),
    resolvedQueryOne: import.meta.resolve("./worker-module-http-query-dep.js?case=dynamic-one"),
    resolvedQueryTwo: import.meta.resolve("./worker-module-http-query-dep.js?case=dynamic-two"),
    resolvedFragmentOne: import.meta.resolve("./worker-module-http-fragment-dep.js#dynamic-one"),
    resolvedFragmentTwo: import.meta.resolve("./worker-module-http-fragment-dep.js#dynamic-two"),
    queryOneUrl: queryOne.moduleUrl,
    sameQueryOneUrl: sameQueryOne.moduleUrl,
    queryTwoUrl: queryTwo.moduleUrl,
    queryOneFirst,
    queryOneSecond,
    sameQueryOneCounter: sameQueryOne.counter,
    queryTwoFirst,
    queryTwoCounter: queryTwo.counter,
    fragmentOneUrl: fragmentOne.moduleUrl,
    sameFragmentOneUrl: sameFragmentOne.moduleUrl,
    fragmentTwoUrl: fragmentTwo.moduleUrl,
    fragmentOneFirst,
    fragmentOneSecond,
    sameFragmentOneCounter: sameFragmentOne.counter,
    fragmentTwoFirst,
    fragmentTwoCounter: fragmentTwo.counter,
    metaUrl: import.meta.url,
    importScriptsType: typeof importScripts,
  });
}

run().catch(function (error) {
  postMessage({ error: String(error && error.message || error) });
});
