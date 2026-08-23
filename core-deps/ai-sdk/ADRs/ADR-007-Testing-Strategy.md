# Architecture Decision Record (ADR) 007: Testing Strategy

**Status:** Proposed  
**Date:** February 17, 2026  
**Deciders:** Engineering Team, QA Lead  

---

## Context

We need a comprehensive testing strategy for the AI SDK that covers unit tests, integration tests, evaluation tests, and LLM-based testing. Testing AI agents is different from traditional software due to non-determinism.

## Decision Drivers

1. **Determinism** - Handle non-deterministic LLM outputs
2. **Cost** - Don't make expensive LLM calls in every test
3. **Speed** - Fast feedback for developers
4. **Coverage** - Test agents, tools, workflows
5. **Evaluation** - Measure agent quality
6. **Regression** - Catch quality degradation

## Considered Options

### Option 1: Traditional Unit Testing Only

**Approach:**
- Mock all LLM calls
- Test business logic only

**Pros:**
- ✅ Fast
- ✅ Deterministic
- ✅ Cheap

**Cons:**
- ❌ Don't test actual LLM behavior
- ❌ Miss integration issues
- ❌ Don't catch prompt regressions

---

### Option 2: Full Integration Testing with Real LLMs

**Approach:**
- Use real LLM calls for all tests
- Test end-to-end

**Pros:**
- ✅ Test actual behavior
- ✅ Catch real issues

**Cons:**
- ❌ Slow
- ❌ Expensive
- ❌ Flaky (non-deterministic)
- ❌ Rate limits

---

### Option 3: Tiered Testing Strategy (Recommended)

**Approach:**
- Unit tests (mocked, fast)
- Integration tests (selective real LLM)
- Snapshot tests (catch changes)
- Evaluation tests (measure quality)
- Regression tests (prevent degradation)

---

## Recommendation

**Option 3: Tiered Testing Strategy**

### Tier 1: Unit Tests (80% of tests)

**Purpose:** Test logic, not LLMs

```typescript
// Mock LLM responses
describe('Tool Execution', () => {
  it('should execute tool with correct arguments', async () => {
    const mockLLM = {
      generate: vi.fn().mockResolvedValue({
        toolCalls: [{
          name: 'calculator',
          arguments: { expression: '2+2' }
        }]
      })
    }
    
    const agent = createAgent({ model: mockLLM })
    const result = await agent.run('Calculate 2+2')
    
    expect(mockLLM.generate).toHaveBeenCalledWith(
      expect.objectContaining({
        tools: expect.arrayContaining([calculatorTool])
      })
    )
  })
})
```

**Characteristics:**
- Fast (< 100ms per test)
- Deterministic
- No LLM calls
- High coverage

### Tier 2: Snapshot Tests (10% of tests)

**Purpose:** Catch prompt/output changes

```typescript
describe('Prompt Snapshots', () => {
  it('should match system prompt snapshot', async () => {
    const agent = createAgent({
      systemPrompt: 'You are a helpful assistant'
    })
    
    // Capture the actual prompt sent to LLM
    const prompt = agent.getSystemPrompt()
    expect(prompt).toMatchSnapshot()
  })
})
```

**Characteristics:**
- Detect prompt regressions
- Review changes in PRs
- Fast (no LLM calls)

### Tier 3: Integration Tests (5% of tests)

**Purpose:** Test real LLM interactions

```typescript
describe('Real LLM Integration', () => {
  it('should correctly use tools', async () => {
    // Use real OpenAI (with small model for cost)
    const agent = createAgent({
      model: openai('gpt-4o-mini') // Cheapest model
    })
    
    const result = await agent.run(
      'What is 2+2? Use the calculator.',
      { tools: [calculator] }
    )
    
    expect(result).toContain('4')
  }, 10000) // 10 second timeout
})
```

**Characteristics:**
- Use cheapest/fastest models
- Run in CI on main branch
- Limited number (due to cost)

### Tier 4: Evaluation Tests (3% of tests)

**Purpose:** Measure agent quality

```typescript
describe('Agent Evaluation', () => {
  it('should achieve >90% accuracy on math', async () => {
    const dataset = loadMathDataset() // 100 examples
    
    const results = await evaluateAgent(agent, {
      dataset,
      metrics: ['accuracy', 'token_usage', 'latency']
    })
    
    expect(results.accuracy).toBeGreaterThan(0.9)
    expect(results.avgLatency).toBeLessThan(2000)
  })
})
```

**Characteristics:**
- Run before releases
- Track metrics over time
- Use evaluation datasets

### Tier 5: Regression Tests (2% of tests)

**Purpose:** Prevent quality degradation

```typescript
// Compare current vs previous version
describe('Regression', () => {
  it('should not degrade from baseline', async () => {
    const baselineResults = loadBaselineResults()
    const currentResults = await evaluateCurrentVersion()
    
    expect(currentResults.accuracy).toBeGreaterThanOrEqual(
      baselineResults.accuracy * 0.95 // Allow 5% variance
    )
  })
})
```

**Characteristics:**
- Run on PRs to main
- Block merge if degraded
- Track in CI

## Mock LLM for Testing

```typescript
// Built-in mock LLM
const mockLLM = createMockLLM({
  // Define responses
  responses: [
    {
      pattern: /calculate.*2\+2/,
      response: {
        toolCalls: [{ name: 'calculator', args: { expr: '2+2' } }]
      }
    },
    {
      pattern: /.*/, // Default
      response: { text: 'I understand' }
    }
  ],
  
  // Or use recordings
  recordings: loadRecordings('test-recordings.json')
})
```

## Test Recording/Replay

```typescript
// Record real LLM interactions
const recorder = createRecorder()

const agent = createAgent({
  model: openai('gpt-4o'),
  middleware: [recorder.middleware]
})

// Run once with real LLM
await agent.run('Calculate 2+2')

// Save recording
recorder.save('calculator-test.json')

// Later: Replay recording in tests
const mockLLM = createReplayLLM('calculator-test.json')
```

## Continuous Evaluation

```typescript
// Run evaluations on schedule
const evaluator = createEvaluator({
  datasets: {
    'math': loadMathDataset(),
    'coding': loadCodingDataset(),
    'qa': loadQADataset()
  },
  
  metrics: ['accuracy', 'relevance', 'safety'],
  
  // Alert if metrics drop
  alerts: {
    accuracy: { min: 0.9, channels: ['slack', 'email'] },
    safety: { min: 0.99, channels: ['pagerduty'] }
  }
})

// Run daily
cron.schedule('0 0 * * *', () => {
  evaluator.runFullSuite()
})
```

## Testing Tools

### Built-in Test Utilities:

```typescript
import { 
  createMockLLM, 
  createTestAgent,
  expectToolCall,
  expectOutput,
  loadDataset 
} from '@ai-sdk/testing'

// Helper for common patterns
const testAgent = createTestAgent({
  model: 'mock', // Uses mock by default
  tools: [calculator, search]
})

it('should handle calculation', async () => {
  const result = await testAgent.run('What is 2+2?')
  
  expectToolCall(result).toBe('calculator')
  expectOutput(result).toContain('4')
})
```

## CI/CD Strategy

**Pull Requests:**
- Run Tier 1 (Unit) - Fast, all PRs
- Run Tier 2 (Snapshots) - Fast, all PRs

**Main Branch:**
- Run all tiers
- Update snapshots if changed

**Release:**
- Run Tier 4 (Evaluation)
- Run Tier 5 (Regression)
- Block release if degraded

---

**Decision Status:** Proposed
