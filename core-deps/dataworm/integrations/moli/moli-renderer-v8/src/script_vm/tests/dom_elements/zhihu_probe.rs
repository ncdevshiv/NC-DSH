use super::*;

#[test]
fn zhihu_probe_canvas_shape_matches_chromium_branding() {
    let mut vm = new_storage_test_vm("https://example.com/");

    let result = vm
        .eval(
            r#"
            (() => {
              const canvas = document.createElement("canvas");
              const proto = HTMLCanvasElement.prototype;
              return [
                canvas.constructor && canvas.constructor.name,
                Object.prototype.toString.call(canvas),
                Object.prototype.hasOwnProperty.call(canvas, "getContext"),
                Object.prototype.hasOwnProperty.call(canvas, "toDataURL"),
                typeof proto.getContext,
                typeof proto.toDataURL,
                typeof proto.transferControlToOffscreen,
                canvas instanceof HTMLCanvasElement
              ].join("|");
            })()
            "#,
        )
        .expect("canvas branding probe should evaluate");

    assert_eq!(
        result,
        "HTMLCanvasElement|[object HTMLCanvasElement]|false|false|function|function|function|true"
    );
}
#[test]
fn zhihu_probe_server_escape_globals_are_absent_even_via_function_constructor() {
    let mut vm = new_storage_test_vm("https://example.com/");

    let result = vm
        .eval(
            r#"
            (() => {
              const probe = new Function(
                "return [typeof process, typeof global, typeof require, typeof window.process, typeof window.global].join('|')"
              );
              return probe();
            })()
            "#,
        )
        .expect("server escape probe should evaluate");

    assert_eq!(result, "undefined|undefined|undefined|undefined|undefined");
}
#[test]
fn zhihu_probe_live_trace_function_constructor_checks_match_expected_shape() {
    let mut vm = new_storage_test_vm("https://example.com/");

    let result = vm
        .eval(
            r#"
            (() => {
              const sources = [
                "return typeof process !== 'undefined' ? 'y':'n'",
                "return typeof require !== 'undefined' ? 'y':'n'",
                "return typeof module !== 'undefined' ? 'y':'n'",
                "return typeof exports !== 'undefined' ? 'y':'n'",
                "return typeof __dirname !== 'undefined' ? 'y':'n'",
                "return typeof __filename !== 'undefined' ? 'y':'n'",
                "return typeof window._parent !== 'undefined' ? 'y':'n'",
                "return typeof window._origin !== 'undefined' ? 'y':'n'",
                "return typeof window._document !== 'undefined' ? 'y':'n'",
                "return typeof window._virtualConsole !== 'undefined' ? 'y':'n'",
                "return typeof ws !== 'undefined' && ws.toString().indexOf('ocke') !== -1 ? 'y':'n'",
                "return typeof JSDOM !== 'undefined' ? 'y':'n'",
                "try{return window.constructor.toString()}catch(_){return ''}",
                "return typeof Hlclient !== 'undefined' ? 'y':'n'",
                "return typeof v_saf !== 'undefined' ? 'y':'n'"
              ];
              const probes = {};
              for (const source of sources) {
                probes[source] = new Function(source)();
              }
              return JSON.stringify({
                windowConstructorName: window.constructor && window.constructor.name,
                windowConstructorString: String(window.constructor),
                probes,
              });
            })()
            "#,
        )
        .expect("function constructor trace probe should evaluate");

    let value: serde_json::Value =
        serde_json::from_str(&result).expect("function constructor trace probe should be json");
    assert_eq!(value["windowConstructorName"], "Window");
    assert_eq!(
        value["windowConstructorString"],
        "function Window() { [native code] }"
    );

    let probes = value["probes"]
        .as_object()
        .expect("trace probes should be an object");
    for key in [
        "return typeof process !== 'undefined' ? 'y':'n'",
        "return typeof require !== 'undefined' ? 'y':'n'",
        "return typeof module !== 'undefined' ? 'y':'n'",
        "return typeof exports !== 'undefined' ? 'y':'n'",
        "return typeof __dirname !== 'undefined' ? 'y':'n'",
        "return typeof __filename !== 'undefined' ? 'y':'n'",
        "return typeof window._parent !== 'undefined' ? 'y':'n'",
        "return typeof window._origin !== 'undefined' ? 'y':'n'",
        "return typeof window._document !== 'undefined' ? 'y':'n'",
        "return typeof window._virtualConsole !== 'undefined' ? 'y':'n'",
        "return typeof ws !== 'undefined' && ws.toString().indexOf('ocke') !== -1 ? 'y':'n'",
        "return typeof JSDOM !== 'undefined' ? 'y':'n'",
        "return typeof Hlclient !== 'undefined' ? 'y':'n'",
        "return typeof v_saf !== 'undefined' ? 'y':'n'",
    ] {
        assert_eq!(
            probes.get(key),
            Some(&serde_json::Value::String("n".to_owned()))
        );
    }
    assert_eq!(
        probes.get("try{return window.constructor.toString()}catch(_){return ''}"),
        Some(&serde_json::Value::String(
            "function Window() { [native code] }".to_owned()
        ))
    );
}
#[test]
fn zhihu_probe_common_automation_markers_are_absent() {
    let mut vm = new_storage_test_vm("https://example.com/");

    let result = vm
        .eval(
            r#"
            (() => JSON.stringify(
              [
                "__phantomas",
                "_phantom",
                "WebPage",
                "fxdriver_id",
                "__fxdriver_unwrapped",
                "ubot",
                "CasperError",
                "casper",
                "$cdc_asdjflasutopfhvcZLmcfl_"
              ].map((key) => [key, typeof window[key]])
            ))()
            "#,
        )
        .expect("automation marker probe should evaluate");

    assert_eq!(
        result,
        r#"[["__phantomas","undefined"],["_phantom","undefined"],["WebPage","undefined"],["fxdriver_id","undefined"],["__fxdriver_unwrapped","undefined"],["ubot","undefined"],["CasperError","undefined"],["casper","undefined"],["$cdc_asdjflasutopfhvcZLmcfl_","undefined"]]"#
    );
}
#[test]
fn zhihu_probe_jsdom_and_server_escape_markers_are_absent() {
    let mut vm = new_storage_test_vm("https://example.com/");

    let result = vm
        .eval(
            r#"
            (() => {
              const windowMarkers = [
                "_resourceLoader",
                "_sessionHistory",
                "_virtualConsole",
                "_parent",
                "_origin",
                "_document",
                "patchRequire",
                "__webdriver_script_fn",
                "process",
                "global"
              ];
              const functionMarkers = [
                "process",
                "global",
                "require",
                "module",
                "exports",
                "__dirname",
                "__filename",
                "executeUserEntryPoint",
                "node",
                "JSDOM",
                "ws",
                "_resourceLoader",
                "_sessionHistory"
              ];
              return JSON.stringify({
                window: Object.fromEntries(
                  windowMarkers.map((key) => [key, typeof window[key]])
                ),
                dynamic: Object.fromEntries(
                  functionMarkers.map((key) => [
                    key,
                    new Function(`return typeof ${key}`)()
                  ])
                )
              });
            })()
            "#,
        )
        .expect("jsdom/server escape marker probe should evaluate");

    assert_eq!(
        result,
        r#"{"window":{"_resourceLoader":"undefined","_sessionHistory":"undefined","_virtualConsole":"undefined","_parent":"undefined","_origin":"undefined","_document":"undefined","patchRequire":"undefined","__webdriver_script_fn":"undefined","process":"undefined","global":"undefined"},"dynamic":{"process":"undefined","global":"undefined","require":"undefined","module":"undefined","exports":"undefined","__dirname":"undefined","__filename":"undefined","executeUserEntryPoint":"undefined","node":"undefined","JSDOM":"undefined","ws":"undefined","_resourceLoader":"undefined","_sessionHistory":"undefined"}}"#
    );
}
