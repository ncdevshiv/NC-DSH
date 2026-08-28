# dsh-turn-restore

[English](README.md) | 中文

用于编辑重发 fork 的回退时工作区恢复。`session.fork` 的 `beforeSeq` 回退会丢弃整个轮次；本策略提供 `ctx.turnRestore`，将那些被丢弃轮次中记录的 `write`/`edit` 工具基准按逆序重放回会话工作区，并在子会话发布前完成，使工作区与对话拼接点一致，而不是残留被移除的工作。

## Plugin（命名空间：`turn-restore`）

零配置函数插件；host 的 `session.fork` 通过 `ctx.get('turnRestore')` 读取它，因此未组合本插件的应用只回退对话而不动工作区：

```yaml
- id: turn-restore
  name: '@deepseek-ai/dsh-turn-restore'
```

恢复过程完全由会话日志事实驱动——`write`/`edit` 工具附加在 `tool/result` meta 上的全文恢复基准（见 `dsh-tool-fs`）——因此对在线与持久化来源的重放一致，不存在单独的日志。对每条基准（最新优先），仅当磁盘当前内容仍等于基准的写后文本时才把文件改写回写前文本；出现分歧（期间的用户编辑）会变成报告的冲突而不是被覆盖。

## Model Experience

### Summaries

#### 模型看到什么

本策略不增加提示词、工具 schema 或会话事件。回退发生在轮次之间，绝不在轮次内部，因此没有模型请求会观察到中间文件状态。

#### Token 影响

无变化。

#### KV Cache 影响

无变化。

## Known Limitations and Deferred Work

- 只有 `write` 与 `edit` 携带恢复基准。`str_replace_editor`、`bash`、`pwsh` 与 `terminal` 的变更会被计数（`notRestorable` / `shell`）并报告，不会被回退。
- 超过 fs-local `diffBasisMaxBytes` 上限的 `write` 不记录写前文本（update 的 `before: null`），因此无法恢复；报告会计数。
- 源 agent 运行中时恢复被跳过（`skipped: 'source-running'`），无工作区根目录的 chat 会话也会被跳过（`skipped: 'no-cwd'`）；两种情况下的回退本身仍会继续。
- 行尾：基准为 LF 规范化，因此从 LF 基准恢复的 CRLF 文件会以 LF 行尾重写。
- 延后：`str_replace_editor` 的逆序恢复（同一 meta 形状）与逐轮次 git 式检查点。
