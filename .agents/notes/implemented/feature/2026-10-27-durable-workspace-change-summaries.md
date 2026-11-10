# Agent Note: Durable workspace change summaries

Status: implemented

English | [中文](2026-10-27-durable-workspace-change-summaries.zh.md)

## Problem

The changed-files card is served by the Host process that recorded the turn. A Host restart or a cold attach leaves the Session log's `workspace/changes` announcement with no summary behind it, so the browser's request for the summary route answers 404 and the card silently disappears. The user sees no card for a turn that did change files, and the browser console shows a 404 that names no cause.

## Decision

The Host stores every recorded summary durably, in the `workspace_changes` storage domain: one document per Session, holding that Session's summaries by the sequence of the event that announced them. A document carries the file list, the line counts, and the working directory. A later Host over the same Harness home serves the stored summary, so a reopened conversation keeps the file lists of its earlier turns, and the native open of a listed file works there too.

The comparison is not stored. It needs both sides of each file, which are snapshot objects and captured copies in the recorder's temporary directory; a durable copy of them would put the workspace's content in the Harness home a second time. A turn recorded by an earlier Host therefore lists its files and states that the comparison is unavailable.

The `workspace/changes` event keeps carrying only the turn number. The event remains the announcement that a turn changed files, and the store holds the list that announcement names. The Session log is uploaded to the DeepSeek session-log service, so a file list in the event would carry every workspace path off the machine.

The client reads the Host's answer as one of three states. A summary the Host does not serve leaves a notice naming the unavailable file list where the card would be; a failed read is a separate state that offers Retry beside the same notice; a served summary renders the card as before.

The plugin now declares the `storageDomain` service, so a composition that mounts it without the storage stack does not activate it. The shipped Web bundle mounts the storage stack already.

## Alternatives considered

**The file list in the `workspace/changes` event** was the first implementation and was rejected in the [historical Turn changed-files card decision](../../archived/feature/2026-09-11-turn-changed-files-card.md): the card would then render from the log forever while the content it opens would not survive. The session-log upload decides it again, because every event in the log is sent to the DeepSeek API.

**A Harness-home object store for the snapshots** was considered in the same decision and rejected there; the comparison still has no durable home, and the summary does not need one.

**Keeping the summary in the Host process and explaining the 404** leaves the user without the file list of a turn that did change files, which is the information the card exists to carry. The notice is added as well, because a summary the Host cannot serve is a state the user should see rather than an unexplained empty card.

**A storage domain per turn** would write one document per announced turn. A document per Session keeps the number of documents proportional to the number of conversations and makes the read a single lookup.

## Consequences

The Harness home gains a `workspace_changes` store: one JSON document per Session under the storage root, holding that Session's summaries. The plugin never deletes these documents; removing them means deleting that store.

The stored summary omits the git snapshot ids, because the objects they name live in the recorder's temporary directory and die with it. A summary served from the store therefore differs from the live one in those ids alone, which no route serves.

The comparison remains live-turn-only. A reopened turn shows its file list and states that the comparison is unavailable, and the review tab reports the same.

Focused tests cover the store's write and read, a summary served by a later Host over the same root, a record that fails its schema, a store that does not open, a summary that cannot be stored, the client's three states, and the notices they render.
