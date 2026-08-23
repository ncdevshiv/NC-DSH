use super::*;

#[test]
fn generated_maplike_and_setlike_for_each_use_typed_callback_semantics() {
    let mut vm = new_storage_test_vm("https://generated-collection-callback.test/");

    let result = vm
        .eval(
            r#"
(() => {
  class CollectionCallbackElement extends HTMLElement {
    constructor() {
      super();
      this.internals = this.attachInternals();
    }
  }
  customElements.define("collection-callback-element", CollectionCallbackElement);
  const states = new CollectionCallbackElement().internals.states;
  states.add("--a");
  states.add("--b");
  states.add("--c");

  const sheet = new CSSStyleSheet();
  sheet.insertRule(`@font-feature-values callback_family {
    @annotation {
      first: 1;
      second: 2;
    }
  }`, 0);
  const featureValues = sheet.cssRules[0].annotation;

  const probe = (collection, mutate) => {
    const receiver = { label: "receiver" };
    const seen = [];
    let applyCount = 0;
    const callback = new Proxy(function(value, key, owner) {
      seen.push([
        this.label,
        key,
        Array.isArray(value) ? value.join(",") : value,
        owner === collection,
        arguments.length
      ].join(":"));
      mutate?.(value, key, owner);
    }, {
      apply(target, thisArg, args) {
        applyCount += 1;
        return Reflect.apply(target, thisArg, args);
      }
    });
    collection.forEach(callback, receiver);

    let omittedThisIsUndefined = false;
    collection.forEach(function() {
      "use strict";
      omittedThisIsUndefined = this === undefined;
    });

    const marker = {};
    let abruptCount = 0;
    let abruptIdentity = false;
    try {
      collection.forEach(() => {
        abruptCount += 1;
        throw marker;
      });
    } catch (error) {
      abruptIdentity = error === marker;
    }

    return {
      seen,
      applyCount,
      omittedThisIsUndefined,
      abruptCount,
      abruptIdentity
    };
  };

  const setlike = probe(states, (value, _key, owner) => {
    if (value === "--a") {
      owner.delete("--b");
      owner.add("--d");
    }
  });
  const maplike = probe(featureValues);

  const revoked = Proxy.revocable(function() {}, {});
  revoked.revoke();
  let revokedError = "";
  try {
    states.forEach(revoked.proxy);
  } catch (error) {
    revokedError = error && error.name;
  }

  return JSON.stringify({
    setlike,
    setlikeValues: [...states],
    maplike,
    revokedError
  });
})()
"#,
        )
        .expect("generated collection callback-function probe should evaluate");

    assert_eq!(
        result,
        r#"{"setlike":{"seen":["receiver:--a:--a:true:3","receiver:--c:--c:true:3","receiver:--d:--d:true:3"],"applyCount":3,"omittedThisIsUndefined":true,"abruptCount":1,"abruptIdentity":true},"setlikeValues":["--a","--c","--d"],"maplike":{"seen":["receiver:first:1:true:3","receiver:second:2:true:3"],"applyCount":2,"omittedThisIsUndefined":true,"abruptCount":1,"abruptIdentity":true},"revokedError":"TypeError"}"#
    );
}

