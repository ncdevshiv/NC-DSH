# Gateway Provider

The project gateway (`https://opencode.ai/zen/go/v1`) speaks the OpenAI
compatible wire protocol with DeepSeek-style extras (`reasoning_content`,
cache-aware usage). Configure via `.env`:

```
AI_SDK_GATEWAY_BASE_URL=https://opencode.ai/zen/go/v1
AI_SDK_GATEWAY_API_KEY=sk-...
```

Models used by the test suites (only these two):
- **Primary**: `deepseek-v4-flash` (text, reasoning, tools, streaming)
- **Vision**: `mimo-v2.5` (image input)

Verified contract facts (2026-08-09/10):
- `GET /models` returns `{object:list,data:[...]}`.
- Streaming ends with `finish_reason` + `usage`, then `[DONE]`, then a
  trailing `{"choices":[],"cost":"0"}` event (tolerated).
- Unknown models answer HTTP 401 ("Model ... is not supported").
