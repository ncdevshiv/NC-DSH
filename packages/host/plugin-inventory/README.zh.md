# @deepseek-ai/dsh-host-plugin-inventory

[English](README.md) | 中文

当前 Cordis Loader 树的只读 Host 投影。`PluginInventoryGateway` 注册 `pluginInventory` 服务，并发布两个由 Typert 生成的直接 Remote：`pluginInventory/list` 与 `pluginInventory/setEnabled`。每次 `list` 调用都直接读取 `ctx.loader.entries()`，跳过结构性的 group 行，再按 Loader 顺序返回其余条目，并且只包含 Loader 条目 id、模块标识、有效启用状态与当前根 Fiber 阶段。`setEnabled` 通过 `ctx.loader.update` 切换一个条目的 `disabled` 标志并报告 Loader 是否接受。当启用与另一个已激活的同模块条目冲突时——同一作用域内两个已挂载实例无法发布同一个服务——网关会替换它：先停用正在运行的重复实例，重试一次启用；若替换失败则恢复原状。

阶段为 `pending`、`loading`、`active`、`failed` 或 `unloading`；条目没有存活的根 Fiber 时则为 `null`。该快照刻意只表示调用当下：Loader 仍是唯一的生命周期权威，本包不拥有缓存、历史、来源模型、事件流或修改路径。公开 payload 类型位于 `./types`，Typert 生成由 `./typert` 与 `./remote` 导出的 Host 和 Client Remote 产物。

该服务仅供 Remote 使用，刻意不声明同进程 Cordis `Context` merge。Client 包通过显式的 [`api-remotes`](../../api/remotes/README.md) 组合消费它，而不导入 Host 实现。

## 模型体验

无，因为这个仅限 Host 的清单投影不注册提示词、工具、消息或提供方请求。

#### KV Cache 影响

无；本包从不组装模型输入。

## 已知限制与暂缓事项

- **仅表示调用当下** —— 结果不包含持久的失败历史或订阅；只要不存在存活的根 Fiber，就会报告 `null`，而不区分其原因。
- **无来源** —— 服务不识别条目由哪个 bundle、profile 或 override 引入，也不能添加或移除插件。
- **仅运行期切换** —— `setEnabled` 通过 `ctx.loader.update` 在当前进程内切换 `disabled`；更改不会自动持久化到 profile 的 `cordis.patch.yml`，除非调用方另行修补该文件，否则重启后失效。group 条目不可切换。替换式启用会让被替换的重复实例在本次进程内保持停用；这次替换同样不会持久化。
