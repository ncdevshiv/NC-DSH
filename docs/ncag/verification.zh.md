---
description: "NC-AG 如何判定任务完成：它要求的证据、证据存放位置，以及如何复现验证。"
kind: "reference"
---

# NC-AG 验证

[English](verification.md) | 中文

## 摘要

任务在其已记录的验收证据通过时完成，绝不因为智能体声称成功。证据是每次已执行检查的一行记录，携带标准、方法、确切命令、退出码、捕获输出与产生它的智能体。本页陈述命令如何解析、记录什么，以及如何复现一次验证。

## 目录

- [规则](#the-rule)
- [命令从何而来](#where-the-command-comes-from)
- [记录什么](#what-is-recorded)
- [审查是独立证据](#reviews-are-separate-evidence)
- [复现验证](#reproducing-a-verification)
- [检查为何会无法运行](#why-a-check-can-fail-to-run)

-----

<a id="the-rule"></a>
## 规则

`unverifiedCriteria(repositories, taskId, criteria)` 列出没有通过证据的标准。编排器把任务移到 `VERIFICATION_PENDING`，执行每个标准，仅当该列表为空时才完成任务。否则任务带着未验证标准的标识作为原因移到 `FAILED`。两种结果都作为 `verification/completed` 事件发出，含每个标准的方法、状态与命令。

<a id="where-the-command-comes-from"></a>
## 命令从何而来

命令在规划设置时是标准自身的 `command`，否则是项目为该标准验证方法配置的检查。项目以方法到命令的表保存 `checks`：

```ts
createProject({
  name: 'checkout',
  description: 'order API',
  repositoryPath: '/abs/path/repo',
  checks: {
    'unit-test': 'npm test',
    'typecheck': 'npx tsc --noEmit',
    'manual-review': 'git diff --check',
  },
})
```

平台绝不猜测命令。方法缺失意味着该检查在此部署中无法执行，标准带着原因失败而不是通过。

<a id="what-is-recorded"></a>
## 记录什么

每次已执行检查追加一行证据：任务、项目、标准标识、方法、状态（`PASSED`、`FAILED`、`ERROR`）、确切命令、退出码、捕获输出（前 8000 字符）、记录时间与产生它的智能体。输出是被截断而非摘要的：保留真实输出的开头字符，使后来的读者能看到命令实际打印了什么。

<a id="reviews-are-separate-evidence"></a>
## 审查是独立证据

审查与验证回答不同问题。审查询问已交付证据是否显示缺陷：没有变更文件、命令失败、标准未验证、完成报告为空，或被委派审查者以具名工件为据的发现。验证询问标准现在是否通过。审查拒绝在验证前停止任务；验证失败在验证后停止它。

每条审查发现携带严重度、描述、证据、工件、影响、建议修正、验证方法与状态。发现是持久的，解决后仍然可见。

<a id="reproducing-a-verification"></a>
## 复现验证

证据行指出工作区与命令。复现方式：

```bash
cd <workspace path recorded for the agent>
<the exact command from the evidence row>
```

在同一工作区重跑该命令会复现记录的退出码与输出，审计者正是以此核对交付与其证据一致，而不是信任报告。

<a id="why-a-check-can-fail-to-run"></a>
## 检查为何会无法运行

当配置的 shell 找不到时，检查记录 `FAILED` 与 `spawnSync <executable> ENOENT`。NC-AG 依次从 `NCAG_SHELL`、Windows 上的 `powershell.exe` 与其他平台的 `pwsh` 解析 shell；缺少该 shell 的部署必须配置 `NCAG_SHELL`。失败被记录为证据而非吞掉，因为未运行的检查并未通过。
