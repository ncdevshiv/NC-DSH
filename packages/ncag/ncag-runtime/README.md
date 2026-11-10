---
description: "The NC-AG agent runtime: a provider-neutral model port, sandboxed workspace tools, prompt assembly, and the agent turn loop."
kind: "package-library"
---

# @deepseek-ai/dsh-ncag-runtime

English | [中文](README.zh.md)

## Summary

This package executes one NC-AG agent. It defines the model port and the tool contract, provides the six workspace tools and the two coordination tools, assembles the system prompt and the assignment message, adapts the harness LLM service to the port, and runs the turn loop: ask the model, execute the tools it requested, await them, feed the results back, stop on a final answer or the step budget.

## Table of Contents

- [Ports](#ports)
- [Tools](#tools)
- [Prompt assembly](#prompt-assembly)
- [The loop](#the-loop)
- [The harness model port](#the-harness-model-port)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)
- [Dev Note](#dev-note)

-----

<a id="ports"></a>
## Ports

`LlmPort.complete(request)` returns text, tool calls and usage. `LlmRequest` carries the provider, the model, the optional reasoning effort, the system prompt, the messages, the tool schemas, the output ceiling and a cancellation signal. `ToolEnvironment` carries the workspace, the agent, the task, the project, the signal and an optional `AgentCoordination` surface through which the coordination tools reach the orchestrator.

A port implementation is the only difference between production and a test: the shipped adapter routes through the harness LLM service, and a test supplies a scripted implementation. Everything else — the loop, the tools, the prompt — runs identically.

<a id="tools"></a>
## Tools

| Tool | Permission | Behavior |
|---|---|---|
| `read` | workspace:read | numbered lines, clipped at 200 KB |
| `write` | workspace:write | creates parents, writes the complete content |
| `edit` | workspace:write | replaces exactly one occurrence, refusing zero or several |
| `glob` | workspace:read | walks the workspace, `**` patterns, 200 results |
| `grep` | workspace:read | regular-expression search with line numbers, 200 matches |
| `shell` | shell:execute | runs a command in the workspace, returns exit code and output |
| `request_help` | communication:send | asks the orchestrator for a specialist, whose real answer comes back as the tool result |
| `hand_off` | communication:send | sends a handoff package and lets the orchestrator re-queue the task for another agent |

The coordination tools answer through a promise because they wait for the orchestrator; a tool with no attached coordination surface reports exactly that instead of pretending the request was served.

Every path is resolved inside the workspace and refused when it escapes. `resolveShell` picks the command shell from `NCAG_SHELL`, then `powershell.exe` on Windows and `pwsh` elsewhere, so an unresolvable shell fails the call with the executable it looked for instead of skipping the work silently.

<a id="prompt-assembly"></a>
## Prompt assembly

`buildSystemPrompt(profile, context)` renders the identity, working style, expertise, responsibilities, restrictions, quality standard and escalation rules, then the project name and instructions, approved architecture, contracts and constraints. `buildAssignmentMessage(task, context)` renders the task, its description, inputs, expected outputs, acceptance criteria, completed dependency outputs and the coordination messages addressed to the agent.

<a id="the-loop"></a>
## The loop

`runAgent(port, request)` builds one `LlmRequest` per step, calls `beforeRequest` with the exact request, awaits every requested tool under `onToolCall`/`onToolResult`, appends the results to the transcript as tool messages, and returns `{ finalText, completed, steps, usage, transcript }`. A malformed argument payload, an unknown tool name and a missing required argument each become a failed tool result rather than an exception, so the model can correct them. Exhausting `maxSteps` returns `completed: false` with the transcript so far.

<a id="the-harness-model-port"></a>
## The harness model port

`createLlmPort(llm)` maps the runtime request onto a `GenerateOptions` value, streams the harness chunks, accumulating text deltas and tool-call deltas by block index, and returns the assembled blocks. The tool-call arguments are the raw JSON the model produced, which the loop then parses and validates.

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **One tool call per block, no images.** The port assembles text and tool calls; image and file blocks in a tool result are not projected yet, and no NC-AG tool produces one.
- **Tool execution is synchronous.** A long command blocks the agent's step. The signal bounds the run, and a per-call deadline belongs in the tool contract once a deployment needs it.

<a id="dev-note"></a>
## Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

The runtime deliberately depends on ports, not services: it imports no Cordis service and no filesystem backend, which is what lets the orchestrator drive it inside a harness process, in a test, or in a standalone operator tool.

</details>
