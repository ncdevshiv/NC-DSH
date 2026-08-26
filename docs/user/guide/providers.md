# Configure models

English | [中文](providers.zh.md)

This guide assumes you started the Web UI through the [root README](../../../README.md#run). Model changes take effect on the next request without restarting the server.

Every provider route is served by one adapter driving the `ai-sidecar` companion process; route ids and their wire dialects are described under [Provider routes](#provider-routes).

## Set up the sidecar binary

Model requests need the `ai-sidecar` executable. Set an absolute path in `binaryPath` (cordis.yml or the `llm-ai-sdk` settings section) or export `DSH_AI_SDK_SIDECAR` in the launching environment. With the path unset everywhere, the first request fails with `CONFIG` naming every configuration entry point; the Models page stays reachable so the path can be configured after boot.

## Configure DeepSeek

Open **Settings → Models**. The DeepSeek card exposes one API-key field; enter the key and save it.

![The Models page: provider cards with a key field each](providers-models-page.png)

Keys are write-only. The page receives a redacted descriptor after saving, never the literal secret. The key is stored in `$DSH_HOME/.credentials.yaml`, while settings retain only its credential reference.

The default `deepseek-official` route exists without any configuration: it serves DeepSeek's public endpoint through the `DEEPSEEK_API_KEY` credential reference, so entering the key is the only setup a default install needs.

## Provider routes

A route is a named entry in the `llm-ai-sdk` provider map. Each declared route appears as a card on the Models page; its display name, base URL, key, and model catalog stay editable there.

The route id selects how requests are spoken:

| Route id | Wire dialect | `baseURL` |
|---|---|---|
| `anthropic` | native Anthropic | optional |
| `google` | native Gemini | optional |
| `openai` | OpenAI-compatible | optional (`https://api.openai.com/v1`) |
| `openrouter` | OpenAI-compatible | optional (`https://openrouter.ai/api/v1`) |
| `ollama` | OpenAI-compatible | optional (`http://localhost:11434/v1`) |
| any other id | OpenAI-compatible | required |

Declare additional routes in the `llm-ai-sdk` section of `$DSH_HOME/settings.yaml`:

```yaml
llm-ai-sdk:
  providers:
    anthropic:
      apiKeyEnv: ANTHROPIC_API_KEY
      models:
        - id: claude-sonnet
          contextWindow: 200000
    my-gateway:
      displayName: Acme gateway
      apiKeyEnv: GATEWAY_API_KEY
      baseURL: https://gateway.example/v1
      models:
        - id: legacy-chat
        - id: vision-preview
          inputModalities: [text, image]
```

An omitted `apiKeyEnv` resolves the conventional reference `<ROUTE>_API_KEY`. A custom OpenAI-compatible gateway needs its full endpoint base including the version path (typically `/v1`). The route id of a saved session's provider is permanent: requests, session logs, model defaults, and credential references use it. To rename a route, declare the new id, move your work onto it, and delete the old row.

### Model catalog

Each profile's `models` list is advisory: unlisted model ids still pass through unchanged, and the picker shows exactly the listed entries with `name` as the label (defaulting to the id). Omitted `inputModalities` means text only; `[text, image]` is what admits image input on that model.

### Image input

Attaching an image to a text-only model is refused before it is sent, naming the model. A request whose accumulated base64 image payload exceeds the route's `maxRequestImageBytes` (default 20 MiB) is refused as well; lower the bound or send fewer images, since nothing is substituted or dropped to make the request fit.

### Reasoning controls

Each route publishes its selectable reasoning efforts (`off`, `low`, `high`, `max` by default) and its default effort (`high`). The composer's model picker offers the route's levels; a request without an explicit choice uses the route default.

## Select a model

Configured providers appear in the model picker. Selecting a model also makes it the default for new sessions. A session that has already sent a request retains the model recorded in its own log.

If a saved default names a provider that was deleted, the composer displays **Select model** and blocks input until another model is selected.

## Troubleshooting

- **`CONFIG`** — No sidecar binary is configured. See [Set up the sidecar binary](#set-up-the-sidecar-binary).
- **`MISSING_CREDENTIAL`** — Store the provider key through the Models page or supply the referenced environment variable.
- **`NO_ADAPTER`** — The session names a route that no longer exists. Select a configured model or re-declare the route under the same id.
- **Requests reach the gateway but every one is refused** — Custom routes speak OpenAI chat completions; check that the base URL includes the version path and the endpoint serves `/chat/completions`. Native Anthropic or Gemini endpoints require the route ids `anthropic` or `google`.
- **An image is refused before sending** — The model declares no image modality. Give the model `inputModalities: [text, image]` in its profile.
- **`TIMEOUT`** — The stream idled past `streamIdleTimeoutMs` (five minutes by default) waiting for the provider; check the endpoint's health, or raise the budget for slow endpoints.
- **The provider rejects a request carrying an image** — The model declares images its endpoint does not actually serve. Remove `image` from that model's `inputModalities`, then start a new session: the attached image stays in the session log, so the same request repeats until the session moves off it.

## Advanced configuration

The generated [plugin configuration catalog](../../config-catalog.md#deepseek-aidsh-llm-ai-sdk) lists every supported field and default for this plugin, derived from the source so it cannot fall behind what the adapter accepts. The [`dsh-llm-ai-sdk`](../../../packages/llm/llm-ai-sdk/README.md) reference owns direct `settings.yaml` configuration, route resolution, reasoning controls, credentials, sidecar lifecycle, and adapter errors.
