# Architecture Decision Record (ADR) 008: Security Architecture

**Status:** Proposed  
**Date:** February 17, 2026  
**Deciders:** Security Lead, Architecture Team  

---

## Context

We need comprehensive security for the AI SDK, covering API keys, PII protection, prompt injection prevention, sandboxing, and audit logging. Security is critical for enterprise adoption.

## Decision Drivers

1. **API Key Security** - Protect provider API keys
2. **PII Protection** - Detect and redact personal information
3. **Prompt Injection** - Prevent malicious prompts
4. **Sandboxing** - Isolate tool execution
5. **Audit** - Immutable security logs
6. **Compliance** - GDPR, EU AI Act requirements

## Considered Options

### API Key Management

#### Option A: Environment Variables Only
```bash
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-...
```

**Pros:**
- ✅ Simple
- ✅ Standard practice

**Cons:**
- ❌ Keys in process memory
- ❌ No rotation
- ❌ No audit trail

#### Option B: Secret Management Integration (Recommended)
```typescript
// Support multiple secret managers
const openai = createOpenAI({
  apiKey: await secrets.get('OPENAI_API_KEY')
})

// Or with built-in rotation
const openai = createOpenAI({
  apiKey: new RotatingKey({
    source: secretsManager,
    keyName: 'openai-key',
    rotateEvery: '24h'
  })
})
```

**Pros:**
- ✅ Integration with secret managers
- ✅ Key rotation
- ✅ Audit trail
- ✅ Supports AWS Secrets Manager, HashiCorp Vault, etc.

**Cons:**
- ⚠️ Additional dependency

---

### PII Protection

**Architecture:**
```typescript
const privacyFilter = createPrivacyFilter({
  // Detectors
  detectors: [
    'pii:email',
    'pii:ssn',
    'pii:credit-card',
    'pii:phone',
    'pii:address',
    'pii:name',
    // Custom patterns
    { name: 'employee-id', regex: /EMP-\d{6}/ }
  ],
  
  // Redaction strategies
  redaction: {
    strategy: 'mask', // or 'hash', 'tokenize', 'remove'
    maskCharacter: '*',
    preserveFormat: true
  }
})

// Apply to input/output
const safeInput = await privacyFilter.redact(userInput)
const safeOutput = await privacyFilter.redact(agentOutput)
```

**Implementation:**
- Use Presidio, AWS Comprehend, or custom NER models
- Run locally (no data leaves system)
- Configurable sensitivity levels

---

### Prompt Injection Prevention

**Multi-layer Defense:**

```typescript
const securityLayer = createSecurityLayer({
  // Layer 1: Input validation
  inputValidation: {
    maxLength: 10000,
    bannedPhrases: ['ignore previous', 'disregard instructions'],
    sanitizeHtml: true
  },
  
  // Layer 2: Heuristic detection
  heuristicDetection: {
    enabled: true,
    rules: [
      { pattern: /system.*prompt/i, action: 'block' },
      { pattern: /DAN|jailbreak/i, action: 'flag' }
    ]
  },
  
  // Layer 3: LLM-based classification
  llmDetection: {
    enabled: true,
    model: 'gpt-4o-mini', // Small, fast model
    threshold: 0.8
  },
  
  // Response
  onThreat: {
    action: 'block', // or 'log', 'sanitize'
    alert: true,
    logLevel: 'security'
  }
})
```

**Best Practices:**
- Never trust user input
- Separate system prompts from user input
- Use structured prompts (JSON, XML) when possible
- Validate tool outputs before using in prompts

---

### Tool Execution Sandboxing

**Option A: Process Isolation**
```typescript
// Execute tools in separate process
const result = await sandbox.execute({
  tool: 'bash',
  args: ['ls', '-la'],
  
  // Security constraints
  constraints: {
    timeout: 30000,
    maxMemory: '512MB',
    allowedPaths: ['/tmp', '/workspace'],
    blockedCommands: ['rm', 'chmod'],
    network: 'none' // or 'restricted'
  }
})
```

**Option B: Container Isolation (Recommended for Production)**
```typescript
// Execute in Docker container
const result = await containerSandbox.execute({
  image: 'ai-sdk-tools:latest',
  tool: 'bash',
  args: ['ls', '-la'],
  
  security: {
    readOnlyRoot: true,
    noNewPrivileges: true,
    dropAllCapabilities: true,
    seccompProfile: 'default.json'
  }
})
```

---

### Audit Logging

**Immutable Audit Trail:**
```typescript
const auditLogger = createAuditLogger({
  // What to log
  events: [
    'model.invocation',
    'tool.execution',
    'user.input',
    'agent.decision',
    'security.violation'
  ],
  
  // Storage
  storage: {
    type: 'append-only',
    backend: 's3', // or 'gcs', 'azure', 'postgres'
    encryption: 'aes-256-gcm',
    signing: 'ed25519' // Immutable
  },
  
  // Retention
  retention: '7-years',
  
  // Tamper detection
  integrity: {
    merkleTree: true,
    periodicHashes: true
  }
})
```

**Audit Event Format:**
```json
{
  "timestamp": "2026-02-17T10:00:00Z",
  "eventId": "uuid",
  "eventType": "model.invocation",
  "actor": { "type": "agent", "id": "agent-123" },
  "action": "generate",
  "resource": { "provider": "openai", "model": "gpt-4o" },
  "context": {
    "traceId": "trace-uuid",
    "userId": "user-456",
    "sessionId": "session-789"
  },
  "input": { "messages": [...], "tokens": 150 },
  "output": { "content": "...", "tokens": 50 },
  "metadata": {
    "cost": 0.002,
    "latency": 1200,
    "signature": "sha256-hash"
  }
}
```

---

## GDPR & EU AI Act Compliance

### Data Retention
```typescript
const gdprManager = createGDPRManager({
  retention: {
    conversations: '90d',
    userProfiles: '2y',
    auditLogs: '7y'
  },
  
  // Right to be forgotten
  deletion: {
    cascade: true,
    audit: true,
    verification: true
  }
})

// Delete user data
await gdprManager.deleteUserData(userId, {
  include: ['conversations', 'memory', 'audit-logs'],
  reason: 'user-request'
})
```

### Data Export
```typescript
// Export all user data (portability)
const export = await gdprManager.exportUserData(userId, {
  format: 'json',
  include: ['conversations', 'memory', 'preferences']
})
```

### EU AI Act High-Risk Systems
```typescript
const aiActCompliance = createAIActCompliance({
  riskLevel: 'high',
  
  humanOversight: {
    enabled: true,
    triggers: [
      { condition: 'confidence < 0.7', action: 'review' },
      { condition: 'sensitive_data', action: 'approval' }
    ]
  },
  
  transparency: {
    informUser: true,
    discloseAI: true,
    provideExplanation: true
  }
})
```

---

## Security Checklist

### Development:
- [ ] No hardcoded secrets
- [ ] Secrets in environment or secret manager
- [ ] Input validation on all user inputs
- [ ] Output encoding to prevent injection

### Runtime:
- [ ] PII detection enabled
- [ ] Prompt injection detection enabled
- [ ] Tool execution sandboxed
- [ ] Rate limiting applied
- [ ] Audit logging enabled

### Deployment:
- [ ] TLS/SSL for all connections
- [ ] API keys rotated regularly
- [ ] Network policies (firewall rules)
- [ ] Container security scanning
- [ ] Dependency vulnerability scanning

---

**Decision Status:** Proposed
