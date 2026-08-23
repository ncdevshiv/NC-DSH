(function (global) {
  "use strict";

  global.__moliWptReport = {
    complete: false,
    status: {
      status: "PENDING",
      message: "",
    },
    tests: [],
  };

  function wellFormedString(value) {
    value = String(value || "");
    if (typeof value.toWellFormed === "function") {
      return value.toWellFormed();
    }
    let output = "";
    for (let index = 0; index < value.length; index += 1) {
      const code = value.charCodeAt(index);
      if (code >= 0xd800 && code <= 0xdbff) {
        const next = value.charCodeAt(index + 1);
        if (next >= 0xdc00 && next <= 0xdfff) {
          output += value[index] + value[index + 1];
          index += 1;
        } else {
          output += "\ufffd";
        }
      } else if (code >= 0xdc00 && code <= 0xdfff) {
        output += "\ufffd";
      } else {
        output += value[index];
      }
    }
    return output;
  }

  global.GLOBAL = global.GLOBAL || {
    isWindow() {
      return true;
    },
    isWorker() {
      return false;
    },
    isShadowRealm() {
      return false;
    },
  };

  function reportTest(test) {
    return {
      name: wellFormedString(test.name),
      status: wellFormedString(test.status),
      message: wellFormedString(test.message),
    };
  }

  global.add_result_callback(function (test) {
    global.__moliWptReport.tests.push(reportTest(test));
  });

  global.add_completion_callback(function (tests, status) {
    global.__moliWptReport.tests = Array.from(tests || [], reportTest);
    global.__moliWptReport.status = {
      status: wellFormedString(status.status),
      message: wellFormedString(status.message),
    };
    global.__moliWptReport.complete = true;
    document.documentElement.dataset.moliWptDone = "true";
  });
})(window);
