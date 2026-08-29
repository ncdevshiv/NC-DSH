# Agent Note: 回退工作区恢复

Status: implemented

[English](2026-08-29-rewind-workspace-restore.md) | 中文

## Problem

编辑重发（2026-08-27 用户消息说明）把对话回退到编辑的轮次之前，但工作区保留了被丢弃轮次的文件变更——那是父线程的状态，不是子会话的。Codex 的等价功能明确不回退文件（交给 git）；我们检查点计划的设计目标更强：用户回退时，工作区应与拼接点一致。

## Decision

新增策略包 `dsh-turn-restore`，提供 `ctx.turnRestore`（零配置函数插件，与 `session-checkpoint-policy` 一样可选组合）。`session.fork` 的 `beforeSeq` 会在发布子会话之前咨询它并逆序重放被丢弃的事件：

- **日志本身就是账本。** `write`/`edit` 工具本来就把它应用的上下文 hunk 附加到 `tool/result.meta`；现在还会附加全文 `basis`（后端同源计算的 before/after，外加 `create`/`update`/`edit` 操作）。tool/result meta 是工具私有的不透明、经 JSON 校验的槽位——不新增会话事件、不改 SDK 表面，天然可重放。
- **恢复是观察安全的。** 基准按最新优先应用，且仅当磁盘当前内容仍等于基准的写后文本；期间的用户编辑会把该条变成报告的冲突而不是覆盖。内容匹配的 create 文件会被删除；已缺失的 create 文件计为前置状态。
- **如实报告限制，而不是静默留空。** 无基准的变更会被报告：`str_replace_editor` 与超限写入（update 的 `before: null`）计为 `notRestorable`，`bash`/`pwsh`/`terminal` 计为 `shell` 并带名称。源 agent 仍在运行或会话没有 cwd 时绝不执行恢复；两种情况都以 `skipped` 原因跳过。
- **客户端呈现结果。** `session.fork` 的 `beforeSeq` 响应新增可选 `restoreReport`（host 契约上的摘要型、客户端契约镜像）；编辑重发流程会把它渲染为子会话编辑器上的 info/error 提示。

## Alternatives considered

**逐轮次文件系统快照。** 全树快照会恢复工具无法证明它写过的文件并加倍存储；日志驱动的基准方案精确恢复被移除轮次所做的变更，除此之外不动任何文件。

**独立账本存储。** 需要自己的持久化、重放与裁剪规则；工具 meta 已携带事实且已持久可重放。

**源 agent 运行中执行恢复。** 拒绝：会被仍在运行的分支覆盖，构成竞态。带原因跳过让子会话保持一致，用户停掉源后再回退一次即可。

## Consequences

回退现在会恢复被移除轮次的所有 `write`/`edit` 文件效果（受 fs-local diff 基准上限约束），并报告一切无法回退的内容。Bash 副作用仍无法回退（没有东西能回滚任意 shell 运行）；报告会列出运行过的 shell 工具。父分支保留自己的日志，运行中则保留自己的文件工作——busy 跳过之后的共享工作区属于父分支接下来做什么。
