# @deepseek-ai/dsh-desktop

Electron desktop shell over the web profile. The main process renders the loopback server `dsh web` already provides — no business logic moves: sessions, tools, and the client-plugin module table stay on the host, and the window is a native chrome plus a hardened (`contextIsolation`, no node integration) renderer.

## Run

```sh
bun run desktop        # built frontend: spawns `dsh web --no-open` + the Electron window
bun run dev:desktop    # dev mode: also runs scripts/dev-web.ts so source edits rebuild
                        # lib/client.js and apps/web/dist; the host stat-polls those
                        # artifacts and pushes `rebuilt` frames — the window hot-reloads
                        # changed client plugins (HMR) without restarting Electron
```

Both commands tear down the whole child tree with Ctrl+C. `DSH_DESKTOP_URL` overrides the URL the window loads; the launcher sets it to the spawned host's port.

## Debug window capture

While running, the shell serves a loopback HTTP endpoint so any local process — an editor extension, a test harness, or the dsh agent itself through its shell or web tooling — can screenshot any of its windows for debugging, including windows that are occluded on screen.

Two routes, both requiring the per-run `token` query parameter:

- `GET /debug/windows.json` — every window's id, title, URL, minimized state, and bounds.
- `GET /debug/screenshot.png[?window=<id>]` — PNG of that window's page content; without `window`, the focused window (else the first). A minimized window is restored for the capture and re-minimized.

External processes don't parse logs: each instance publishes `{ pid, port, token }` to `%TEMP%/dsh-desktop-debug/endpoint-<pid>.json` (removed on quit) and logs only the file location. The bundled client wraps discovery and capture:

```sh
node scripts/desktop-screenshot.mjs --list          # live shells and their windows
node scripts/desktop-screenshot.mjs shot.png        # focused window of the (single) shell
node scripts/desktop-screenshot.mjs shot.png --pid 1234 --window 1
```

Trust model: bind is 127.0.0.1 only, responses carry no CORS headers, and the token is random per run — possessing the discovery record is what grants capture access, so treat that file like a credential; screenshots can contain whatever the window renders. Set `DSH_DESKTOP_DEBUG_CAPTURE=0` to disable the listener entirely.

## Known Limitations and Deferred Work

- No packaging story yet (electron-builder / single-exe): this ships as a dev-facing workspace app.
- The window trusts whatever `DSH_DESKTOP_URL` names; the launcher only ever points it at 127.0.0.1.
