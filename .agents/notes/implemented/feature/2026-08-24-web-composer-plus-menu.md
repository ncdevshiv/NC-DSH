# Agent Note: Composer plus menu merges the upload and command launchers

Status: implemented

English | [中文](2026-08-24-web-composer-plus-menu.zh.md)

## Problem

The composer tool row carried three sibling launcher circles — `+` for the command menu, 上传图片 for the image picker, 上传文件夹 for the folder picker. Three icons front one concept, "add something to the draft", and the product direction for the web composer is the merged-launcher pattern recent harness UIs converged on: one visible `+` whose menu names the capabilities. The [composer upload buttons](2026-08-23-web-composer-upload-buttons.md) note rejected a launcher menu on two grounds: it would put a second floating menu beside ui-input-trigger's `MenuView`, and it would hide both gestures behind an extra click.

## Decision

The `+` button is the tool row's only launcher, and it opens one menu built from the shared ui-primitives `Menu` — the same primitive the Access chip in the same row renders. Three rows: 上传图片 and 上传文件夹, a separator, then 命令. The upload rows click the same two hidden inputs as before (the composer's only file inputs; the accept filter and `webkitdirectory` are unchanged), and the 命令 row calls the same `toggleCommandMenu` over the current textarea selection, so ui-input-trigger's `MenuView` remains the sole command pick path. Each row keeps the exact gate its separate button had: attachment rows disable while the composer cannot accept images (`canAcceptDrop`), the command row while locked or the command face is absent, and the launcher itself disables only when no row is reachable. A pick closes the menu and refocuses the textarea before dispatch — the command menu arbitrates keystrokes through the textarea, and the OS picker returns focus to the previously focused element.

Both grounds for the earlier rejection are answered: the "second floating menu" is reuse of an existing shared primitive rather than a new component, and the extra click is accepted as the cost of one legible entry point — paste and whole-page drop remain one-gesture paths. The earlier note's rejection is superseded on this point only; its pickers, intake pre-check, and folder-aware drops stay owned there.

## Alternatives considered

- **Keep the three buttons**: three icons for one concept is the surface the product asked to collapse, and a permanent folder icon buys no more discoverability than a menu row.
- **`+` opens attachments only, commands stay on typed `/`**: the menu would then not name every capability the row offers, and the command launcher was the `+`'s existing role. Revisit if the command menu gains its own dedicated affordance.
- **A purpose-built popover**: duplicates the shared `Menu`'s anchoring (upward via `side="top"`), outside-click and Escape dismissal, disabled rows, and icon slots.

## Consequences

The file inputs, intake pre-check, limits, and error copy are untouched, so paste, drop, and picker behavior are identical; only the entry points collapsed. The launcher's tooltip and accessible name move to the new `input.add` locale key ('添加' / 'Add') while the rows reuse the existing keys, so the trigger announces "Add" rather than the command label, and screen-reader users reach each capability through the menu rows. `commandMenuOpen` now gates only queue-steering arbitration, not launcher chrome.
