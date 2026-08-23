// Copyright 2026 the Moli authors. MIT license.

#include "support.h"
#include "v8-inspector.h"

using namespace support;

extern "C" {

void v8_inspector__V8InspectorSession__cancelPauseOnNextStatement(
    v8_inspector::V8InspectorSession* self) {
  self->cancelPauseOnNextStatement();
}

void v8_inspector__V8InspectorSession__breakProgram(
    v8_inspector::V8InspectorSession* self,
    v8_inspector::StringView reason,
    v8_inspector::StringView detail) {
  self->breakProgram(reason, detail);
}

bool v8_inspector__V8InspectorSession__unwrapObject(
    v8_inspector::V8InspectorSession* self,
    v8_inspector::StringBuffer** error, v8_inspector::StringView object_id,
    const v8::Value** value, const v8::Context** context,
    v8_inspector::StringBuffer** object_group) {
  std::unique_ptr<v8_inspector::StringBuffer> error_unique;
  v8::Local<v8::Value> value_local;
  v8::Local<v8::Context> context_local;
  std::unique_ptr<v8_inspector::StringBuffer> object_group_unique;
  bool success = self->unwrapObject(&error_unique, object_id, &value_local,
                                    &context_local, &object_group_unique);
  if (error) {
    *error = success ? nullptr : error_unique.release();
  }
  if (value) {
    *value = success ? local_to_ptr(value_local) : nullptr;
  }
  if (context) {
    *context = success ? local_to_ptr(context_local) : nullptr;
  }
  if (object_group) {
    *object_group = success ? object_group_unique.release() : nullptr;
  }
  return success;
}

}  // extern "C"
