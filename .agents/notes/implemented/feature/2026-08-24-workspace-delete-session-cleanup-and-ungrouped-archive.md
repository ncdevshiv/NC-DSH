# Agent Note: Workspace Deletion Session Cleanup and Ungrouped Bulk Archive

Status: implemented

English | [中文](2026-08-24-workspace-delete-session-cleanup-and-ungrouped-archive.zh.md)

## Problem

Deleting a Workspace registration removes the workspace entity from the registry while preserving its persisted session logs. Because the sidebar browser derives grouping by collecting each registered Workspace's accounted sessions and dumping any remaining unassigned sessions into the "Ungrouped" bucket, deleting a project caused all of its sessions to spill into Ungrouped. Users who wanted to delete or clean up a project had to manually archive dozens of loose sessions one by one. Furthermore, the Ungrouped section header had no action menu to clear or archive all unassigned sessions in bulk.

## Decision

1. **Workspace Delete Dialog Option**: The Workspace deletion modal in `WorkspaceBrowser` now includes an opt-in checkbox ("Also archive sessions in this workspace", checked by default). When confirmed, the client automatically triggers `archiveSession` for all sessions belonging to the deleted workspace, preventing them from spilling into Ungrouped.
2. **Ungrouped Bulk Archive Action**: `ProjectRowItem` and `SessionTree` now support group actions on the Ungrouped section header. When ungrouped sessions exist, the header reveals an action menu with "Archive all sessions" (`归档全部会话`), archiving all loose sessions in one gesture.

## Verification

Unit tests in `packages/client/ui-workspace` pin:
- Checking/unchecking the session archive option during workspace deletion.
- Opening the Ungrouped group menu and archiving all ungrouped sessions.
- Full typecheck and component test coverage.
