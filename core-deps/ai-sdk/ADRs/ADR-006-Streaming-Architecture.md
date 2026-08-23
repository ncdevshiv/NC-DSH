# Architecture Decision Record (ADR) 006: Streaming Architecture

**Status:** Proposed  
**Date:** February 17, 2026  
**Deciders:** Architecture Team  

---

## Context

Streaming is a core requirement for our SDK. We need to support streaming for LLM responses, tool calls, multi-modal data (voice, video), and structured data. This decision impacts real-time user experience and resource usage.

## Decision Drivers

1. **Real-time UX** - Users see responses as they're generated
2. **Tool Integration** - Stream tool calls as they're decided
3. **Multi-modal** - Stream audio/video chunks
4. **Efficiency** - Don't buffer entire response
5. **Backpressure** - Handle slow consumers
6. **Cancellation** - Allow users to cancel streams

## Considered Options

### Option 1: Callback-Based Streaming

```typescript
model.generate({
  prompt: 'Hello',
  onToken: (token) => console.log(token),
  onToolCall: (tool) => handleTool(tool),
  onComplete: () => console.log('Done')
})
```

**Pros:**
- ✅ Simple API
- ✅ Works everywhere

**Cons:**
- ❌ Hard to compose/transform streams
- ❌ Callback hell for complex flows
- ❌ No cancellation support
- ❌ Hard to test

---

### Option 2: Promise-Based with Events

```typescript
const stream = model.stream({ prompt: 'Hello' })

stream.on('data', (chunk) => {})
stream.on('end', () => {})
stream.on('error', (err) => {})

// Cancel
stream.cancel()
```

**Pros:**
- ✅ Better than callbacks
- ✅ Event-driven

**Cons:**
- ❌ Still callback-based
- ❌ Hard to compose
- ❌ Memory leaks possible

---

### Option 3: Async Iterables (Recommended)

```typescript
// Basic usage
for await (const chunk of model.stream({ prompt: 'Hello' })) {
  console.log(chunk)
}

// With cancellation
const abortController = new AbortController()
for await (const chunk of model.stream({ prompt: 'Hello' }, { signal: abortController.signal })) {
  if (shouldStop) abortController.abort()
}

// Transform streams
const transformed = stream
  .pipeThrough(new TransformStream({
    transform(chunk, controller) {
      controller.enqueue(process(chunk))
    }
  }))
```

**Pros:**
- ✅ Native JavaScript/TypeScript
- ✅ Composable with pipeThrough/pipeTo
- ✅ Cancellation via AbortController
- ✅ Backpressure built-in
- ✅ Works with Web Streams API
- ✅ Easy to test

**Cons:**
- ⚠️ Learning curve for developers unfamiliar with async iterables
- ⚠️ Node.js < 18 needs polyfill for some features

---

## Recommendation

**Option 3: Async Iterables**

**Rationale:**
1. **Standard**: Native JavaScript feature
2. **Composability**: Can transform, filter, merge streams easily
3. **Cancellation**: Native AbortController support
4. **Backpressure**: Automatic handling
5. **Web Streams**: Compatible with browser Streams API
6. **Type Safety**: Works great with TypeScript

**Implementation Pattern:**

```typescript
// Core streaming types
interface TextStreamPart {
  type: 'text'
  text: string
}

interface ToolCallStreamPart {
  type: 'tool-call'
  toolCall: {
    toolName: string
    args: Record<string, unknown>
  }
}

interface ToolResultStreamPart {
  type: 'tool-result'
  toolResult: {
    toolName: string
    result: unknown
  }
}

interface FinishStreamPart {
  type: 'finish'
  finishReason: 'stop' | 'length' | 'content-filter'
  usage: { promptTokens: number; completionTokens: number }
}

type StreamPart = 
  | TextStreamPart 
  | ToolCallStreamPart 
  | ToolResultStreamPart 
  | FinishStreamPart

// Streaming function
async function* stream(
  options: StreamOptions
): AsyncIterable<StreamPart> {
  // Implementation yields StreamParts
}
```

**Usage Examples:**

```typescript
// Basic streaming
for await (const part of stream({ prompt: 'Hello' })) {
  switch (part.type) {
    case 'text':
      process.stdout.write(part.text)
      break
    case 'tool-call':
      console.log('Tool called:', part.toolCall.toolName)
      break
    case 'finish':
      console.log('Tokens used:', part.usage)
      break
  }
}

// Transform: Only text
const textStream = stream({ prompt: 'Hello' })
  .pipeThrough(new TransformStream({
    transform(part, controller) {
      if (part.type === 'text') {
        controller.enqueue(part.text)
      }
    }
  }))

// Merge multiple streams
const merged = mergeStreams([stream1, stream2])

// Collect to array (for testing)
const parts = await Array.fromAsync(stream({ prompt: 'Hello' }))
```

## Multi-Modal Streaming

For voice, video, etc.:

```typescript
interface AudioStreamPart {
  type: 'audio'
  audio: Uint8Array
  timestamp: number
}

interface VideoStreamPart {
  type: 'video'
  frame: ImageData
  timestamp: number
}

// Multi-modal stream
async function* multimodalStream(options: MultimodalOptions) {
  // Yield text, audio, video parts
}
```

## Protocol Support

**Server-Sent Events (SSE):**
```typescript
// Server
app.get('/stream', async (req, res) => {
  res.setHeader('Content-Type', 'text/event-stream')
  
  for await (const part of stream()) {
    res.write(`data: ${JSON.stringify(part)}\n\n`)
  }
  res.end()
})
```

**WebSockets:**
```typescript
ws.on('message', async (data) => {
  const options = JSON.parse(data)
  
  for await (const part of stream(options)) {
    ws.send(JSON.stringify(part))
  }
})
```

**HTTP/2 Server Push:**
- For HTTP/2 enabled servers
- Push multiple streams over single connection

## Backpressure Handling

```typescript
// Slow consumer example
for await (const part of fastStream()) {
  // If this loop is slow, the producer pauses automatically
  await slowProcess(part)
}
```

## Cancellation

```typescript
const controller = new AbortController()

// Cancel after 5 seconds
setTimeout(() => controller.abort(), 5000)

try {
  for await (const part of stream({ signal: controller.signal })) {
    console.log(part)
  }
} catch (error) {
  if (error.name === 'AbortError') {
    console.log('Stream cancelled')
  }
}
```

## Testing

```typescript
// Mock stream for testing
async function* mockStream(): AsyncIterable<StreamPart> {
  yield { type: 'text', text: 'Hello' }
  yield { type: 'text', text: ' World' }
  yield { type: 'finish', finishReason: 'stop', usage: { ... } }
}

// Test
const parts = []
for await (const part of mockStream()) {
  parts.push(part)
}
expect(parts).toHaveLength(3)
```

---

**Decision Status:** Proposed
