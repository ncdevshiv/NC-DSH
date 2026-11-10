---
kind: upgrade-guide
description: "workspace-changes 插件现在等待存储栈，它的改动摘要保存在 Harness home 下。"
---

# workspace-changes 插件等待存储栈

[English](guide.md) | 中文

## 变更

`@deepseek-ai/dsh-workspace-changes` 现在声明 `storageDomain` 服务。没有存储栈的组合不再激活该插件：插件会等待该服务，因此改动文件卡片及其逐文件对比不再出现，组合的其余部分照常运行。正式提供的 Web bundle 本就挂载了存储栈，其行为不变。

插件把每份记录下来的改动摘要存放在 `workspace_changes` 存储域中：Harness home 的存储根下每个 Session 一份 JSON 文档，位于 `storages/workspace_changes/sessions/<session-id>.json`。没有记录该轮次的 Host 会提供这份已存的摘要，因此重启之后，先前轮次的改动文件卡片会再次列出其文件。逐文件对比仍然只由运行中的 Host 为它记录过的轮次提供，插件也从不删除这些已存文档。

## 迁移

1. 在自定义 `cordis.yml` 中，把存储相关行挂载在 `@deepseek-ai/dsh-workspace-changes` 之前：

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

2. 打开一个改动了文件的对话即可确认：该轮次以改动文件卡片收尾，且 Harness home 下有该 Session 的 `storages/workspace_changes/sessions/<session-id>.json`。
