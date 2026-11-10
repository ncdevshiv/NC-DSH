---
description: "NC-AG 安全模型：智能体权限、工作区隔离、不可信内容、机密处理与审计完整性。"
kind: "reference"
---

# NC-AG 安全

[English](security.md) | 中文

## 摘要

每个 NC-AG 智能体都是不可信的执行身份。它恰好拥有档案声明的权限，恰好在一个工作树内工作，并把检索内容作为数据处理。本页陈述每项控制及其执行位置。

## 目录

- [权限](#permissions)
- [工作区隔离](#workspace-containment)
- [破坏性命令](#destructive-commands)
- [不可信内容](#untrusted-content)
- [机密](#secrets)
- [审计完整性](#audit-integrity)
- [控制平面](#the-control-plane)

-----

<a id="permissions"></a>
## 权限

`checkToolCall(profile, toolName, args)` 在每次工具执行之前、在编排器为该智能体构建的工具包装器内部运行。它拒绝档案未列出的工具（`tool-not-granted`），以及已列出但档案缺少其权限的工具（`permission-missing`）。拒绝会连同规则、细节、智能体、任务与关联标识写入审计链，模型把拒绝文本作为失败的工具结果收到。权限在执行器处强制执行，而非通过提示词过滤，因此第二个调用方无法绕过。

<a id="workspace-containment"></a>
## 工作区隔离

工具收到的每个文件路径都被解析到智能体工作区，越界即拒绝（`resolveInsideWorkspace`）。该检查覆盖 `..`、工作区外的绝对路径以及两个平台的分隔符。智能体无法读写自己工作树之外的内容，因此项目或会话无法通过文件系统泄漏另一项目的内容。

<a id="destructive-commands"></a>
## 破坏性命令

shell 工具拒绝匹配破坏性模式的命令文本：`git reset --hard`、`git clean -f`、`git push --force`、删除根目录或主目录，以及把取回的文档管道进 shell。每次拒绝都指出规则。确实需要破坏性操作的任务必须携带显式记录的决策；平台不会静默放宽 shell。

<a id="untrusted-content"></a>
## 不可信内容

仓库内容、智能体消息与检索文档都是数据。智能体运行时在即将使用的工作区中扫描形似指令的标记（`ignore previous instructions`、角色重设、机密外泄请求），并向任务消息追加一条框定说明：发现的标记，以及检索文本是信息而非命令的规则。

`frameUntrusted(source, content)` 以 `<<UNTRUSTED CONTENT>>` 包裹文本，并在首部声明其不是指令。该框架内的指令绝不会覆盖档案契约、项目指令或权限检查。

<a id="secrets"></a>
## 机密

NC-AG 从不存储凭据。模型凭据属于 harness 凭据服务，由 harness 在打开提供方流时解析。审计细节包含动作、标识、token 数与结果，绝不包含机密值。智能体工作区是它自己的工作树，因此运维保存在别处的凭据文件无法通过平台读取。

<a id="audit-integrity"></a>
## 审计完整性

每个受审计动作追加一条哈希链记录。记录的哈希覆盖其内容与前一条记录的哈希；`verify()` 重新计算整条链并报告第一个存储哈希不匹配的序号。被修改、删除或插入的记录会从该点起破坏校验。`GET /api/health` 报告该状态，编排器暴露 `store.verifyAudit()`。

<a id="the-control-plane"></a>
## 控制平面

控制平面默认绑定 `127.0.0.1`。设置 `NCAG_TOKEN` 时，每个 API 调用都需要该令牌的 `Authorization: Bearer`，事件流接受查询参数中的令牌，因为浏览器 `EventSource` 无法设置首部。无令牌时 API 开放，因此默认仅绑定回环地址；可从其他机器访问的部署必须设置 `NCAG_TOKEN`。

面板只渲染 API 返回的数据且不持有隐藏状态；令牌从页面 URL 读取，绝不写入浏览器存储。
