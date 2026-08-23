# Architecture Decision Record (ADR) 009: Deployment & Runtime Architecture

**Status:** Proposed  
**Date:** February 17, 2026  
**Deciders:** DevOps, Architecture Team  

---

## Context

We need to support multiple deployment targets: cloud servers, edge runtimes (Cloudflare Workers, Vercel Edge), browsers, and self-hosted environments. This decision impacts runtime compatibility and performance.

## Decision Drivers

1. **Cloud-Native** - Traditional servers (AWS, GCP, Azure)
2. **Edge** - Edge runtimes for low latency
3. **Browser** - Client-side agent execution
4. **Self-Hosted** - On-premise deployments
5. **Serverless** - Functions as a Service
6. **Containerized** - Docker/Kubernetes

## Considered Options

### Option 1: Cloud-Native Only (Node.js/Python)

**Targets:**
- AWS EC2/Lambda
- Google Cloud Run
- Azure Container Instances

**Pros:**
- ✅ Simple deployment
- ✅ Full feature support
- ✅ Mature tooling

**Cons:**
- ❌ No edge deployment
- ❌ No browser execution
- ❌ Higher latency for global users

---

### Option 2: Edge-First (WebAssembly)

**Targets:**
- Cloudflare Workers
- Vercel Edge Functions
- Fastly Compute
- Deno Deploy

**Pros:**
- ✅ Global low latency
- ✅ Cost-effective
- ✅ Scales automatically

**Cons:**
- ❌ Limited runtime (no fs, limited memory)
- ❌ WebAssembly complexity
- ❌ Not all features available

---

### Option 3: Universal Runtime (Recommended)

**Architecture:**
- Core SDK works everywhere
- Feature detection for capabilities
- Graceful degradation

**Implementation:**
```typescript
// Feature detection
const runtime = detectRuntime()

// Cloud (full features)
if (runtime === 'node') {
  return new FullFeatureSet()
}

// Edge (limited features)
if (runtime === 'edge') {
  return new EdgeFeatureSet({
    persistence: 'kv-store',
    maxMemory: '128MB'
  })
}

// Browser (client-side)
if (runtime === 'browser') {
  return new BrowserFeatureSet({
    storage: 'indexeddb',
    model: 'onnx-web'
  })
}
```

---

## Recommendation

**Option 3: Universal Runtime with Capability Detection**

### Runtime Targets

#### 1. Node.js (Primary)

**Use Case:** Traditional servers, background jobs

```typescript
// Full feature support
import { createAgent } from '@ai-sdk/agents'
import { createMemory } from '@ai-sdk/memory'

const agent = createAgent({
  model: openai('gpt-4o'),
  memory: createMemory({
    type: 'persistent',
    store: 'postgresql'
  }),
  tools: [filesystemTool, databaseTool]
})
```

**Deployment:**
- Docker containers
- Kubernetes
- AWS ECS/EKS
- Google Cloud Run
- Traditional VPS

#### 2. Edge Runtimes

**Use Case:** Low-latency APIs, global distribution

**Supported Platforms:**
- Cloudflare Workers
- Vercel Edge Functions
- Deno Deploy
- Netlify Edge

```typescript
// Edge-compatible
export default {
  async fetch(request: Request) {
    const agent = createAgent({
      model: openai('gpt-4o'),
      // Use KV store for memory
      memory: createMemory({
        type: 'kv',
        store: cloudflareKV // or vercelKV
      }),
      // Limited tools (no fs)
      tools: [httpTool]
    })
    
    const result = await agent.run(await request.text())
    return new Response(result)
  }
}
```

**Limitations:**
- No filesystem access
- Limited memory (128MB typical)
- CPU time limits
- No native modules

#### 3. Browser

**Use Case:** Client-side agents, offline capabilities, privacy

```typescript
// Browser-native
import { createBrowserAgent } from '@ai-sdk/edge'

const agent = await createBrowserAgent({
  // Local ONNX model
  model: {
    type: 'onnx',
    url: '/models/phi-3-mini.onnx',
    executionProviders: ['webgpu', 'wasm']
  },
  
  // Browser storage
  memory: {
    type: 'indexeddb',
    encryption: true
  },
  
  // Browser-compatible tools
  tools: [domTool, fetchTool]
})

// Works offline
await agent.run('Analyze this page')
```

**Requirements:**
- WebAssembly support
- WebGPU for acceleration (optional)
- IndexedDB for storage
- Service Worker for offline

