# Architecture Decision Record (ADR) 001: Language Selection

**Status:** Proposed  
**Date:** February 17, 2026  
**Deciders:** Technical Lead, Engineering Team  

---

## Context

We need to select the primary implementation language for our AI SDK. This is a foundational decision that impacts developer adoption, performance, and ecosystem compatibility.

## Decision Drivers

1. **Multi-provider LLM Support** - Must work seamlessly with 25+ providers
2. **WebAssembly Compilation** - Required for browser and edge deployment  
3. **Developer Experience** - Type safety, IDE support, debugging
4. **AI/ML Ecosystem** - Integration with existing tools
5. **Performance** - Throughput, latency, memory efficiency
6. **Package Distribution** - npm, PyPI, crates.io reach
7. **Cross-platform** - Browser, Node.js, Deno, Python, edge runtimes

## Considered Options

### Option 1: TypeScript/JavaScript (Node.js)

**Pros:**
- ✅ Excellent for full-stack development
- ✅ Native browser support (no WASM needed)
- ✅ Massive ecosystem (npm, 2M+ packages)
- ✅ Best-in-class IDE support (VSCode, IntelliSense)
- ✅ Easy WebAssembly integration
- ✅ Vercel AI SDK compatibility
- ✅ Easy hiring
- ✅ Great for streaming and async
- ✅ Deno/Bun compatibility

**Cons:**
- ❌ Slower than Rust/Go for CPU-intensive ops
- ❌ Single-threaded (requires Worker Threads)
- ❌ Memory overhead vs Rust
- ❌ Not primary ML/data science language

---

### Option 2: Python

**Pros:**
- ✅ Dominant AI/ML ecosystem (PyTorch, TensorFlow, HuggingFace)
- ✅ LangChain compatibility
- ✅ Data science integration
- ✅ Great for ML model serving
- ✅ Jupyter notebook support

**Cons:**
- ❌ GIL limits parallelism
- ❌ Slower runtime
- ❌ Browser deployment requires WASM
- ❌ Complex type system
- ❌ Higher memory usage

---

### Option 3: Rust

**Pros:**
- ✅ Best performance
- ✅ Excellent WebAssembly support
- ✅ Memory safety without GC
- ✅ True parallelism (no GIL)
- ✅ Small binary size
- ✅ Great for systems programming

**Cons:**
- ❌ Steeper learning curve
- ❌ Slower development velocity
- ❌ Smaller talent pool
- ❌ FFI complexity
- ❌ Longer compile times

---

### Option 4: Go

**Pros:**
- ✅ Excellent concurrency (goroutines)
- ✅ Fast compile times
- ✅ Single static binary
- ✅ Great for cloud-native
- ✅ Good performance

**Cons:**
- ❌ Limited ML/AI ecosystem
- ❌ Verbose error handling
- ❌ Limited generics
- ❌ Browser deployment requires WASM

---

### Option 5: Multi-Language (Core in Rust, Bindings in TS/Python/Go)

**Pros:**
- ✅ Best of all worlds
- ✅ Rust core for performance
- ✅ Language-specific SDKs
- ✅ Maximum reach

**Cons:**
- ❌ Significantly more complex
- ❌ Maintenance burden
- ❌ FFI overhead
- ❌ Consistency challenges
- ❌ Longer time to market

---

## Recommendation

**Primary Recommendation: TypeScript/Node.js**

**Rationale:**
1. **Developer Adoption**: Broadest reach for full-stack developers
2. **Ecosystem**: Massive npm ecosystem
3. **Browser-Native**: Runs directly in browsers
4. **WebAssembly**: Can compile critical parts to WASM
5. **Streaming**: Excellent async/streaming support
6. **Tooling**: Best-in-class IDE support
7. **Speed to Market**: Fastest development velocity
8. **Hiring**: Largest talent pool

**Alternative: Multi-Language** if resources allow

## Consequences

### Positive
- Fast development and iteration
- Maximum developer reach
- Easy browser deployment
- Great tooling ecosystem
- Can add Rust WASM modules for performance-critical parts

### Negative
- Performance not as good as Rust/Go
- Single-threaded limitations
- Memory overhead

## Related Decisions
- ADR-002: Package Structure
- ADR-005: Performance Optimization Strategy

---

**Decision Status:** Awaiting final approval