#[tokio::test]
async fn generated_maplike_and_setlike_for_each_use_callback_relevant_realm() {
    let mut vm = new_storage_test_vm("https://generated-collection-callback-realm.test/");

    vm.eval(
        r#"
(() => {
  const root = document.documentElement || document.appendChild(document.createElement("html"));
  const body = document.body || root.appendChild(document.createElement("body"));
  const frame = document.createElement("iframe");
  body.appendChild(frame);
  globalThis.__collectionCallbackFrame = frame;
})()
"#,
    )
    .expect("generated collection callback realm setup should evaluate");
    assert_initial_about_blank_child_completed_synchronously_for_test(
        &mut vm,
        "generated collection callback realm setup",
    )
    .await;
    let _ = materialize_single_child_default_realm_for_test(
        &mut vm,
        "generated collection callback realm setup",
    );

    let result = vm
        .eval(
            r#"
(() => {
  class CollectionRealmElement extends HTMLElement {
    constructor() {
      super();
      this.internals = this.attachInternals();
    }
  }
  customElements.define("collection-realm-element", CollectionRealmElement);
  const states = new CollectionRealmElement().internals.states;
  states.add("--state");

  const sheet = new CSSStyleSheet();
  sheet.insertRule(`@font-feature-values realm_family {
    @annotation { feature: 9; }
  }`, 0);
  const featureValues = sheet.cssRules[0].annotation;

  const child = __collectionCallbackFrame.contentWindow;
  child.__collectionCallbackOwners = { states, featureValues };
  child.__collectionCallbackSeen = [];
  child.__collectionCallbackRealmMarker = "child";
  const makeCallback = child.Function(
    "kind",
    `return function(value, key, owner) {
      globalThis.__collectionCallbackSeen.push([
        globalThis.__collectionCallbackRealmMarker,
        this.receiverMarker,
        kind,
        key,
        Array.isArray(value) ? value.join(",") : value,
        owner === globalThis.__collectionCallbackOwners[kind]
      ].join(":"));
    }`
  );
  const setCallback = makeCallback("states");
  const mapCallback = makeCallback("featureValues");
  states.forEach(setCallback, { receiverMarker: "parent-this" });
  featureValues.forEach(mapCallback, { receiverMarker: "parent-this" });

  return JSON.stringify({
    setCallbackRealm:
      Object.getPrototypeOf(setCallback) === child.Function.prototype,
    mapCallbackRealm:
      Object.getPrototypeOf(mapCallback) === child.Function.prototype,
    seen: child.__collectionCallbackSeen
  });
})()
"#,
        )
        .expect("cross-Realm generated collection callbacks should evaluate");

    assert_eq!(
        result,
        r#"{"setCallbackRealm":true,"mapCallbackRealm":true,"seen":["child:parent-this:states:--state:--state:true","child:parent-this:featureValues:feature:9:true"]}"#
    );
}

