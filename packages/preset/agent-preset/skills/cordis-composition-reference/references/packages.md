# Loadable Harness plugin packages

This file is GENERATED from workspace manifests (`scripts/gen-plugin-packages.ts`) and verified fresh by `bun run verify-plugin-packages` (part of `doc-sync`); do not edit it by hand.

Every package below exports a Cordis plugin that a bundle patch can name in a Loader row. `Config` marks packages whose row accepts a `config` mapping; query `Config.listConfigs` through `cordis_inspect_query` (filter by `name`, then query the `entry` id) for the mounted schema. Packages under `experimental` are pre-stable.

## api

| Package | Config | Description |
|---|---|---|
| `@deepseek-ai/dsh-api-account-controller` | no | Expose safe account operations over authenticated Remote |
| `@deepseek-ai/dsh-api-gateway` | yes | Typert Remote Host dispatcher and Client API endpoint |
| `@deepseek-ai/dsh-api-job-controller` | yes | Job Remote observation stream and the reference-counted client job-output service |
| `@deepseek-ai/dsh-api-remote-access` | no | Method-level Remote access control for non-owner Peers and the per-Peer access snapshot |
| `@deepseek-ai/dsh-api-session-controller` | yes | Session Remote commands, cold reads, and live control transport |
| `@deepseek-ai/dsh-api-settings-controller` | yes | Remote owner for the configuration surfaces over the settings-domain seams |
| `@deepseek-ai/dsh-api-terminal-controller` | yes | Session-owned interactive terminals with shell discovery, screen recovery and typed Remote control |
| `@deepseek-ai/dsh-api-workspace-controller` | yes | Workspace Remote commands and reconnect-safe state transport |

## browser-use

| Package | Config | Description |
|---|---|---|
| `@deepseek-ai/dsh-browser-use` | no | Exclusive named browser-use provider registration |

## client

| Package | Config | Description |
|---|---|---|
| `@deepseek-ai/dsh-client-product-analytics` | yes | Desktop product event collection and authenticated Host reporting |
| `@deepseek-ai/dsh-client-ui-settings-account` | yes | Manage DeepSeek login and open Platform billing pages |
| `@deepseek-ai/dsh-client-ui-sidebar-terminal` | no | Interactive shell tabs for the right Sidebar |

## computer-use

| Package | Config | Description |
|---|---|---|
| `@deepseek-ai/dsh-computer-use` | no | Exclusive named computer-use provider registration |

## credentials

| Package | Config | Description |
|---|---|---|
| `@deepseek-ai/dsh-deepseek-account-platform` | yes | Authorize DeepSeek accounts through browser PKCE |

## experimental

| Package | Config | Description |
|---|---|---|
| `@deepseek-ai/dsh-experimental-api-speech-to-text` | yes | Authenticated experimental speech transcription for browser clients |
| `@deepseek-ai/dsh-experimental-browser-use-chrome-devtools-mcp` | yes | Experimental per-Session Chromium browser tools through chrome-devtools-mcp |
| `@deepseek-ai/dsh-experimental-browser-use-playwright-mcp` | yes | Experimental per-Session Chromium browser tools through @playwright/mcp |
| `@deepseek-ai/dsh-experimental-browser-use-stagehand-native` | yes | Experimental Stagehand browser tools with separately configured native models |
| `@deepseek-ai/dsh-experimental-client-ui-claude-code-mods` | no | Web band above the prompt for Claude Code mods: draws each session's mod tree and sends button presses back to the bridge |
| `@deepseek-ai/dsh-experimental-client-ui-cot-translation` | yes | Optional machine translation of expanded reasoning, preserving original Session text |
| `@deepseek-ai/dsh-experimental-client-ui-voice-input` | no | Record speech and insert editable text into the conversation draft |
| `@deepseek-ai/dsh-experimental-computer-use-cua-driver-mcp` | yes | Experimental computer use through an installed Cua Driver MCP executable |
| `@deepseek-ai/dsh-experimental-computer-use-cua-driver-native` | no | Experimental computer-use provider embedding the Cua Driver native npm SDK |
| `@deepseek-ai/dsh-experimental-cua-macos` | yes | macOS native provider for shared Computer Use application and window objects |
| `@deepseek-ai/dsh-experimental-speech-to-text` | yes | Experimental speech recognition with independently selectable providers |
| `@deepseek-ai/dsh-experimental-speech-to-text-sensevoice` | yes | Local SenseVoice ONNX transcription with a managed sherpa-onnx process |
| `@deepseek-ai/dsh-experimental-translator` | yes | Machine translation with reusable Session results |

## extensions

| Package | Config | Description |
|---|---|---|
| `@deepseek-ai/dsh-cordis-host-runner` | yes | Dynamic package definition registry, host-half sandbox lifecycle, and invoke handler table for model-mounted dual-half packages |

## shell

| Package | Config | Description |
|---|---|---|
| `@deepseek-ai/dsh-pwsh-local` | yes | Local PowerShell implementation of the DeepSeek Harness bash executor seam |
| `@deepseek-ai/dsh-shell-env` | yes | Tool-independent managed DSH_* shell environment registry |
| `@deepseek-ai/dsh-tool-pwsh` | yes | Model-facing pwsh tool over the bash executor seam |

## typert

| Package | Config | Description |
|---|---|---|
| `@deepseek-ai/dsh-typert-loader` | yes | Loader integration for generated Typert package contributions |
