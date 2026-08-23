# Architecture Decision Records (ADRs) Index

**Project:** AI SDK  
**Version:** 1.0  
**Last Updated:** February 17, 2026  

---

## Overview

This directory contains Architecture Decision Records (ADRs) for the AI SDK. ADRs capture important architectural decisions, their context, and consequences to provide transparency and historical record.

## What is an ADR?

An Architecture Decision Record (ADR) is a document that captures:
- **Context:** Why the decision was needed
- **Options:** What alternatives were considered
- **Decision:** What was chosen
- **Consequences:** Impact of the decision

## ADR Statuses

- **Proposed:** Under discussion, not yet approved
- **Accepted:** Approved and being implemented
- **Deprecated:** Decision changed, see new ADR
- **Superseded:** Replaced by a newer ADR

---

## All ADRs

### Core Architecture

| # | Title | Status | Date | Key Decision |
|---|-------|--------|------|--------------|
| [ADR-001](ADR-001-Language-Selection.md) | Language Selection | Proposed | 2026-02-17 | TypeScript/Node.js (with multi-language potential) |
| [ADR-002](ADR-002-Package-Structure.md) | Package Structure | Proposed | 2026-02-17 | Core + Feature Packages (@ai-sdk/core, @ai-sdk/agents, etc.) |
| [ADR-003](ADR-003-Memory-Storage-Architecture.md) | Memory & Storage Architecture | Proposed | 2026-02-17 | Unified interface with pluggable backends |

### Provider & Integration

| # | Title | Status | Date | Key Decision |
|---|-------|--------|------|--------------|
| [ADR-004](ADR-004-Provider-Architecture.md) | Provider Architecture | Proposed | 2026-02-17 | Unified interface with provider extensions |
| [ADR-010](ADR-010-Protocol-Architecture.md) | Protocol Architecture (MCP & A2A) | Proposed | 2026-02-17 | Native MCP + A2A with unified protocol layer |

### Agent System

| # | Title | Status | Date | Key Decision |
|---|-------|--------|------|--------------|
| [ADR-005](ADR-005-Agent-Orchestration-Architecture.md) | Agent Orchestration Architecture | Proposed | 2026-02-17 | Hybrid: Centralized patterns + Decentralized swarms |

### Technical Implementation

| # | Title | Status | Date | Key Decision |
|---|-------|--------|------|--------------|
| [ADR-006](ADR-006-Streaming-Architecture.md) | Streaming Architecture | Proposed | 2026-02-17 | Async Iterables with Web Streams API |
| [ADR-007](ADR-007-Testing-Strategy.md) | Testing Strategy | Proposed | 2026-02-17 | Tiered: Unit → Snapshot → Integration → Evaluation → Regression |

### Operations & Security

| # | Title | Status | Date | Key Decision |
|---|-------|--------|------|--------------|
| [ADR-008](ADR-008-Security-Architecture.md) | Security Architecture | Proposed | 2026-02-17 | Multi-layer: Keys, PII, Prompt Injection, Sandbox, Audit |
| [ADR-009](ADR-009-Deployment-Architecture.md) | Deployment & Runtime Architecture | Proposed | 2026-02-17 | Universal runtime: Node.js + Edge + Browser + WASM |

---

## Quick Reference by Topic

### 🎯 Getting Started
- **Language:** ADR-001
- **Package Structure:** ADR-002

### 🔌 Providers & Protocols
- **LLM Providers:** ADR-004
- **MCP & A2A Protocols:** ADR-010

### 🤖 Agents
- **Orchestration:** ADR-005

### 💾 Data
- **Memory & Storage:** ADR-003

### 📡 Real-time
- **Streaming:** ADR-006

### 🧪 Quality
- **Testing:** ADR-007

### 🔒 Security
- **Security & Compliance:** ADR-008

### 🚀 Deployment
- **Deployment Targets:** ADR-009

---

## How to Create a New ADR

### Template:

```markdown
# Architecture Decision Record (ADR) XXX: Title

**Status:** Proposed  
**Date:** YYYY-MM-DD  
**Deciders:** [Names]  

## Context

[What is the issue we're deciding?]

## Decision Drivers

1. [Driver 1]
2. [Driver 2]

## Considered Options

### Option 1: [Name]

**Pros:**
- ✅ [Advantage]

**Cons:**
- ❌ [Disadvantage]

## Recommendation

**[Chosen Option]**

**Rationale:**
1. [Reason 1]

## Consequences

### Positive
- [Benefit]

### Negative
- [Cost]

---

**Decision Status:** Proposed
```

### Process:

1. **Draft:** Create ADR with status "Proposed"
2. **Review:** Share with team for feedback
3. **Decide:** Update status to "Accepted" or revise
4. **Implement:** Follow the decision
5. **Update:** Mark as "Deprecated" or "Superseded" if decision changes

---

## Relationships Between ADRs

```
ADR-001 (Language)
  ↓
ADR-002 (Package Structure)
  ↓
  ├── ADR-004 (Provider Architecture)
  ├── ADR-005 (Agent Orchestration)
  ├── ADR-006 (Streaming)
  └── ADR-007 (Testing)

ADR-003 (Memory)
  ↓
  └── ADR-009 (Deployment)

ADR-010 (Protocols)
  ↓
  └── ADR-004 (Provider Architecture)

ADR-008 (Security)
  ↓
  ├── ADR-003 (Memory)
  ├── ADR-004 (Provider Architecture)
  └── ADR-009 (Deployment)
```

---

## Key Decisions Summary

### Language & Platform
- **Primary:** TypeScript/Node.js
- **Alternative:** Multi-language (Rust core + TS/Python/Go bindings) if resources allow

### Architecture Pattern
- **Modular:** Core + feature packages
- **Pluggable:** Swappable backends for memory, providers, protocols
- **Universal:** Works in Node.js, Edge, Browser, WASM

### Agent System
- **Hybrid:** Centralized patterns for workflows + Decentralized swarms for scale
- **Self-improving:** Self-healing, self-correction, self-skill creation

### Data & Storage
- **Polyglot:** Redis (cache/short-term) + PostgreSQL (long-term) + Pinecone (vector)
- **Unified Interface:** Abstract storage layer

### Protocols
- **Native:** First-class MCP (tools) + A2A (agents) support
- **Extensible:** Easy to add future protocols

### Security
- **Defense in depth:** Keys, PII detection, prompt injection prevention, sandboxing, audit
- **Compliance:** GDPR, EU AI Act built-in

### Testing
- **Tiered:** 5 levels from fast unit tests to expensive evaluation tests
- **Deterministic:** Mock LLMs for most tests, real LLMs for integration

---

## Next Steps

1. **Review ADRs:** Team review of all proposed ADRs
2. **Decision Meeting:** Approve or revise ADRs
3. **Update Status:** Mark approved ADRs as "Accepted"
4. **Begin Implementation:** Start Phase 1 development based on ADRs

---

## Questions?

- Review individual ADR files for detailed information
- Discuss in team architecture meetings
- Update ADRs as decisions evolve

---

**Total ADRs:** 10  
**Proposed:** 10  
**Accepted:** 0  
**Last Updated:** February 17, 2026
