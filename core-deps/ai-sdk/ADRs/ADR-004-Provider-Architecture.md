# Architecture Decision Record (ADR) 004: Provider Architecture

**Status:** Proposed  
**Date:** February 17, 2026  
**Deciders:** Architecture Team  

---

## Context

We need to support 25+ LLM providers (OpenAI, Anthropic, Google, Mistral, etc.) with a unified API. This decision impacts how we abstract provider differences and handle provider-specific features.

## Decision Drivers

1. **Unified API** - Same interface across all providers
2. **Provider Features** - Access to provider-specific capabilities
3. **Extensibility** - Easy to add new providers
4. **Type Safety** - Compile-time safety for provider options
5. **Performance** - Minimal overhead for abstraction
6. **Streaming** - Consistent streaming across providers

## Considered Options

### Option 1: Adapter Pattern

**Architecture:**
```typescript
interface LLMProvider {
  generate(options: GenerateOptions): Promise<Generation>
  stream(options: GenerateOptions): AsyncIterable<Chunk>
}

class OpenAIAdapter implements LLMProvider {
  // OpenAI-specific implementation
}

class AnthropicAdapter implements LLMProvider {
  // Anthropic-specific implementation
}
```

**Pros:**
- ✅ Clean abstraction
- ✅ Easy to add new providers
- ✅ Can expose provider-specific options

**Cons:**
- ⚠️ May hide provider differences too much
- ⚠️ Feature parity challenges

---

### Option 2: Schema-Based Configuration

**Architecture:**
```typescript
const model = createModel({
  provider: 'openai',
  model: 'gpt-4o',
  // Provider-specific options passed through
  ...openaiOptions
})
```

**Pros:**
- ✅ Simple to understand
- ✅ Full access to provider options
- ✅ TypeScript can infer types

**Cons:**
- ❌ Less abstraction
- ❌ Harder to switch providers

---

### Option 3: Unified Interface with Provider Extensions (Recommended)

**Architecture:**
```typescript
// Unified interface
interface LanguageModel {
  generate(options: GenerateOptions): Promise<Generation>
  stream(options: StreamOptions): AsyncIterable<TextChunk>
  doStream(options: StreamOptions): AsyncIterable<StreamPart>
}

// Provider-specific implementations
const openai = createOpenAI({ apiKey: '...' })
const anthropic = createAnthropic({ apiKey: '...' })

// Usage - same interface
const result1 = await openai('gpt-4o').generate({ prompt: 'Hello' })
const result2 = await anthropic('claude-3').generate({ prompt: 'Hello' })

// Provider-specific features via extensions
const result3 = await anthropic('claude-3', {
  // Anthropic-specific
  cacheControl: { type: 'ephemeral' }
}).generate({ prompt: 'Hello' })
```

**Pros:**
- ✅ Unified API for common operations
- ✅ Can access provider-specific features
- ✅ Type-safe provider options
- ✅ Easy to switch providers

**Cons:**
- ⚠️ More complex type system
- ⚠️ Need to document provider differences

---

## Recommendation

**Option 3: Unified Interface with Provider Extensions**

**Pattern:**

```typescript
// Core types (provider-agnostic)
interface GenerateOptions {
  messages: Message[]
  temperature?: number
  maxTokens?: number
  tools?: Tool[]
  // ... common options
}

// Provider factory functions
function createOpenAI(config: OpenAIConfig): OpenAIProvider
function createAnthropic(config: AnthropicConfig): AnthropicProvider

// Provider-specific options through generics
type OpenAIGenerateOptions = GenerateOptions & {
  responseFormat?: { type: 'json_object' }
  seed?: number
}

type AnthropicGenerateOptions = GenerateOptions & {
  cacheControl?: CacheControl
  topK?: number
}
```

**Key Design Principles:**

1. **Common Interface**: All providers implement same base interface
2. **Factory Functions**: Each provider has its own factory function
3. **Provider Options**: Provider-specific options through intersection types
4. **Tree-shaking**: Only import providers you use
5. **Lazy Loading**: Provider clients initialized on first use

## Consequences

### Positive
- Can switch providers with minimal code changes
- Access to provider-specific optimizations
- Type-safe across all providers
- Easy to add new providers

### Negative
- Need to maintain provider-specific options
- Some features may not be available across all providers
- Documentation complexity

## Provider Support Strategy

### Tier 1 (Native, Full Support):
- OpenAI
- Anthropic
- Google (Gemini)
- Mistral
- Cohere
- Azure OpenAI

### Tier 2 (Community, Basic Support):
- Groq
- Together AI
- Fireworks AI
- AWS Bedrock
- GCP Vertex
- Ollama

### Adding New Providers:

```typescript
// Example: Adding a new provider
export function createCustomProvider(config: CustomConfig): CustomProvider {
  return {
    model(modelId: string) {
      return {
        async generate(options: GenerateOptions) {
          // Implementation
        },
        async *stream(options: StreamOptions) {
          // Implementation
        }
      }
    }
  }
}
```

## Streaming Strategy

All providers must support streaming through a common interface:

```typescript
interface StreamPart {
  type: 'text' | 'tool-call' | 'tool-result' | 'error' | 'finish'
  value?: string | ToolCall | ToolResult | Error | FinishReason
}

async function* stream(options: StreamOptions): AsyncIterable<StreamPart> {
  // Provider-specific streaming implementation
  // Must yield StreamPart objects
}
```

---

**Decision Status:** Proposed
