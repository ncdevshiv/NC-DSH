---
kind: upgrade-guide
description: "Model settings gain provider and model pause and ordering keys, and the configurable-provider directory reports provider product names instead of route identifiers."
---

# Model settings pause and order providers, and directory labels become product names

English | [中文](guide.zh.md)

## Change

`llm-pi-ai` and `llm-deepseek` settings sections accept new keys. A provider profile accepts `enabled`; `false` withdraws that route from the provider registry, so it leaves `listProviders`, the browser model catalog, and every request that names it, while its endpoint, credential reference, and models stay stored. A profile also accepts `modelOrder`, a list of model ids that decides the order that route serves, and each `models` entry and `modelOverrides` value accepts `enabled` to withdraw one model. `llm-pi-ai` additionally accepts a section-level `providerOrder`, which orders the routes a configuration surface lists; an entry naming a route with no profile is refused. Absence keeps prior behavior everywhere: an unset `enabled` is enabled, and an unset or empty order keeps configuration order, so a stored document that names none of these keys behaves exactly as before.

`llm/listConfigurableProviders()` now returns a provider's product name in `displayName` — `OpenAI`, `Moonshot AI` — where it previously returned the route identifier (`openai`, `moonshotai`). Consumers that compare a label to a route id are affected; `provider` still carries the identifier.

Withdrawing a model removes it from `listModels`, so a Session selecting that model id is now refused by `session/selectModel` the same way an unknown model is.

## Migration

1. Nothing is required. To pause a provider, set its profile `enabled: false`; to resume, remove the key or set it `true`.
2. To hide one model of a catalog route that declares no `models` list, add `modelOverrides: { "<model-id>": { enabled: false } }`. To reorder, list ids in `modelOrder`.
3. Compare against `provider`, not `displayName`, wherever code matched a configurable provider to its route id.
4. Confirm by calling `llm/listProviders` and `llm/listModels`: a paused provider and a withdrawn model are absent from both, and re-enabling returns them without further edits.