#### 4. WebAssembly (WASM)

**Use Case:** Portable, sandboxed execution

```typescript
// Compile to WASM
const wasmModule = await compileToWASM({
  target: 'wasm32-wasip1',
  features: ['core', 'agents'],
  wasiNN: { backend: 'onnx' }
})

// Run anywhere with WASM runtime
const agent = await instantiateWASM(wasmModule, {
  imports: { /* host functions */ }
})
```

**Targets:**
- Any WASM runtime (Wasmtime, WasmEdge)
- Serverless functions
- IoT devices
- Plugin systems

---

## Deployment Patterns

### Pattern 1: Cloud-Native (Traditional)

```yaml
# docker-compose.yml
version: '3.8'
services:
  api:
    build: .
    environment:
      - OPENAI_API_KEY=${OPENAI_API_KEY}
      - DATABASE_URL=${DATABASE_URL}
    ports:
      - "3000:3000"
  
  worker:
    build: .
    command: npm run worker
    environment:
      - REDIS_URL=${REDIS_URL}
```

**Pros:**
- Full control
- All features available
- Easy debugging

**Cons:**
- Infrastructure to manage
- Higher latency for global users

---

### Pattern 2: Edge + Origin (Hybrid)

```typescript
// Edge function (global)
export default {
  async fetch(request: Request, env: Env) {
    // Check cache first
    const cacheKey = await hash(request)
    const cached = await env.CACHE.get(cacheKey)
    if (cached) return new Response(cached)
    
    // Route to origin for complex tasks
    if (requiresComplexProcessing(request)) {
      return fetch('https://api.example.com/process', {
        method: 'POST',
        body: request.body
      })
    }
    
    // Handle simple tasks at edge
    const result = await simpleAgent.run(await request.text())
    await env.CACHE.put(cacheKey, result, { expirationTtl: 3600 })
    return new Response(result)
  }
}
```

**Pros:**
- Low latency for simple tasks
- Scales automatically
- Cost-effective

**Cons:**
- More complex architecture
- Edge limitations apply

---

### Pattern 3: Serverless Functions

```typescript
// AWS Lambda
export const handler = async (event: APIGatewayEvent) => {
  const agent = createAgent({
    model: openai('gpt-4o'),
    // Use DynamoDB for memory
    memory: createMemory({ type: 'dynamodb' })
  })
  
  const result = await agent.run(event.body)
  
  return {
    statusCode: 200,
    body: JSON.stringify(result)
  }
}
```

**Platforms:**
- AWS Lambda
- Google Cloud Functions
- Azure Functions
- Vercel Functions

**Pros:**
- Pay per use
- Auto-scaling
- No server management

**Cons:**
- Cold start latency
- Execution time limits
- Limited persistence

---

### Pattern 4: Browser-First

```html
<script type="module">
  import { createBrowserAgent } from 'https://cdn.ai-sdk.io/edge.js'
  
  const agent = await createBrowserAgent({
    model: 'phi-3-mini', // Runs locally
    tools: [domTool]
  })
  
  document.getElementById('analyze').onclick = async () => {
    const result = await agent.run('Analyze this page')
    console.log(result)
  }
</script>
```

**Pros:**
- Zero server costs
- Complete privacy
- Works offline
- No latency

**Cons:**
- Limited model capabilities
- Requires WebAssembly
- Browser compatibility issues

---

## Configuration by Runtime

```typescript
// ai.config.ts
export default defineConfig({
  // Common configuration
  providers: {
    openai: { apiKey: process.env.OPENAI_API_KEY }
  },
  
  // Runtime-specific overrides
  runtime: {
    node: {
      memory: { store: 'postgresql' },
      tools: [filesystemTool, databaseTool, bashTool]
    },
    
    edge: {
      memory: { store: 'kv' },
      tools: [httpTool],
      maxMemory: '128MB'
    },
    
    browser: {
      memory: { store: 'indexeddb' },
      tools: [domTool, fetchTool],
      model: { type: 'onnx' }
    }
  }
})
```

---

## Monitoring Across Runtimes

```typescript
// Universal observability
const telemetry = createTelemetry({
  // Works everywhere
  exporter: {
    cloud: 'otlp', // OpenTelemetry
    edge: 'cloudflare-analytics',
    browser: 'beacon-api'
  }
})
```

---

**Decision Status:** Proposed
