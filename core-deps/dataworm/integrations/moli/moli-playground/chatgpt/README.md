# ChatGPT Moli CDP Demo

This demo starts `moli serve`, connects through raw CDP, opens
`https://chatgpt.com/`, fills the email/password login form, then optionally
sends a prompt and prints the latest assistant response.

It does not bypass CAPTCHA, MFA, passkeys, or other human verification steps.
If ChatGPT asks for one of those, the script exits with a clear error.

## Run

From this directory:

```bash
uv run --with websockets python chatgpt_cdp_demo.py --email "$CHATGPT_EMAIL" --prompt "Say hello in one sentence."
```

The password is read with hidden terminal input. To use environment variables:

```bash
CHATGPT_EMAIL="you@example.com" \
CHATGPT_PASSWORD="..." \
uv run --with websockets python chatgpt_cdp_demo.py --prompt "Say hello in one sentence."
```

For a login-only smoke:

```bash
uv run --with websockets python chatgpt_cdp_demo.py --login-only
```

For an interactive prompt loop after login:

```bash
uv run --with websockets python chatgpt_cdp_demo.py
```

For the full-screen TUI:

```bash
uv run --with websockets python chatgpt_cdp_tui.py
```

The preferred TUI uses Playwright as the CDP client framework:

```bash
uv run --with websockets --with playwright python chatgpt_playwright_tui.py
```

For one-shot Playwright debugging:

```bash
uv run --with websockets --with playwright python chatgpt_playwright_demo.py \
  --email "$CHATGPT_EMAIL" \
  --password-stdin \
  --prompt "今天几号？"
```

If `--profile-dir` already contains a logged-in session, avoid reading
credentials during repeated performance probes:

```bash
uv run --with websockets --with playwright python chatgpt_playwright_demo.py \
  --use-existing-session \
  --profile-dir .profile \
  --moli-bin ../../target/release/moli \
  --timestamps \
  --prompt "只回复 OK"
```

If the account asks for device approval, the Playwright CLI waits until
`--login-timeout` expires. To switch that page to email-code verification:

```bash
uv run --with websockets --with playwright python chatgpt_playwright_demo.py \
  --email "$CHATGPT_EMAIL" \
  --password-stdin \
  --try-email-verification \
  --auth-code-prompt \
  --prompt "今天几号？"
```

The Playwright TUI defaults to a longer login timeout and switches device
approval prompts to email-code verification by default. It shows an `Auth Code`
input when the code page is reached. Pass `--no-try-email-verification` if you
want to wait for the mobile device approval prompt instead.

TUI keys:

- `Tab`: switch input field
- `Enter` or `Ctrl-S`: login or send the current prompt
- `Ctrl-L`: clear the message log
- `Ctrl-Q`: quit and stop `moli serve`

Useful options:

- `--moli-bin ../../target/release/moli`
- `--profile-dir .profile` to persist cookies/localStorage between runs
- `--http-proxy http://127.0.0.1:7890` if the Moli runtime needs a proxy
- `--http-no-proxy 127.0.0.1,localhost` when testing against local URLs
- `--http-timeout 120000` is the demo default for Moli, because ChatGPT can serve large script bundles slowly through CDN/challenge paths
- `--http-max-concurrent 16` to test more active fetch transfers on script-heavy ChatGPT loads
- `--debug-snapshot` to print sanitized DOM snapshots around login steps
- `--timestamps` to print elapsed seconds for each status line in the one-shot Playwright demo
- `--answer-timeout 300` to wait longer for slow ChatGPT responses
- `--no-reload-recovery` in the Playwright demo/TUI to expose live DOM render failures instead of recovering persisted answers by reloading the conversation
- `--live-trace` in the Playwright demo/TUI to record in-page fetch, response body stream, WebSocket, event-loop, MessageChannel, app-state, Router loaderData, and DOM selector trace events for live-render debugging
- `--live-trace-output /tmp/chatgpt-live.jsonl` to append sanitized trace summaries as JSON Lines for diffable debugging
- `--login-timeout 180` to allow time for device approval or email-code entry
- `--auth-code-stdin` to pipe password and email code as two stdin lines

`--profile-dir` stores login cookies/localStorage. Treat that directory like a
credential store.

Prompt responses print `answer source: live-dom` when the current page renders
the assistant text directly. `answer source: persisted-reload` means the demo
had to reload the conversation and read the persisted response; that proves the
prompt/backend path worked, but not that live rendering is fixed.

Prompt submission in the Playwright demo/TUI first tries Playwright-native
composer input and send-button click. The older page helper remains as a
fallback so login/session debugging can continue when native input regresses.

For live-render debugging, combine `--no-reload-recovery --live-trace`. In that
mode a timeout after entering `/c/...` is useful: the output includes sanitized
conversation fetch/WebSocket/body-stream, event-loop, app-state, selector, and
patch-frame summaries so a persisted reload cannot hide a live DOM failure.
Current traces also report whether `MessagePort.onmessage`,
`requestIdleCallback`, the conversation route loaderData, React thread fiber
identity, shallow thread selector hook state, and selected thread fiber source
hints progressed. The live trace also includes a focused conversation
materialization probe: it checks whether the current React conversation wrapper
can already produce display turns even when those turns have not appeared in
the DOM. It also records focused source/lazy-payload hints for the thread-list
Suspense branch so a no-reload timeout can distinguish "data exists but render
subtree stayed suspended" from "conversation data never arrived".

Known limitation as of 2026-05-27: a no-reload run can reach `/c/...` with
conversation data materialized inside the React wrapper while the live DOM still
has no user/assistant turn. Treat `answer source: persisted-reload` as a
recovery path, not as proof that live conversation rendering is fixed.

For long traces, add `--live-trace-output /tmp/chatgpt-live.jsonl`. Each timeout
or answer result appends one sanitized JSON object with the compact trace
summary and answer path reason.

If prompt submission appears to hang, run the CLI with `--debug-snapshot` first.
The script reports whether it found the composer, whether the send button became
usable, and the last visible conversation state before timeout.
