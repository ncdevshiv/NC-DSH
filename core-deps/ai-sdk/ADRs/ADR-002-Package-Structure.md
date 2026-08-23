# Architecture Decision Record (ADR) 002: Package Structure

**Status:** Proposed  
**Date:** February 17, 2026  
**Deciders:** Technical Lead, Architecture Team  

---

## Context

We need to define the package/module structure for the AI SDK. This impacts:
- Tree-shaking and bundle size
- Developer experience (imports)
- Maintenance complexity
- Versioning strategy
- Dependency management

## Decision Drivers

1. **Modularity** - Users should only import what they need
2. **Tree-shaking** - Unused code should be eliminated
3. **Developer Experience** - Clear, intuitive imports
4. **Maintainability** - Reasonable package count
5. **Versioning** - Independent versioning vs monolithic
6. **Ecosystem** - Alignment with industry standards (Vercel AI SDK, etc.)

## Considered Options

### Option 1: Monolithic Package

**Structure:**
```
ai-sdk/
  ├── src/
  │   ├── providers/
  │   ├── agents/
  │   ├── tools/
  │   └── ...
  └── package.json
```

**Usage:**
```typescript
import { createAgent, OpenAIProvider } from 'ai-sdk'
```

**Pros:**
- ✅ Simple to understand
- ✅ Single version to track
- ✅ Easy installation
- ✅ No dependency conflicts

**Cons:**
- ❌ Large bundle size even if using subset
- ❌ Forces all dependencies on users
- ❌ Harder to tree-shake
- ❌ Slower install times

---

### Option 2: Fully Modular (Many Small Packages)

**Structure:**
```
@ai-sdk/
  ├── core
  ├── provider-openai
  ├── provider-anthropic
  ├── agent-base
  ├── agent-swarm
  ├── tool-http
  ├── tool-fs
  ├── memory-short
  ├── memory-long
  ├── rag-vector
  ├── rag-hybrid
  └── ... (50+ packages)
```

**Usage:**
```typescript
import { createAgent } from '@ai-sdk/agent-base'
import { OpenAIProvider } from '@ai-sdk/provider-openai'
```

**Pros:**
- ✅ Maximum tree-shaking
- ✅ Install only what you need
- ✅ Independent versioning
- ✅ Clear dependencies

**Cons:**
- ❌ Complex to manage
- ❌ Version conflicts between packages
- ❌ Harder to discover features
- ❌ More maintenance overhead
- ❌ Dependency hell

---

### Option 3: Core + Feature Packages (Recommended)

**Structure:**
```
@ai-sdk/
  ├── core          # Essential primitives
  ├── providers     # All LLM providers
  ├── agents        # Agent system + swarms
  ├── tools         # Built-in tools
  ├── memory        # All memory types
  ├── rag           # RAG capabilities
  ├── voice         # Voice & multi-modal
  ├── compliance    # GDPR, EU AI Act
  ├── edge          # WebAssembly & edge
  └── cli           # Command line tools
```

**Usage:**
```typescript
// Core usage
import { generateText } from '@ai-sdk/core'

// Full agent system
import { createAgent } from '@ai-sdk/agents'

// Specific provider
import { openai } from '@ai-sdk/providers'

// Add voice capabilities
import { createVoiceAgent } from '@ai-sdk/voice'
```

**Pros:**
- ✅ Good balance of modularity and simplicity
- ✅ Reasonable package count (8-10)
- ✅ Clear separation of concerns
- ✅ Can tree-shake within packages
- ✅ Manageable maintenance

**Cons:**
- ⚠️ Some packages may still be large
- ⚠️ Need to manage cross-package dependencies

---

## Recommendation

**Core + Feature Packages (Option 3)**

**Proposed Packages:**

```
@ai-sdk/core          # Essentials: LLM calls, streaming, tool schemas
@ai-sdk/providers     # All 25+ LLM providers (tree-shakeable)
@ai-sdk/agents        # Agents, subagents, swarms, orchestration
@ai-sdk/tools         # Built-in tools + MCP integration
@ai-sdk/memory        # All memory types + compaction
@ai-sdk/rag           # RAG: embeddings, vector search, GraphRAG
@ai-sdk/voice         # Voice, vision, multi-modal
@ai-sdk/compliance    # GDPR, EU AI Act, PII, audit
@ai-sdk/edge          # WebAssembly, browser, edge deployment
@ai-sdk/cli           # Command line tools
@ai-sdk/devtools      # Debugger, playground, testing
```

**Additional Considerations:**

1. **Separate @ai-sdk/prompts?**
   - Could include prompt management, versioning, A/B testing
   - Decision: Include in core for now, extract if it grows

2. **Provider Packages?**
   - Option A: All in @ai-sdk/providers (tree-shakeable)
   - Option B: Separate @ai-sdk/provider-openai, etc.
   - Decision: Option A for simplicity, ensure tree-shaking works

3. **Self-X Features?**
   - Self-healing, self-correction in @ai-sdk/agents
   - Self-skill creation in @ai-sdk/tools

## Consequences

### Positive
- Clear separation of concerns
- Users install only what they need
- Can version features independently
- Reasonable maintenance burden

### Negative
- Need to manage peer dependencies
- Cross-package testing complexity
- Documentation spread across packages

## Implementation Notes

- Use pnpm workspaces for monorepo management
- Each package has own version
- Shared build tooling
- Unified changelog

---

**Decision Status:** Proposed
