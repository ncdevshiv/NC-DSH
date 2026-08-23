#include "support.h"
#include "v8.h"

#include <cstdint>

using namespace support;

using v8__ModifyCodeGenerationFromStringsCallback =
    bool (*)(v8::Local<v8::Context> context,
             v8::Local<v8::Value> source,
             bool is_code_like,
             const v8::String** modified_source);

namespace {

// Reserved by rusty_v8 for this callback. This must match
// Isolate::MODIFY_CODE_GENERATION_CALLBACK_SLOT in isolate.rs.
constexpr uint32_t kModifyCodeGenerationCallbackSlot = 1;

struct ModifyCodeGenerationCallbackSlot {
  v8__ModifyCodeGenerationFromStringsCallback callback;
};

}  // namespace

extern "C" {

// V8 does not accept callback data for this hook. Keep the Rust callback keyed
// by the isolate itself so renderer and worker isolates can enforce different
// code-generation policies concurrently without process-global teardown state.
static v8::ModifyCodeGenerationFromStringsResult
v8__RustModifyCodeGenerationFromStringsCallback(
    v8::Local<v8::Context> context,
    v8::Local<v8::Value> source,
    bool is_code_like) {
  v8::ModifyCodeGenerationFromStringsResult result;
  auto* callback_slot = static_cast<ModifyCodeGenerationCallbackSlot*>(
      v8::Isolate::GetCurrent()->GetData(kModifyCodeGenerationCallbackSlot));
  if (!callback_slot) {
    return result;
  }
  const v8::String* modified_source = nullptr;
  result.codegen_allowed = callback_slot->callback(
      context, source, is_code_like, &modified_source);
  if (result.codegen_allowed && modified_source) {
    result.modified_source = ptr_to_local(modified_source);
  }
  return result;
}

void v8__Isolate__SetModifyCodeGenerationFromStringsCallback(
    v8::Isolate* isolate,
    v8__ModifyCodeGenerationFromStringsCallback callback) {
  auto* callback_slot = static_cast<ModifyCodeGenerationCallbackSlot*>(
      isolate->GetData(kModifyCodeGenerationCallbackSlot));
  if (!callback) {
    // Stop V8 from entering the trampoline before detaching and deleting the
    // isolate-owned callback data that the trampoline resolves.
    isolate->SetModifyCodeGenerationFromStringsCallback(nullptr);
    isolate->SetData(kModifyCodeGenerationCallbackSlot, nullptr);
    delete callback_slot;
    return;
  }
  if (callback_slot) {
    callback_slot->callback = callback;
  } else {
    callback_slot = new ModifyCodeGenerationCallbackSlot{callback};
    isolate->SetData(kModifyCodeGenerationCallbackSlot, callback_slot);
  }
  isolate->SetModifyCodeGenerationFromStringsCallback(
      v8__RustModifyCodeGenerationFromStringsCallback);
}

}  // extern "C"
