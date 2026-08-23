self.onmessage = function (event) {
  const mode = event.data && event.data.mode;

  if (mode === "global-scope") {
    let workerConstructorError = null;
    let dedicatedConstructorError = null;
    try {
      new WorkerGlobalScope();
    } catch (error) {
      workerConstructorError = error && error.name;
    }
    try {
      new DedicatedWorkerGlobalScope();
    } catch (error) {
      dedicatedConstructorError = error && error.name;
    }
    postMessage({
      hasWorkerGlobalScope: Object.prototype.hasOwnProperty.call(self, "WorkerGlobalScope"),
      hasDedicatedWorkerGlobalScope:
        Object.prototype.hasOwnProperty.call(self, "DedicatedWorkerGlobalScope"),
      workerCtorType: typeof WorkerGlobalScope,
      dedicatedCtorType: typeof DedicatedWorkerGlobalScope,
      workerCtorName: WorkerGlobalScope && WorkerGlobalScope.name,
      dedicatedCtorName: DedicatedWorkerGlobalScope && DedicatedWorkerGlobalScope.name,
      selfIsWorkerGlobalScope: self instanceof WorkerGlobalScope,
      selfIsDedicatedWorkerGlobalScope: self instanceof DedicatedWorkerGlobalScope,
      dedicatedExtendsWorker:
        DedicatedWorkerGlobalScope.prototype instanceof WorkerGlobalScope,
      globalPrototypeIsDedicated:
        Object.getPrototypeOf(self) === DedicatedWorkerGlobalScope.prototype,
      dedicatedPrototypeParentIsWorker:
        Object.getPrototypeOf(DedicatedWorkerGlobalScope.prototype) ===
        WorkerGlobalScope.prototype,
      workerTag: Object.prototype.toString.call(WorkerGlobalScope.prototype),
      dedicatedTag: Object.prototype.toString.call(DedicatedWorkerGlobalScope.prototype),
      selfTag: Object.prototype.toString.call(self),
      constructorOnWorkerPrototype:
        WorkerGlobalScope.prototype.constructor === WorkerGlobalScope,
      constructorOnDedicatedPrototype:
        DedicatedWorkerGlobalScope.prototype.constructor === DedicatedWorkerGlobalScope,
      dedicatedConstructorInheritsWorker:
        Object.getPrototypeOf(DedicatedWorkerGlobalScope) === WorkerGlobalScope,
      workerConstructorError: workerConstructorError,
      dedicatedConstructorError: dedicatedConstructorError,
    });
    close();
    return;
  }

  if (mode === "request-url") {
    const request = new Request("./data.txt");
    const url = new URL("./data.txt?x=1&x=2", location.href);
    const params = new URLSearchParams("a=1&a=2&b=3");
    postMessage({
      requestUrl: request.url,
      urlCtor: typeof URL,
      searchParamsCtor: typeof URLSearchParams,
      href: url.href,
      origin: url.origin,
      pathname: url.pathname,
      search: url.search,
      xValues: url.searchParams.getAll("x"),
      paramsString: params.toString(),
      canParseRelative: URL.canParse("./next.js", location.href),
      canParseInvalidBase: URL.canParse("./next.js", "not a url"),
      urlTag: Object.prototype.toString.call(url),
      paramsTag: Object.prototype.toString.call(params),
    });
    close();
    return;
  }

  if (mode === "navigator") {
    (async function () {
      "use strict";

      let assignError = null;
      try {
        navigator.appName = "";
      } catch (error) {
        assignError = error && error.name;
      }
      const proto = Object.getPrototypeOf(navigator);
      const userAgent = Object.getOwnPropertyDescriptor(proto, "userAgent");
      const userAgentData = Object.getOwnPropertyDescriptor(proto, "userAgentData");
      const uaData = navigator.userAgentData;
      const emptyHighEntropy = await uaData.getHighEntropyValues([]);
      const architectureHighEntropy = await uaData.getHighEntropyValues(["architecture"]);
      postMessage({
        navigatorOwn: Object.prototype.hasOwnProperty.call(self, "navigator"),
        ctorOwn: Object.prototype.hasOwnProperty.call(self, "WorkerNavigator"),
        navigatorType: typeof navigator,
        ctorType: typeof WorkerNavigator,
        ctorName: navigator.constructor && navigator.constructor.name,
        protoCtor: proto && proto.constructor && proto.constructor.name,
        tag: Object.prototype.toString.call(navigator),
        instanceofWorkerNavigator: navigator instanceof WorkerNavigator,
        ownUserAgent: Object.prototype.hasOwnProperty.call(navigator, "userAgent"),
        userAgentGetterType: typeof (userAgent && userAgent.get),
        userAgentDataGetterType: typeof (userAgentData && userAgentData.get),
        appCodeName: navigator.appCodeName,
        appName: navigator.appName,
        platformType: typeof navigator.platform,
        product: navigator.product,
        language: navigator.language,
        languages: Array.from(navigator.languages || []),
        onLineType: typeof navigator.onLine,
        hardwareConcurrencyPositive: navigator.hardwareConcurrency > 0,
        deviceMemoryPositive:
          typeof navigator.deviceMemory === "number" && navigator.deviceMemory > 0,
        uaDataCtorOwn: Object.prototype.hasOwnProperty.call(self, "NavigatorUAData"),
        uaDataCtorType: typeof NavigatorUAData,
        uaDataType: typeof uaData,
        uaDataSameObject: uaData === navigator.userAgentData,
        uaDataInstance: uaData instanceof NavigatorUAData,
        uaDataTag: Object.prototype.toString.call(uaData),
        uaDataJsonKeys: Object.keys(uaData.toJSON()),
        emptyHighEntropyKeys: Object.keys(emptyHighEntropy),
        architectureHighEntropyKeys: Object.keys(architectureHighEntropy),
        architectureType: typeof architectureHighEntropy.architecture,
        architectureOmitsFormFactors: !("formFactors" in architectureHighEntropy),
        assignError: assignError,
      });
      close();
    })().catch((error) => {
      postMessage({ probeError: error && error.name });
      close();
    });
    return;
  }

  if (mode === "location") {
    const beforeHref = location.href;
    location.href = "https://example.com/ignored";
    const proto = Object.getPrototypeOf(location);
    const href = Object.getOwnPropertyDescriptor(proto, "href");
    postMessage({
      locationOwn: Object.prototype.hasOwnProperty.call(self, "location"),
      ctorOwn: Object.prototype.hasOwnProperty.call(self, "WorkerLocation"),
      ctorType: typeof WorkerLocation,
      ctorName: location.constructor && location.constructor.name,
      protoCtor: proto && proto.constructor && proto.constructor.name,
      tag: Object.prototype.toString.call(location),
      stringified: String(location),
      instanceofWorkerLocation: location instanceof WorkerLocation,
      ownHref: Object.prototype.hasOwnProperty.call(location, "href"),
      hrefGetterType: typeof (href && href.get),
      href: location.href,
      origin: location.origin,
      protocol: location.protocol,
      host: location.host,
      hostname: location.hostname,
      port: location.port,
      pathname: location.pathname,
      search: location.search,
      hash: location.hash,
      unchanged: location.href === beforeHref,
    });
    close();
    return;
  }

  if (mode === "domexception") {
    const exception = new DOMException("bad data", "InvalidCharacterError");
    const descriptor = Object.getOwnPropertyDescriptor(DOMException, "INVALID_CHARACTER_ERR");
    postMessage({
      ctorType: typeof DOMException,
      instance: exception instanceof DOMException,
      name: exception.name,
      message: exception.message,
      code: exception.code,
      constructorConstant: DOMException.INVALID_CHARACTER_ERR,
      prototypeConstant: DOMException.prototype.INVALID_CHARACTER_ERR,
      inheritedConstant: exception.INVALID_CHARACTER_ERR,
      writable: descriptor && descriptor.writable,
      enumerable: descriptor && descriptor.enumerable,
      configurable: descriptor && descriptor.configurable,
    });
    close();
    return;
  }

  if (mode === "base64") {
    let thrown = null;
    try {
      btoa("\u2713");
    } catch (error) {
      thrown = error;
    }
    postMessage({
      atobType: typeof atob,
      btoaType: typeof btoa,
      atobLength: atob.length,
      btoaLength: btoa.length,
      encoded: btoa("worker"),
      binaryEncoded: btoa("\x00\xff"),
      decoded: atob("d29ya2Vy"),
      whitespaceDecoded: atob(" Y\tQ\n==\r"),
      omittedPaddingDecoded: atob("YQ"),
      errorName: thrown && thrown.name,
      errorCode: thrown && thrown.code,
      errorIsDomException: thrown instanceof DOMException,
      errorLegacyConstant: DOMException.INVALID_CHARACTER_ERR,
    });
    close();
    return;
  }

  postMessage({ mode: mode, error: "unknown mode" });
  close();
};
