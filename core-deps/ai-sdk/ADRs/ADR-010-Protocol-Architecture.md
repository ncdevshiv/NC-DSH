# Architecture Decision Record (ADR) 010: Protocol Architecture (MCP & A2A)

**Status:** Proposed  
**Date:** February 17, 2026  
**Deciders:** Architecture Team  

---

## Context

We need to support emerging AI protocols: MCP (Model Context Protocol) for tool interoperability and A2A (Agent-to-Agent Protocol) for agent communication. These are becoming industry standards.

## Decision Drivers

1. **Interoperability** - Work with tools/agents from different vendors
2. **Ecosystem** - Leverage existing tools and agents
3. **Standards** - Use open standards vs proprietary protocols
4. **Flexibility** - Easy to add new protocols
5. **Future-proofing** - Support emerging standards

## Understanding the Protocols

### MCP (Model Context Protocol)

**Purpose:** Standardize how agents connect to tools and resources

**Analogy:** USB-C for AI tools

**Components:**
- **Tools:** Functions agents can call
- **Resources:** Data agents can access
- **Prompts:** Templates for common tasks
- **Sampling:** Let tools request LLM completions

**Example:**
```typescript
// MCP Server (exposes tools)
const mcpServer = createMCPServer({
  name: 'database-server',
  tools: [
    {
      name: 'query',
      description: 'Query the database',
      parameters: z.object({ sql: z.string() })
    }
  ]
})

// MCP Client (uses tools)
const mcpClient = createMCPClient({
  servers: ['database-server']
})

const tools = await mcpClient.listTools()
const result = await mcpClient.callTool('query', { sql: 'SELECT * FROM users' })
```

---

### A2A (Agent-to-Agent Protocol)

**Purpose:** Standardize how agents communicate with each other

**Analogy:** Email/Slack for AI agents

**Components:**
- **Agent Cards:** Discover agent capabilities
- **Tasks:** Work units agents perform
- **Skills:** Specific capabilities agents offer
- **Artifacts:** Results agents produce

**Example:**
```typescript
// A2A Server (exposes agent)
const a2aServer = createA2AServer({
  agentCard: {
    name: 'travel-booking-agent',
    skills: [
      {
        id: 'book-flight',
        name: 'Book Flight',
        inputSchema: z.object({ from: z.string(), to: z.string() })
      }
    ]
  },
  handlers: {
    'book-flight': async (input) => {
      return { bookingId: '123', status: 'confirmed' }
    }
  }
})

// A2A Client (delegates to agents)
const a2aClient = createA2AClient()
const agent = await a2aClient.discoverAgent('travel-booking-agent')
const result = await a2aClient.sendTask(agent.id, 'book-flight', input)
```

---

## Architecture Decision

### Unified Protocol Layer

**Design:** Single abstraction that supports multiple protocols

```typescript
// Protocol-agnostic interface
interface ProtocolAdapter {
  name: string
  discover(): Promise<Capability[]>
  invoke(capability: string, input: unknown): Promise<unknown>
}

// MCP Adapter
class MCPAdapter implements ProtocolAdapter {
  async discover() {
    return this.mcpClient.listTools()
  }
  
  async invoke(tool: string, input) {
    return this.mcpClient.callTool(tool, input)
  }
}

// A2A Adapter  
class A2AAdapter implements ProtocolAdapter {
  async discover() {
    const card = await this.a2aClient.discoverAgent(this.agentId)
    return card.skills
  }
  
  async invoke(skill: string, input) {
    return this.a2aClient.sendTask(this.agentId, skill, input)
  }
}

// Usage (protocol-agnostic)
const capabilities = await protocol.discover()
const result = await protocol.invoke('search', { query: 'hello' })
```

---

## Implementation Strategy

### 1. Native MCP Support

**MCP Client:**
```typescript
// Connect to any MCP server
const mcp = createMCPClient({
  servers: [
    { name: 'fs', command: 'npx -y @modelcontextprotocol/server-filesystem' },
    { name: 'postgres', url: 'http://localhost:3001/sse' },
    { name: 'slack', url: 'ws://localhost:3002' }
  ]
})

// Auto-discover and use tools
const tools = await mcp.listTools()
const agent = createAgent({ tools: [...tools, customTools] })
```

**MCP Server:**
```typescript
// Expose SDK capabilities as MCP server
const server = createMCPServer({
  name: 'my-ai-service',
  tools: sdkTools,
  resources: [
    { uri: 'docs://api', name: 'API Docs', mimeType: 'text/markdown' }
  ]
})

await server.start({ transport: 'http', port: 3000 })
```

