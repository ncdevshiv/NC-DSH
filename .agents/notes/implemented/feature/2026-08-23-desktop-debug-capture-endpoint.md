# Agent Note: Desktop debug window-capture endpoint

Status: implemented

English | [中文](2026-08-23-desktop-debug-capture-endpoint.zh.md)

## Problem

Debugging the desktop shell's rendered UI required OS-level screen capture: an external tool grabs whatever pixels overlap the window rect, so an occluded or minimized window yields the wrong surface, and nothing about the flow is reachable from the dsh agent or other local tooling without a human at the controls. UI bugs therefore surfaced only when a user happened to screenshot the right moment.

## Decision

The Electron main process serves a loopback debug-capture listener (`apps/desktop/main.mjs`) alongside the window: `GET /debug/windows.json` lists every window's id, title, URL, minimized state, and bounds, and `GET /debug/screenshot.png[?window=<id>]` returns `webContents.capturePage()` PNG bytes for that window's page content regardless of what overlaps it on screen. A minimized window is restored for the capture and re-minimized; a window that never painted answers 409.

Discovery stays out of logs: each instance writes `{ pid, port, token }` to `%TEMP%/dsh-desktop-debug/endpoint-<pid>.json`, removes the record on quit, and prints only the file location on stdout. Both routes require the per-run random `token`; bind is 127.0.0.1 only and responses carry no CORS headers, so possessing the discovery record is the capture grant. `DSH_DESKTOP_DEBUG_CAPTURE=0` disables the listener. `scripts/desktop-screenshot.mjs` is the bundled external client (discovery by pid liveness, listing, single-window capture) and doubles as the path the dsh agent calls through its shell tool.

## Alternatives considered

- **Second-instance CLI forwarding** (`app.requestSingleInstanceLock` + argv): keeps everything inside Electron but every capture pays a full process spawn, and multiple dev instances complicate lock ownership.
- **File-drop trigger** (watcher writes PNGs on request): zero ports, but no structured window listing, polling latency, and a second file contract to keep current.
- **Expose captures through the `dsh web` host plane**: the backend owns no windows; capture must live in the process whose surfaces are captured.
- **OS-level capture (PrintWindow / CopyFromScreen)**: foreground rights, overlapping-pixel artifacts, and no programmatic discovery — the exact failure being fixed.

## Consequences

Any local process that can read the user's temp directory can silently screenshot every open shell window; the token bounds this to holders of the record but cannot stop a same-user process, which is the trust domain Electron already runs in. A minimized window visibly flashes during capture because Chromium has no compositor surface while iconic. Stale records after a crash are skipped by pid liveness in the client rather than cleaned up eagerly. The endpoint is debug-only: it renders state the loopback server plus OS already expose to the same user, and carries none of the business logic that stays behind `dsh web`.
