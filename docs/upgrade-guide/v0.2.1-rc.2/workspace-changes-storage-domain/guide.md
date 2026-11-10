---
kind: upgrade-guide
description: "The workspace-changes plugin now waits for the storage stack, and its change summaries are stored under the Harness home."
---

# The workspace-changes plugin waits for the storage stack

English | [中文](guide.zh.md)

## Change

`@deepseek-ai/dsh-workspace-changes` now declares the `storageDomain` service. A composition that mounts the plugin without the storage stack no longer activates it: the plugin waits for that service, so the changed-files card and its per-file comparisons stop appearing, while the rest of the composition keeps running. The shipped Web bundle mounts the storage stack already, so its behavior does not change.

The plugin stores each recorded change summary in the `workspace_changes` storage domain: one JSON document per Session under the storage root of the Harness home, at `storages/workspace_changes/sessions/<session-id>.json`. A Host that did not record the turn serves that stored summary, so after a restart the changed-files card of an earlier turn lists its files again. The per-file comparison is still served only for a turn the running Host recorded, and the plugin never deletes the stored documents.

## Migration

1. In a custom `cordis.yml`, mount the storage rows before `@deepseek-ai/dsh-workspace-changes`:

   ```yaml
   - id: storage
     name: '@deepseek-ai/dsh-storage'
   - id: storage-json
     name: '@deepseek-ai/dsh-storage-json'
     config:
       root: !!js dshHomePath('storages')
   - id: storage-domain
     name: '@deepseek-ai/dsh-storage-domain'
     config:
       backend: json
   ```

2. Confirm the change by opening a conversation that changed files: the turn ends with a changed-files card, and the Harness home holds `storages/workspace_changes/sessions/<session-id>.json` for that Session.