---

### 2. Native A2A Support

**A2A Client:**
```typescript
const a2a = createA2AClient({
  registry: 'https://agents.company.com'
})

// Discover agents
const agents = await a2a.discover({ capability: 'code-review' })

// Delegate task
const result = await a2a.delegate({
  to: agents[0].id,
  skill: 'review-pr',
  input: { prUrl: 'https://github.com/...' }
})
```

**A2A Server:**
```typescript
const server = createA2AServer({
  agentCard: {
    name: 'code-review-agent',
    skills: [
      { id: 'review-pr', ... },
      { id: 'suggest-refactor', ... }
    ]
  }
})

// Agents can use A2A to call other agents
orchestratorAgent = createAgent({
  tools: [
    // Subagents exposed as A2A
    reviewAgent.asA2ATool(),
    testAgent.asA2ATool()
  ]
})
```

---

### 3. Protocol Composition

**MCP + A2A Together:**
```typescript
// Agents can use both protocols
const agent = createAgent({
  // MCP for tools
  mcp: {
    servers: ['filesystem', 'database']
  },
  
  // A2A for subagents
  a2a: {
    agents: ['research-agent', 'writing-agent']
  }
})

// Agent uses both seamlessly
// - Calls tools via MCP
// - Delegates to subagents via A2A
```

---

## Transport Options

### MCP Transports

```typescript
// stdio (local processes)
{ transport: 'stdio', command: 'npx @modelcontextprotocol/server-fs' }

// HTTP/SSE (remote servers)
{ transport: 'http', url: 'https://mcp.example.com/sse' }

// WebSocket (real-time)
{ transport: 'websocket', url: 'wss://mcp.example.com/ws' }
```

### A2A Transports

```typescript
// HTTP/2 (default)
{ transport: 'http2', url: 'https://agent.example.com' }

// gRPC (high performance)
{ transport: 'grpc', address: 'agent.example.com:50051' }

// WebSocket (streaming)
{ transport: 'websocket', url: 'wss://agent.example.com' }
```

---

## Security

### Authentication

```typescript
// MCP with auth
const mcp = createMCPClient({
  servers: [{
    name: 'secure-server',
    url: 'https://mcp.example.com',
    auth: {
      type: 'oauth2',
      clientId: process.env.CLIENT_ID,
      clientSecret: process.env.CLIENT_SECRET
    }
  }]
})

// A2A with auth
const a2a = createA2AClient({
  auth: {
    type: 'apiKey',
    key: process.env.A2A_API_KEY
  }
})
```

### Authorization

```typescript
// Define what agents can do
const a2aServer = createA2AServer({
  agentCard: { ... },
  
  authorization: {
    // Who can invoke skills
    'book-flight': ['role:travel-agent', 'user:admin'],
    'cancel-flight': ['role:supervisor']
  }
})
```

---

## Discovery & Registry

### Unified Discovery

```typescript
// Find capabilities across protocols
const discovery = createUnifiedDiscovery({
  sources: [
    { type: 'mcp', registry: 'https://mcp-registry.io' },
    { type: 'a2a', registry: 'https://a2a-registry.io' }
  ]
})

// Search for capabilities
const searchResults = await discovery.search({
  query: 'database query tools',
  protocols: ['mcp', 'a2a']
})

// Returns both MCP tools and A2A agent skills
```

---

## Migration Strategy

### From Proprietary to Standard

```typescript
// Existing proprietary tool
const oldTool = createCustomTool({ ... })

// Wrap as MCP
const mcpTool = wrapAsMCP(oldTool)

// Now interoperable with any MCP client
```

---

## Future Protocols

**Extensibility:**

```typescript
// Add new protocol support
class CustomProtocolAdapter implements ProtocolAdapter {
  // Implement interface
}

// Register
registerProtocol('custom', CustomProtocolAdapter)

// Use
const client = createProtocolClient({ type: 'custom', ... })
```

**Potential Future Protocols:**
- WebMCP (browser-native)
- ACP (Agent Communication Protocol)
- CDI (Cross-Domain Interoperability - IETF)
- Custom enterprise protocols

---

## Consequences

### Positive
- Interoperability with ecosystem
- Future-proof (open standards)
- Can leverage existing tools/agents
- Single SDK supports multiple protocols

### Negative
- Protocol complexity
- Need to track standard evolution
- More code to maintain
- Potential protocol conflicts

---

**Decision Status:** Proposed