#[test]
fn explicit_collection_for_each_uses_typed_call_semantics() {
    let mut vm = new_storage_test_vm("https://explicit-collection-callback.test/");

    let result = vm
        .eval(
            r#"
(() => {
  const body = document.body ||
    (document.documentElement || document.appendChild(document.createElement("html")))
      .appendChild(document.createElement("body"));
  const styleTarget = document.createElement("div");
  styleTarget.style.cssText = "color: red; font-size: 17px";
  body.appendChild(styleTarget);
  const styleMap = styleTarget.computedStyleMap();

  const fontA = new FontFace("callback-font-a", "local(Arial)");
  const fontB = new FontFace("callback-font-b", "local(Arial)");
  const fontC = new FontFace("callback-font-c", "local(Arial)");
  const fonts = document.fonts;
  fonts.clear();
  fonts.add(fontA);
  fonts.add(fontB);

  const text = document.createTextNode("abc");
  body.appendChild(text);
  const range = (start, end) => {
    const value = new Range();
    value.setStart(text, start);
    value.setEnd(text, end);
    return value;
  };
  const rangeA = range(0, 1);
  const rangeB = range(1, 2);
  const rangeC = range(2, 3);
  const ranges = new Highlight(rangeA, rangeB);
  const rangeNames = new Map([
    [rangeA, "range-a"],
    [rangeB, "range-b"],
    [rangeC, "range-c"]
  ]);

  const pristineApply = Reflect.apply;
  const originalReflectApply = Reflect.apply;
  Reflect.apply = () => {
    throw new Error("page Reflect.apply must not be observed");
  };

  const emptyHighlight = new Highlight();
  CSS.highlights.clear();
  const emptyRegistry = CSS.highlights;
  const emptyNonCallableErrors = [emptyHighlight, emptyRegistry].map(collection => {
    try {
      collection.forEach({});
      return "none";
    } catch (error) {
      return error && error.name;
    }
  });
  const highlightA = new Highlight(rangeA);
  const highlightB = new Highlight(rangeB);
  const highlightC = new Highlight(rangeC);
  CSS.highlights.set("highlight-a", highlightA);
  CSS.highlights.set("highlight-b", highlightB);
  const registry = CSS.highlights;

  const exercise = (collection, describeKey, mutate) => {
    const receiver = {};
    const seen = [];
    let applyCount = 0;
    let ownCallCount = 0;
    let shapeIsExact = true;
    const target = function(value, key, owner) {
      "use strict";
      shapeIsExact &&= this === receiver &&
        owner === collection &&
        arguments.length === 3;
      seen.push(describeKey(key));
      mutate?.(value, key, owner, seen.length);
    };
    Object.defineProperty(target, "call", {
      value() {
        ownCallCount += 1;
        throw new Error("callback.call must not be read");
      }
    });
    const callback = new Proxy(target, {
      apply(target, thisArg, args) {
        applyCount += 1;
        return pristineApply(target, thisArg, args);
      }
    });
    collection.forEach(callback, receiver);

    let omittedThisIsUndefined = false;
    const omittedStop = {};
    try {
      collection.forEach(function() {
        "use strict";
        omittedThisIsUndefined = this === undefined;
        throw omittedStop;
      });
    } catch (error) {
      if (error !== omittedStop) {
        throw error;
      }
    }

    const abruptMarker = {};
    let abruptCount = 0;
    let abruptIdentity = false;
    try {
      collection.forEach(() => {
        abruptCount += 1;
        throw abruptMarker;
      });
    } catch (error) {
      abruptIdentity = error === abruptMarker;
    }

    return {
      seen,
      applyCount,
      ownCallCount,
      shapeIsExact,
      omittedThisIsUndefined,
      abruptCount,
      abruptIdentity
    };
  };

  const style = exercise(styleMap, key => key);
  const font = exercise(fonts, key => key.family, (_value, _key, owner, count) => {
    if (count === 1) {
      owner.delete(fontB);
      owner.add(fontC);
    }
  });
  const eventCounts = exercise(performance.eventCounts, key => key);
  const highlight = exercise(ranges, key => rangeNames.get(key), (_value, _key, owner, count) => {
    if (count === 1) {
      owner.delete(rangeB);
      owner.add(rangeC);
    }
  });
  const highlightRegistry = exercise(
    registry,
    key => key,
    (_value, _key, owner, count) => {
      if (count === 1) {
        owner.delete("highlight-b");
        owner.set("highlight-c", highlightC);
      }
    }
  );
  Reflect.apply = originalReflectApply;

  const revoked = Proxy.revocable(function() {}, {});
  revoked.revoke();
  const revokedErrors = [styleMap, ranges].map(collection => {
    try {
      collection.forEach(revoked.proxy);
      return "none";
    } catch (error) {
      return error && error.name;
    }
  });

  return JSON.stringify({
    style: {
      countMatchesSize: style.seen.length === styleMap.size,
      applyMatchesSize: style.applyCount === styleMap.size,
      ownCallCount: style.ownCallCount,
      shapeIsExact: style.shapeIsExact,
      omittedThisIsUndefined: style.omittedThisIsUndefined,
      abruptCount: style.abruptCount,
      abruptIdentity: style.abruptIdentity
    },
    font,
    currentFonts: [...fonts].map(face => face.family),
    eventCounts: {
      countMatchesSize: eventCounts.seen.length === performance.eventCounts.size,
      applyMatchesSize: eventCounts.applyCount === performance.eventCounts.size,
      ownCallCount: eventCounts.ownCallCount,
      shapeIsExact: eventCounts.shapeIsExact,
      omittedThisIsUndefined: eventCounts.omittedThisIsUndefined,
      abruptCount: eventCounts.abruptCount,
      abruptIdentity: eventCounts.abruptIdentity
    },
    highlight,
    highlightRegistry,
    emptyNonCallableErrors,
    revokedErrors
  });
})()
"#,
        )
        .expect("explicit collection callback-function probe should evaluate");

    assert_eq!(
        result,
        r#"{"style":{"countMatchesSize":true,"applyMatchesSize":true,"ownCallCount":0,"shapeIsExact":true,"omittedThisIsUndefined":true,"abruptCount":1,"abruptIdentity":true},"font":{"seen":["callback-font-a","callback-font-b"],"applyCount":2,"ownCallCount":0,"shapeIsExact":true,"omittedThisIsUndefined":true,"abruptCount":1,"abruptIdentity":true},"currentFonts":["callback-font-a","callback-font-c"],"eventCounts":{"countMatchesSize":true,"applyMatchesSize":true,"ownCallCount":0,"shapeIsExact":true,"omittedThisIsUndefined":true,"abruptCount":1,"abruptIdentity":true},"highlight":{"seen":["range-a","range-c"],"applyCount":2,"ownCallCount":0,"shapeIsExact":true,"omittedThisIsUndefined":true,"abruptCount":1,"abruptIdentity":true},"highlightRegistry":{"seen":["highlight-a","highlight-c"],"applyCount":2,"ownCallCount":0,"shapeIsExact":true,"omittedThisIsUndefined":true,"abruptCount":1,"abruptIdentity":true},"emptyNonCallableErrors":["TypeError","TypeError"],"revokedErrors":["TypeError","TypeError"]}"#
    );
}

