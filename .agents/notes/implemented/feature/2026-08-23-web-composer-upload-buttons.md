# Agent Note: Composer upload buttons and folder-aware drops

Status: implemented

English | [中文](2026-08-23-web-composer-upload-buttons.zh.md)

## Problem

The composer had no click path into attachment intake at all: the leading plus button opens the slash-command menu (its documented role), and images entered only through clipboard paste or a whole-window drag. A user reaching for "+" to attach a file found nothing, and dropped folders yielded nothing because the flat `dataTransfer.files` read cannot see inside a directory.

## Decision

Two sibling launcher circles join the plus in InputBar's tool row: 上传图片 opens a hidden `multiple` `<input type="file">` whose accept filter mirrors the projected media types (the fixed v1 image list when no service projects limits), and 上传文件夹 opens a hidden `webkitdirectory` picker set through its ref (no React prop exists). Both feed the existing `intakeImages` pre-check, so picked batches obey exactly the paste rules — whole-batch refusal, format banner first, immediate product copy — and the input value clears after each change so one path can be re-picked. Both buttons disable under the same gate as the drop target (locked, machine-busy, or no attachment service).

Whole-page drops now descend: `filesFromDataTransfer` in ui-attachment reads every item's entry synchronously (entry handles die after the event turn), walks directories through chunked `readEntries` until an empty page, guards cycles by visited entry identity, skips unreadable files and errored subdirectories while keeping siblings, and falls back to the flat files list when no item exposes an entry or the walk collects nothing. The blocked-drop check stays ahead of the traversal.

The two launchers sit as menu rows under the plus button ([Composer plus menu](2026-08-24-web-composer-plus-menu.md)); no upload protocol, wire field, or new menu component is introduced, and attachments remain images-only v1.

## Alternatives considered

- **Turn "+" into a launcher menu** with Command/Upload rows: needs a second floating menu beside ui-input-trigger's `MenuView`, which the composer decision forbids, and hides both gestures behind an extra click. Superseded on both grounds by [Composer plus menu](2026-08-24-web-composer-plus-menu.md).
- **File System Access API (`showDirectoryPicker`)**: Chromium-only; `webkitdirectory` covers Chromium, Firefox, and Safari from one input.
- **Filter non-images out of folder picks client-side**: silently dropping files contradicts the settled whole-batch-refusal semantics; the format banner names the problem instead.
- **Extend the attachment seam to documents now**: crosses host admission, the wire envelope, model-visible logging, and provider capability — a capability-seam change, not a gesture gap; deferred until document support is wanted.

## Consequences

Folder uploads are image-filtered by admission rather than by the picker: choosing a mixed folder refuses the whole batch with the format banner — honest but noisy for large mixed trees. `webkitdirectory` gives no per-file dialog feedback and some engines ignore `accept` entirely, so the rail remains the confirmation surface. The traversal relies on entry objects being valid within the drop event turn, which is why collection is synchronous-first. Non-image file cards and upload progress stay deferred (#2248), tracked in the ui-attachment README limitations. The intake pre-check these gestures feed is owned by [Web image intake and limits alignment](2026-08-12-web-image-intake-and-limits-alignment.md).
