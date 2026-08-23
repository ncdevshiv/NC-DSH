use super::*;

#[test]
fn window_queue_microtask_uses_typed_callback_and_v8_fifo() {
    let mut vm = new_storage_test_vm("https://queue-microtask.test/");

    let initial = vm
        .eval(
            r#"
JSON.stringify((() => {
  globalThis.__queueMicrotaskEvents = [];
  globalThis.__queueMicrotaskErrors = [];
  const events = __queueMicrotaskEvents;
  const conversionErrors = [];
  for (const invoke of [
    () => queueMicrotask(),
    () => queueMicrotask(null),
    () => queueMicrotask({})
  ]) {
    try {
      invoke();
      conversionErrors.push("missing");
    } catch (error) {
      conversionErrors.push(error.name);
    }
  }

  onerror = (_message, _source, _line, _column, error) => {
    __queueMicrotaskErrors.push(error && error.name);
    return true;
  };

  const callback = new Proxy(
    function() {
      "use strict";
      events.push(`callback:${this === undefined}:${arguments.length}`);
      queueMicrotask(() => events.push("nested"));
    },
    {
      apply(target, receiver, args) {
        events.push(`apply:${receiver === undefined}:${args.length}`);
        return Reflect.apply(target, receiver, args);
      }
    }
  );
  queueMicrotask(callback);
  Promise.resolve().then(() => events.push("promise"));
  queueMicrotask(() => events.push("second"));

  const revoked = Proxy.revocable(function() {}, {});
  revoked.revoke();
  let revokedAccepted = true;
  try {
    queueMicrotask(revoked.proxy);
  } catch {
    revokedAccepted = false;
  }
  events.push("sync");

  return { conversionErrors, revokedAccepted, events: [...events] };
})())
"#,
        )
        .expect("Window queueMicrotask setup should evaluate");
    assert_eq!(
        initial,
        r#"{"conversionErrors":["TypeError","TypeError","TypeError"],"revokedAccepted":true,"events":["sync"]}"#
    );

    assert_eq!(
        vm.eval(
            r#"
JSON.stringify({
  events: __queueMicrotaskEvents,
  errors: __queueMicrotaskErrors
})
"#
        )
        .expect("Window queueMicrotask results should evaluate"),
        r#"{"events":["sync","apply:true:0","callback:true:0","promise","second","nested"],"errors":["TypeError"]}"#
    );
}

#[test]
fn window_queue_microtask_separates_target_and_callback_realms() {
    let mut vm = new_storage_test_vm("https://queue-microtask-realms.test/");
    vm.eval(
        r#"
(() => {
  const root =
    document.documentElement ||
    document.appendChild(document.createElement("html"));
  const body = document.body || root.appendChild(document.createElement("body"));
  const frame = document.createElement("iframe");
  body.appendChild(frame);
  globalThis.__queueMicrotaskFrame = frame;
  globalThis.__queueMicrotaskRealmEvents = [];
})()
"#,
    )
    .expect("queueMicrotask child Realm setup should evaluate");
    materialize_single_child_default_realm_for_test(&mut vm, "queueMicrotask child Realm setup");

    vm.eval(
        r#"
(() => {
  const child = __queueMicrotaskFrame.contentWindow;
  child.onerror = child.Function(
    "message",
    "source",
    "line",
    "column",
    "error",
    `parent.__queueMicrotaskRealmEvents.push(
       "child-error:" + (error && error.message)
     );
     return true;`
  );
  onerror = () => {
    __queueMicrotaskRealmEvents.push("wrong-parent-error");
    return true;
  };

  const parentCallback = function() {
    "use strict";
    __queueMicrotaskRealmEvents.push(
      `parent-callback:${globalThis === window}:${this === undefined}:${arguments.length}`
    );
  };
  const childThrow = child.Function(
    `"use strict";
     parent.__queueMicrotaskRealmEvents.push(
       "child-callback:" + (globalThis === parent.__queueMicrotaskFrame.contentWindow)
     );
     throw new Error("child-microtask");
    `
  );

  child.queueMicrotask(parentCallback);
  queueMicrotask(childThrow);
})()
"#,
    )
    .expect("cross-Realm queueMicrotask callbacks should queue");

    assert_eq!(
        vm.eval("JSON.stringify(__queueMicrotaskRealmEvents)")
            .expect("cross-Realm queueMicrotask callbacks should settle"),
        r#"["parent-callback:true:true:0","child-callback:true","child-error:child-microtask"]"#
    );

    vm.eval(
        r#"
(() => {
  const frame = __queueMicrotaskFrame;
  const child = frame.contentWindow;
  const retiredCallback = child.Function(
    `parent.__queueMicrotaskRealmEvents.push("retired-callback-ran");`
  );
  queueMicrotask(retiredCallback);
  child.queueMicrotask(() => {
    __queueMicrotaskRealmEvents.push("live-callback-after-target-retirement");
  });
  frame.remove();
})()
"#,
    )
    .expect("queueMicrotask retirement callbacks should queue");

    assert_eq!(
        vm.eval("JSON.stringify(__queueMicrotaskRealmEvents)")
            .expect("queueMicrotask retirement callbacks should settle"),
        r#"["parent-callback:true:true:0","child-callback:true","child-error:child-microtask","live-callback-after-target-retirement"]"#
    );
}
