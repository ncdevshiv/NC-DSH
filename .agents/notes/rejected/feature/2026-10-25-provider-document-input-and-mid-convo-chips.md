# Agent Note: Provider document input and mid-conversation capability controls

Status: rejected — no provider behind this harness accepts document blocks, and the mid-conversation switches are withheld by design, so the controls would claim requests the transport cannot send.

English | [中文](2026-10-25-provider-document-input-and-mid-convo-chips.zh.md)

## Problem

The Settings → Models parity audit against a competitor listed four model-metadata controls this page does not have: document (`pdf`) input, video/audio input, a native-web-search flag, and a mid-conversation-system-messages flag. Each is a checkbox over a claim the harness must then honor in every request it builds for that model, so an unanswered claim is not a cosmetic gap: the provider rejects the request mid-turn, after the message is durable, and the Session repeats a request that cannot succeed.

Documents already have a working path. A `FileBlock` is durable and logged, and request assembly projects each occurrence to deterministic handle text naming its read-only saved path, which the model then opens with filesystem tools ([llm types](../../../../packages/llm/llm/src/types.ts)). The office-to-pdf seam converts bytes for document *preview*, not for model input.

## Proposal

Upload documents through the DeepSeek Files API, serialize them as Anthropic Messages `document` blocks referencing a `file_id`, declare `pdf` in the modality vocabulary, and expose the competitor's capability chips per model.

## Alternatives considered

**Document input through the existing Files path.** Rejected on provider fact: DeepSeek's [Anthropic compatibility page](https://api-docs.deepseek.com/guides/anthropic_api) supports `text`, `image`, `thinking`, `tool_use`, `tool_result`, `server_tool_use`, and `web_search`, and does not support `document`. The Files store is also image-only by construction — `ImageMediaType`, a png/jpeg/webp/gif extension map, and a per-image byte limit — so its `file_id` has nothing to reference. This was the user's chosen option and is the one this note overturns.

**Widen the modality vocabulary in our own layer.** Rejected: `PiAiModality` is `Model<Api>['input'][number]`, which `MODALITY_GATE` pins to `text | image` as a compile-time drift check, and the installed provider library carries no `document` or `application/pdf` serializer at all. Naming a modality the transport cannot emit is the facade the audit forbids.

**Expose mid-conversation system and tool-update switches per model.** Rejected: `COMPAT_GATES` marks `supportsMidConvoSystemMessages`, `supportsMidConvoToolChanges`, `supportsMidConvoToolAdditions`, `supportsAdditionalTools`, and `supportsToolSearch` as `withhold`, and `assertOfferedCompatFields` refuses them with the reason that pi-ai's installed catalog sets them for the vendors that need it. `conversationUpdates()` reads those flags to decide prompt admission and tool-history projection, so a user-set claim changes cache-prefix reuse for a model that may not honor it.

**Ship the capability switches the schema already offers.** Chosen instead where the UI needs capability controls: offered fields such as `supportsStrictTools`, `supportsStrictMode`, and `supportsTemperature` reach real request assembly and can be surfaced without new vocabulary.

## Consequences

Document and media input stay exactly as they are: text and images to the provider, every other file through the read-only path and tools. The parity list loses four rows, and the honest remaining row is the offered-compat set. Reintroducing `pdf` needs a provider whose Messages implementation accepts `document` plus an upstream modality that names it; verify both before building again.
