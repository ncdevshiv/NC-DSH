# Agent Note: Composer plain-text typing echoes to the glyph layers in one task

Status: implemented

English | [中文](2026-08-23-composer-typing-echo-one-task.zh.md)

## Problem

The composer keeps the native caret in the transparent textarea and paints every visible glyph in the React-rendered backdrop — the two-layer split that makes claim-token highlights, reference chips, and the ghost hint possible ([one scrollport for both text layers](2026-07-31-composer-text-layers-share-one-scrollport.md)). That split leaves the two sides on different update paths: the caret moves inside the keypress task, while the backdrop repaints at the component's next React commit. Nothing bounds that commit on a loaded main thread — streaming-transcript renders share it — so typing into a live session could land its commit frames late, leaving the newest characters invisible while the caret was painted past them: the blinking line read as falling behind, or landing between, the words around it. The single-scrollport fix bound the caret to its glyphs against scroll offsets; the per-keystroke gap owned by React scheduling remained open.

## Decision

`InputBar.onChange` patches both text layers' Text nodes synchronously whenever the frame stays decoration-free — no claim token, chips, lexicon highlight, or ghost hint before or after the keystroke. The machine dispatches first, `deriveDecorations` over the post-dispatch snapshot decides, and the patch writes exactly the strings the later commit writes, so reconciliation changes nothing visually. One ref (`plainEchoRef`) records which draft the DOM currently shows, letting keystrokes that outrun a commit keep taking the fast path; a decorated frame falls back to the normal commit path where structural DOM changes belong. Paste, undo, cut, and chip operations keep their existing paths unchanged.

## Testing

`input-bar.client.spec.tsx` asserts the patch already applied inside the act window — before any commit could run — and that a lexicon-highlight frame skips the echo and builds its marked range through the commit. Playwright probes under `.artifacts/caret-probe/` measured the healthy-frame baseline this decision generalizes: zero catch-up lag idle and under synthetic congestion headless, and exact trusted-click caret placement — evidence the defect lived in real-page commit latency rather than geometry.

## Alternatives considered

**Show the textarea's own glyphs and paint decorations above them.** Rejected: reference chips replace their leading glyph with a domain icon, which requires the textarea's own text to stay transparent; flipping glyph ownership to the textarea breaks the chip look the split exists to provide.

**Draw the caret ourselves on the compositor.** Rejected: a self-drawn caret removes the native composition caret that IME input requires, and the product drafts in Chinese as often as English.

**Shrink transcript render cost until commits are always prompt.** Rejected: useful work, but it shrinks the gap instead of closing it — no render budget can promise a commit within the keystroke's own task, which is the property the echo provides by construction.

## Consequences

Bought: for all plain-draft typing, visible glyphs land in the same task as the keystroke, so caret-to-glyph separation no longer depends on React scheduling, store traffic, or main-thread load. Cost: one extra `deriveDecorations` scan per keystroke on the event path, and the echo's correctness leans on checking the plain precondition against post-dispatch machine state rather than the rendered DOM — the ref bridges the window between patch and commit, which is one more piece of state a refactor of either layer must carry along.
