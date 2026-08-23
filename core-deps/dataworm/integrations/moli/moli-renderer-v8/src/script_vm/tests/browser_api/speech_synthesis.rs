use super::*;

#[test]
fn speech_synthesis_surface_matches_chromium_non_playback_contract() {
    let mut vm = new_storage_test_vm("https://speech-synthesis.test/");

    let result = vm
        .eval(
            r#"
            (() => {
              const descriptor = (owner, name) => {
                const value = Object.getOwnPropertyDescriptor(owner, name);
                return [
                  typeof value?.get,
                  value?.get?.name,
                  value?.get?.length,
                  typeof value?.set,
                  value?.set?.name,
                  value?.set?.length,
                  value?.enumerable,
                  value?.configurable,
                  typeof value?.value,
                  value?.value?.name,
                  value?.value?.length,
                  value?.writable
                ];
              };
              const throwsTypeError = callback => {
                try {
                  callback();
                  return false;
                } catch (error) {
                  return error instanceof TypeError;
                }
              };
              const synthesis = speechSynthesis;
              const firstVoices = synthesis.getVoices();
              const secondVoices = synthesis.getVoices();
              let listenerCalls = 0;
              const listener = () => listenerCalls++;
              synthesis.addEventListener("voiceschanged", listener);
              synthesis.dispatchEvent(new Event("voiceschanged"));
              synthesis.removeEventListener("voiceschanged", listener);
              synthesis.dispatchEvent(new Event("voiceschanged"));

              return JSON.stringify({
                windowDescriptor: descriptor(window, "speechSynthesis"),
                sameObject: synthesis === speechSynthesis,
                objectShape: [
                  Object.prototype.toString.call(synthesis),
                  Object.getPrototypeOf(synthesis) === SpeechSynthesis.prototype,
                  Object.getPrototypeOf(SpeechSynthesis.prototype) === EventTarget.prototype,
                  synthesis instanceof SpeechSynthesis,
                  synthesis instanceof EventTarget,
                  Object.getOwnPropertyNames(synthesis).length
                ],
                constructorShape: [
                  descriptor(window, "SpeechSynthesis"),
                  throwsTypeError(() => new SpeechSynthesis()),
                  throwsTypeError(() => SpeechSynthesis())
                ],
                methodDescriptors: ["speak", "cancel", "pause", "resume", "getVoices"]
                  .map(name => [name, descriptor(SpeechSynthesis.prototype, name)]),
                attributeDescriptors: ["pending", "speaking", "paused", "onvoiceschanged"]
                  .map(name => [name, descriptor(SpeechSynthesis.prototype, name)]),
                initialState: [
                  synthesis.pending,
                  synthesis.speaking,
                  synthesis.paused,
                  synthesis.onvoiceschanged,
                  Array.isArray(firstVoices),
                  firstVoices.length,
                  firstVoices !== secondVoices
                ],
                brandChecks: [
                  throwsTypeError(() => SpeechSynthesis.prototype.cancel.call({})),
                  throwsTypeError(() => SpeechSynthesis.prototype.pause.call({})),
                  throwsTypeError(() => SpeechSynthesis.prototype.resume.call({})),
                  throwsTypeError(() => SpeechSynthesis.prototype.getVoices.call({})),
                  throwsTypeError(() => Object.getOwnPropertyDescriptor(
                    SpeechSynthesis.prototype,
                    "pending"
                  ).get.call({})),
                  throwsTypeError(() => synthesis.speak()),
                  throwsTypeError(() => synthesis.speak({}))
                ],
                listenerCalls
              });
            })()
            "#,
        )
        .expect("speech synthesis surface should evaluate");

    let value: serde_json::Value = serde_json::from_str(&result).expect("valid JSON probe");
    assert_eq!(
        value["windowDescriptor"],
        serde_json::json!([
            "function",
            "get speechSynthesis",
            0,
            "undefined",
            null,
            null,
            true,
            true,
            "undefined",
            null,
            null,
            null
        ])
    );
    assert_eq!(value["sameObject"], true);
    assert_eq!(
        value["objectShape"],
        serde_json::json!(["[object SpeechSynthesis]", true, true, true, true, 0])
    );
    assert_eq!(value["constructorShape"][1], true);
    assert_eq!(value["constructorShape"][2], true);
    assert_eq!(
        value["initialState"],
        serde_json::json!([false, false, false, null, true, 0, true])
    );
    assert_eq!(
        value["brandChecks"],
        serde_json::json!([true, true, true, true, true, true, true])
    );
    assert_eq!(value["listenerCalls"], 1);

    for entry in value["methodDescriptors"]
        .as_array()
        .expect("method descriptor list")
    {
        let name = entry[0].as_str().expect("method name");
        let descriptor = &entry[1];
        let expected_length = usize::from(name == "speak");
        assert_eq!(descriptor[6], true, "{name} enumerable");
        assert_eq!(descriptor[7], true, "{name} configurable");
        assert_eq!(descriptor[8], "function", "{name} value type");
        assert_eq!(descriptor[9], name, "{name} function name");
        assert_eq!(descriptor[10], expected_length, "{name} function length");
        assert_eq!(descriptor[11], true, "{name} writable");
    }

    for entry in value["attributeDescriptors"]
        .as_array()
        .expect("attribute descriptor list")
    {
        let name = entry[0].as_str().expect("attribute name");
        let descriptor = &entry[1];
        assert_eq!(descriptor[0], "function", "{name} getter type");
        assert_eq!(descriptor[1], format!("get {name}"), "{name} getter name");
        assert_eq!(descriptor[2], 0, "{name} getter length");
        assert_eq!(descriptor[6], true, "{name} enumerable");
        assert_eq!(descriptor[7], true, "{name} configurable");
        if name == "onvoiceschanged" {
            assert_eq!(descriptor[3], "function");
            assert_eq!(descriptor[4], "set onvoiceschanged");
            assert_eq!(descriptor[5], 1);
        } else {
            assert_eq!(descriptor[3], "undefined");
        }
    }
}

