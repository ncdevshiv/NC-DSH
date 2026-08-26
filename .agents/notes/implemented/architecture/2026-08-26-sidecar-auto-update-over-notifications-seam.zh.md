# Agent Note: Sidecar auto-update pipeline over a shared notifications seam

English | [2026-08-26-sidecar-auto-update-over-notifications-seam.md](2026-08-26-sidecar-auto-update-over-notifications-seam.md)

Status: implemented

## 问题

`ai-sidecar` 模型引擎二进制在 npm 之外分发，此前没有更新路径：运维人员需要盯发布、下载正确的平台资产、信任它、替换二进制并手动重启。任何想要呈现更新状态的宿主或客户端界面都得自建存储、关闭处理与事件管线，把这条接缝按功能重复一遍。

## 决策

两个插件端到端地拥有这项能力。`@deepseek-ai/dsh-notifications` 是通用的可关闭通知 Service Definition，挂在 `ctx.notifications` 下：稳定 id 的发布/替换语义、已读/已关闭/已删除状态、`<harness home>/notifications/v1/state.json` 下一个原子重写的 JSON 文档、经包含处理的 `notifications/updated` 与 `notifications/removed` 事件，以及一个用事件流对照存储做断言的不变量伴生插件。`@deepseek-ai/dsh-sidecar-updates` 是更新器的消费方：轮询 GitHub 的 `releases/latest`，将 `{prefix}-{platform}-{arch}` 资产对照发布自带的 `SHA256SUMS` 校验，安装到按 tag 划分的版本目录布局中，并原子改指 `current.json`，因此运行中的二进制永远不会被覆盖。每次检查、安装与忽略都通过通知接缝（`sdk-update:{tag}`、`sdk-update-installed:{tag}`）与 `sidecar-updates/status` 事件提交状态快照。被忽略的版本存放在 `<installDir>/ignored.json`；设置节的 `ignoredVersions` 字段只是加载时合并进去的种子。可选的 `sidecar-updates` 设置命名空间经 `installSettingsSection` 把用户层叠加在编排入口配置之上。

## 曾考虑的替代方案

- **由更新器自己写设置** —— 为了一个数组字段让更新器耦合进设置的写入路径。败给安装目录文件：自成一体、与管线的其余状态同样原子，且仍可由配置播种。
- **原地替换二进制** —— 覆盖 `ai-sidecar` 在 Windows 上会失败（运行中的 `.exe` 被锁定），且交换中途崩溃会留下损坏的引擎。败给版本目录加指针的布局：整个安装在一个改名处全有或全无。
- **通知作为 sidecar-updates 的私有存储** —— 私有表今天可行，但会迫使未来每个通知生产方（jobs、workflows）重复持久化与关闭逻辑。败给共享接缝：生产方完全不必关心存储。
- **完整 semver 引擎** —— 这里的 tag 只是数字点分发布名；引入依赖只增加表面积而没有现实依据。败给严格的数字点分比较器，缺失的段记为零。

## 后果

更新状态在每一步都能挺过崩溃：暂存下载、各版本可执行文件与指针各自原子提交，且不变量伴生插件会对任何指针指向不存在可执行文件的状态报错。同时接受了这些取舍：完整性止于发布自身清单的摘要（无签名验证）、未认证的 GitHub 查询继承匿名限额、只能安装公开的最新发布、被取代的版本目录仍需手动删除。
