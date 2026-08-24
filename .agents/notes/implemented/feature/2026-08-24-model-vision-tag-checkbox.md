# Agent Note: Manual vision tagging in the Models settings page

Status: implemented

English | [中文](2026-08-24-model-vision-tag-checkbox.zh.md)

## Problem

Whether a model accepts image input was decided entirely outside the product: a pi-ai route the installed catalog ships inherits each entry's `input` declaration, while a hand-declared gateway route falls back to its `defaultInput`, which defaults to text-only. A deployment listing vision models on such a route — `deepseek-v4-flash-vision-exp`, `mimo-v2-omni`, qwen — had every one of them classified text-only, so attaching an image failed with "Model does not support images" even for models whose endpoints accept images. The only fix was hand-editing `settings.yaml` to add `input: ['text', 'image']`.

## Decision

Both model editors on the Models settings page carry a per-row Vision checkbox next to the display name. The checkbox reads and writes the row's explicit modality declaration — pi-ai rows write `input`, DeepSeek catalog rows write `inputModalities`; both states pin the field (`['text', 'image']` checked, `['text']` unchecked) because on a catalog route an absent field would inherit the installed entry's declaration, and "unchecked" must mean text-only, not "whatever the catalog says". Untouched rows keep inheriting.

Where capability data exists, it flows automatically instead of being retyped: catalog-backed discovery now returns each installed entry's modalities as `inputModalities` on `LlmDiscoveredModel` (carried through `LlmRuntime.discoverModels`'s rebuild and the `llm.discoverModels` wire view), and adopting a candidate seeds its row's declaration from what answered. A listing endpoint discloses no modalities, so network-discovered rows stay on the route default. No name-pattern heuristics were added; identification remains exactly two sources — the installed catalog and an explicit declaration.

## Alternatives considered

- **Guess modalities from model names** (`vision`, `omni` substrings): hardcoded fiction that misclassifies on first contact with a gateway that names models differently.
- **Probe the endpoint with an image at save time**: spends tokens, needs a live key at configuration time, and cannot distinguish a gateway-level refusal from a model-level one.
- **Show inherited effective state in the checkbox**: requires a per-provider effective-model lookup the settings page does not have; the tooltip states the inheritance rule instead.
- **Route-level-only defaultInput editor**: one switch for all models cannot express the common case where a gateway serves a mix of vision and text-only models.

## Consequences

A user who unchecks Vision on a row they never otherwise touched materializes the whole user-owned model array with that row pinned text-only — the same override semantics every other row edit already has. The checkbox shows unchecked for an untouched catalog-vision row until the row is adopted or edited; the tooltip names this rather than hiding it. Discovery adoption now trusts the answering source's declaration, so a catalog that over-claims propagates until the user pins the row down — the same trust the request-time gates already place in declarations.