#[tokio::test]
async fn explicit_collection_for_each_uses_callback_relevant_realm() {
    let mut vm = new_storage_test_vm("https://explicit-collection-callback-realm.test/");

    vm.eval(
        r#"
(() => {
  const root = document.documentElement || document.appendChild(document.createElement("html"));
  const body = document.body || root.appendChild(document.createElement("body"));
  const frame = document.createElement("iframe");
  body.appendChild(frame);
  globalThis.__explicitCollectionCallbackFrame = frame;
})()
"#,
    )
    .expect("explicit collection callback realm setup should evaluate");
    assert_initial_about_blank_child_completed_synchronously_for_test(
        &mut vm,
        "explicit collection callback realm setup",
    )
    .await;
    let _ = materialize_single_child_default_realm_for_test(
        &mut vm,
        "explicit collection callback realm setup",
    );

    let result = vm
        .eval(
            r#"
(() => {
  const body = document.body || document.documentElement;
  const styleTarget = document.createElement("div");
  styleTarget.style.color = "red";
  body.appendChild(styleTarget);
  const styleMap = styleTarget.computedStyleMap();
  const fonts = document.fonts;
  fonts.clear();
  fonts.add(new FontFace("realm-font", "local(Arial)"));
  const eventCounts = performance.eventCounts;
  const text = document.createTextNode("x");
  body.appendChild(text);
  const range = new Range();
  range.setStart(text, 0);
  range.setEnd(text, 1);
  const highlight = new Highlight(range);
  CSS.highlights.clear();
  CSS.highlights.set("realm-highlight", highlight);
  const registry = CSS.highlights;

  const child = __explicitCollectionCallbackFrame.contentWindow;
  child.__explicitCollectionOwners = {
    styleMap,
    fonts,
    eventCounts,
    highlight,
    registry
  };
  child.__explicitCollectionSeen = {};
  child.__explicitCollectionRealmMarker = "child";
  const makeCallback = child.Function(
    "kind",
    `return function(value, key, owner) {
      if (!(kind in globalThis.__explicitCollectionSeen)) {
        globalThis.__explicitCollectionSeen[kind] = [
          globalThis.__explicitCollectionRealmMarker,
          this.receiverMarker,
          arguments.length,
          owner === globalThis.__explicitCollectionOwners[kind]
        ].join(":");
      }
    }`
  );
  const callbacks = {};
  for (const [kind, collection] of Object.entries(
    child.__explicitCollectionOwners
  )) {
    const callback = makeCallback(kind);
    callbacks[kind] =
      Object.getPrototypeOf(callback) === child.Function.prototype;
    collection.forEach(callback, { receiverMarker: "parent-this" });
  }

  return JSON.stringify({
    callbacks,
    seen: child.__explicitCollectionSeen
  });
})()
"#,
        )
        .expect("cross-Realm explicit collection callbacks should evaluate");

    assert_eq!(
        result,
        r#"{"callbacks":{"styleMap":true,"fonts":true,"eventCounts":true,"highlight":true,"registry":true},"seen":{"styleMap":"child:parent-this:3:true","fonts":"child:parent-this:3:true","eventCounts":"child:parent-this:3:true","highlight":"child:parent-this:3:true","registry":"child:parent-this:3:true"}}"#
    );
}
