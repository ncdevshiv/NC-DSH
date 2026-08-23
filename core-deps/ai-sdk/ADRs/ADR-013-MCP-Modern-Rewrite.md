# ADR-013: MCP Implementation — Modern Stateless Rewrite (2026-07-28)

**Status:** Accepted
**Date:** 2026-08-10
**Deciders:** Project owner (Shivam Tiwari), ZCode engineering agent

## Context

The first MCP implementation (`ADR-010` era, 2025-08-09) followed the
**2025-03-26** revision: an `initialize` handshake, connection-scoped
capability negotiation, and session state. Anthropic/MCP released a new
protocol revision — **2026-07-28** — that is a **fundamental architectural
rewrite**:

- **Stateless**: "There is no negotiation handshake." Every request carries
  `_meta.io.modelcontextprotocol/protocolVersion` and
  `_meta.io.modelcontextprotocol/clientCapabilities` (both REQUIRED).
- `server/discover` is a REQUIRED server method returning
  `{ supportedVersions, capabilities, instructions? }`.
- Every result carries `resultType` (`"complete"` | `"input_required"`).
- **Multi Round-Trip Requests (MRTR)**: servers answer with
  `InputRequiredResult { inputRequests, requestState }`; clients fulfill the
  input requests and retry with `inputResponses` + `requestState`.
- **Elicitation** (`elicitation/create`, form/url modes) is a client
  capability; sampling/roots are deprecated (SEP-2577).
- **Subscriptions**: `subscriptions/listen` replaces the HTTP GET endpoint;
  notifications carry `io.modelcontextprotocol/subscriptionId`.
- New error codes: `-32020` HeaderMismatch, `-32021`
  MissingRequiredClientCapability, `-32022` UnsupportedProtocolVersion
  (with `data.supported`); legacy `-32002` retired in favor of `-32602`.
- Streamable HTTP with the `MCP-Protocol-Version` header; modern errors map
  to HTTP 400. JSON Schema 2020-12 is the default dialect. OpenTelemetry
  trace context propagates via `_meta`.

## Decision

**Complete rewrite of the MCP module** (`ai-protocols/src/mcp.rs`,
`mcp_http.rs`) to the 2026-07-28 stateless architecture:

1. Per-request `_meta` validation (missing fields → `-32602`; unsupported
   version → `-32022` with `data.supported`; missing capability → `-32021`
   with `data.requiredCapabilities`).
2. `server/discover` + client version retry/negotiation.
3. `resultType` on all results.
4. MRTR: tool handlers return `HandlerOutcome::Complete | NeedsInput`;
   clients fulfill `elicitation/create` (and `sampling/createMessage` via a
   resolver hook) and retry with `inputResponses`/`requestState`.
5. Elicitation support with capability gating (server refuses to request
   input the client did not declare).
6. `subscriptions/listen` with `subscriptionId`-correlated notifications on
   stdio and SSE-over-HTTP.
7. Streamable HTTP client + server (`MCP-Protocol-Version` header, HTTP 400
   mapping).
8. OTel trace-context passthrough via reserved `_meta` keys (metadata
   carried, not interpreted).

**Deliberately not implemented:** the legacy initialize-handshake era
(dual-era support is a roadmap item), Roots (deprecated by SEP-2577),
HTTP+SSE (removed).

## Rationale

1. The stateless model invalidates the old handshake core — patching would
   produce a hybrid satisfying neither era.
2. Nothing has shipped: rewrite cost is minimal now.
3. The 2026-07-28 wire format is what current servers/clients speak.

## Consequences

### Positive

- Conforms to the current MCP spec (verified against
  `schema/2026-07-28/schema.ts`).
- MRTR/elicitation/subscriptions give servers and agents modern
  interaction patterns (long-running tasks, user-input collection).
- HTTP transport with proper version/capability semantics.

### Negative

- Legacy MCP clients (2025-11-25 and earlier, e.g. older Claude Desktop)
  cannot talk to this server until dual-era support lands.

## Related

- `ENGINEERING-SPEC.md` §16 (updated), ADR-010 (superseded MCP details),
  `ENGINEERING-LOG.md` 2026-08-10 entry.
