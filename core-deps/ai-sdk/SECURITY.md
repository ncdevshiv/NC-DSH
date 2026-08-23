# Security Policy

## Reporting a Vulnerability

Please report security vulnerabilities privately to the repository owner via
email (`ncdevshiv@gmail.com`) or a GitHub private advisory. Do **not** open a
public issue for security problems.

## Security principles in this project

1. **Secrets are never committed.** API keys and credentials are read from
   environment variables or config files; `.env` is gitignored.
2. **Sensitive data is never logged accidentally.** All observability paths
   run through configurable redaction (API keys, authorization headers,
   cookies, provider secrets, PII) — see `ai-security`.
3. **SSRF protection.** The web subsystem validates and normalizes
   user-supplied URLs, enforces schemes/ports, and applies blocklist rules
   before any request (see `ai-web`/`ai-security`).
4. **Tool permission boundaries.** Tools execute only within declared
   permissions; dangerous operations (filesystem, command execution) require
   explicit opt-in.
5. **Bounded resources.** Concurrency, queue depths, timeouts, and content
   sizes are bounded by configuration to prevent resource exhaustion.

## Supported versions

Only the current development version (`0.1.x`) is supported while the project
is pre-1.0.
