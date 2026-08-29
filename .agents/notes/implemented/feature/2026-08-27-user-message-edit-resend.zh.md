# Agent Note: 用户消息编辑重发

Status: implemented

[English](2026-08-27-user-message-edit-resend.md) | 中文

## Problem

已发送的用户气泡只有复制操作。2026-07-31 的编辑存根说明记录过：编辑必须与背后的能力一起回归——既需要针对已定稿用户消息的 client 变更，也需要 host 对已经消费过它的轮次给出行为。从末尾向前分支会保留错误的轮次，因此缺少“修正提问并从那里重跑”的手势。

## Decision

编辑重发是向后 fork，而不是日志编辑：

- **Host `session.fork` 新增 `beforeSeq`**（与 `atSeq` 互斥）。它反转锚点：子会话继承该锚点所在轮次开始之前的所有事件，因此锚点的整个轮次及之后的所有事件都留在父会话。锚点所在轮次未完成、或锚点落在所有轮次之外时，以 `fork-unavailable` 失败。切割复用既有锚点路径的尾随追加规则，因此上一个被保留轮次与切割点之间的独立事件仍会进入子会话种子。
- **客户端转发** —— `ISessions.fork` 与 `SessionManager` 与 `atSeq` 一样接受并向下取整 `beforeSeq`。
- **UI** —— 打开已完成轮次的用户气泡新增编辑操作（steering 与开放轮次的开启消息不显示，对应 fork 的 `OPEN_TURN` 规则）。点击后打开回退对话框，说明有多少后续轮次保留在原会话；确认后以 `beforeSeq` fork、打开子会话，并把原文本经输入 shell 的草稿写入路径恢复进子会话编辑器。不会自动发送：用户编辑后正常发送，与 Codex 桌面语义一致。

本次不新增任何会话日志格式或会话事件——父会话原封不动，追加式不变量成立；只有新 locale 与一个 RPC 参数产生移动。

## Alternatives considered

**原地截断日志。** 否决：历史按构造追加，fork 方案保留原分支可再选择。

**fork 后自动发送编辑后的提示词。** Codex 把提示词恢复到编辑器；自动发送会夺走用户要的最后一次确认。

**把动作倒向文件回退。** 现在回退会通过日志中的 `write`/`edit` 基准恢复工作区文件（[回退工作区恢复](2026-08-29-rewind-workspace-restore.md)）；无基准的 shell 与 `str_replace_editor` 变更会被报告，不会被回退。

## Consequences

原会话保持完整且可再选择；子会话携带 `parentSession` 谱系。只有文本内容被恢复进编辑器——编辑消息中的图片附件与非文本内容不会被带入，对话框对此不作声明。2026-07-31 的存根删除说明被本变更取代；分支操作仍只存在于 assistant 之下（[user-bubbles-drop-the-branch-action](../simplification/2026-08-06-user-bubbles-drop-the-branch-action.md) 在分支层面继续有效）。