#[test]
fn speech_synthesis_utterance_matches_chromium_state_and_webidl_conversion() {
    let mut vm = new_storage_test_vm("https://speech-utterance.test/");

    let result = vm
        .eval(
            r#"
            (() => {
              const utterance = new SpeechSynthesisUtterance("hello");
              let eventCalls = 0;
              utterance.onstart = () => eventCalls++;
              utterance.dispatchEvent(new Event("start"));
              utterance.onstart = null;
              utterance.dispatchEvent(new Event("start"));

              utterance.text = null;
              utterance.lang = 42;
              utterance.volume = 2;
              utterance.rate = 0;
              utterance.pitch = -1;
              const converted = [
                utterance.text,
                utterance.lang,
                utterance.voice,
                utterance.volume,
                utterance.rate,
                utterance.pitch
              ];
              const finiteErrors = ["volume", "rate", "pitch"].map(name => {
                try {
                  utterance[name] = Infinity;
                  return false;
                } catch (error) {
                  return error instanceof TypeError;
                }
              });
              let voiceError = false;
              try {
                utterance.voice = {};
              } catch (error) {
                voiceError = error instanceof TypeError;
              }
              speechSynthesis.speak(utterance);

              return JSON.stringify({
                shape: [
                  SpeechSynthesisUtterance.length,
                  Object.prototype.toString.call(utterance),
                  Object.getPrototypeOf(utterance) === SpeechSynthesisUtterance.prototype,
                  Object.getPrototypeOf(SpeechSynthesisUtterance.prototype) === EventTarget.prototype,
                  utterance instanceof EventTarget,
                  Object.getOwnPropertyNames(utterance).length
                ],
                converted,
                finiteErrors,
                voiceError,
                eventCalls,
                synthesisState: [
                  speechSynthesis.pending,
                  speechSynthesis.speaking,
                  speechSynthesis.paused
                ],
                voiceSurface: [
                  typeof SpeechSynthesisVoice,
                  Object.getPrototypeOf(SpeechSynthesisVoice.prototype) === Object.prototype,
                  Object.getOwnPropertyNames(SpeechSynthesisVoice.prototype).sort()
                ]
              });
            })()
            "#,
        )
        .expect("speech synthesis utterance surface should evaluate");

    let value: serde_json::Value = serde_json::from_str(&result).expect("valid JSON probe");
    assert_eq!(
        value["shape"],
        serde_json::json!([0, "[object SpeechSynthesisUtterance]", true, true, true, 0])
    );
    assert_eq!(
        value["converted"],
        serde_json::json!(["null", "42", null, 1, 0.10000000149011612_f64, 0])
    );
    assert_eq!(value["finiteErrors"], serde_json::json!([true, true, true]));
    assert_eq!(value["voiceError"], true);
    assert_eq!(value["eventCalls"], 1);
    assert_eq!(
        value["synthesisState"],
        serde_json::json!([false, false, false])
    );
    assert_eq!(
        value["voiceSurface"],
        serde_json::json!([
            "function",
            true,
            [
                "constructor",
                "default",
                "lang",
                "localService",
                "name",
                "voiceURI"
            ]
        ])
    );
}
