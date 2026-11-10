---
kind: upgrade-guide
description: "模型设置新增供应商与模型的停用和排序字段，可配置供应商目录改为返回产品名而非路由标识。"
---

# 模型设置支持停用与排序供应商，目录名称改为产品名

[English](guide.md) | 中文

## 变更

`llm-pi-ai` 和 `llm-deepseek` 设置节接受新字段。供应商 profile 接受 `enabled`；值为 `false` 时该路由从供应商注册表撤回，因而从 `listProviders`、浏览器模型目录以及所有指名它的请求中消失，而其端点、凭据引用和模型列表保持存储。profile 还接受 `modelOrder`，用模型 id 列表决定该路由的服务顺序；每个 `models` 条目和每个 `modelOverrides` 值都接受 `enabled` 以撤回单个模型。`llm-pi-ai` 另接受节级 `providerOrder`，决定配置界面列出路由的顺序；其中引用没有 profile 的路由会被拒绝。各处缺省都保持原有行为：未设置 `enabled` 即启用，未设置或为空的排序保持配置顺序，因此不含这些字段的已存储文档行为与之前完全一致。

`llm/listConfigurableProviders()` 现在在 `displayName` 返回供应商的产品名——`OpenAI`、`Moonshot AI`——此前返回的是路由标识（`openai`、`moonshotai`）。把标签与路由标识作比较的调用方会受影响；`provider` 字段仍携带标识。

撤回的模型会从 `listModels` 消失，因此选择该模型 id 的 Session 会被 `session/selectModel` 拒绝，与拒绝未知模型的方式相同。

## 迁移

1. 无需改动即可继续使用。要暂停供应商，把其 profile 的 `enabled` 设为 `false`；要恢复，删除该字段或设为 `true`。
2. 要隐藏未声明 `models` 列表的目录路由上的某个模型，添加 `modelOverrides: { "<model-id>": { enabled: false } }`。要排序，把 id 写入 `modelOrder`。
3. 凡此前用 `displayName` 匹配可配置供应商路由标识的代码，改为比较 `provider`。
4. 通过调用 `llm/listProviders` 和 `llm/listModels` 确认：已暂停的供应商与被撤回的模型在两者中都不存在，重新启用后无需其他编辑即恢复。
