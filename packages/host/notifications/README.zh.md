# @deepseek-ai/dsh-notifications

English | [README.md](README.md)

DeepSeek Harness 宿主端的可关闭通知接缝。任意插件通过 `ctx.notifications` 以稳定 id 发布用户可见通知；本接缝为每个 id 维护已读、已关闭与已删除状态，将其持久化到 Harness 主目录下的一个 JSON 文档，并以事件公告每一次已提交的变更。生产方无需关心存储与生命周期，未来的 Host 或 Client 界面可以直接渲染同一份记录而无需各自建库。

以既有 id 再次发布将替换其内容（`kind`、`title`、`body`、`data`），把 `dismissed` 与 `read` 重置为 `false`，并保留原始 `createdAt`。输入在发布时被深拷贝，因此调用方随后对 `data` 的修改不会影响存储或外发的视图。`list()` 按插入序的逆序（最新在前）返回冻结快照；替换保留原有位置。

## 服务

| 方法 | 行为 |
|---|---|
| `publish(input)` | 插入或替换一条通知；发出 `notifications/updated`。 |
| `list()` | 全部存活条目的冻结快照，最新在前。 |
| `setRead(id, read?)` | 标记已读（默认 `true`）或未读；未知 id 显式报错。 |
| `dismiss(id)` | 置 `dismissed: true`；条目仍保留在列表中。 |
| `delete(id)` | 移除条目；删除不存在的 id 视为已完成；发出 `notifications/removed`。 |

`NotificationView` 包含 `id`、`kind`、`title`、可选的 `body`、可选的 `data`、ISO-8601 的 `createdAt`，以及 `read`/`dismissed` 两个标志。

## 事件

发布、替换、已读或关闭提交后发出 `notifications/updated(id)`；删除提交后发出 `notifications/removed(id)`。派发时，updated 事件的 id 必定存在于 `list()`，removed 事件的 id 必定不存在——这正是包内不变量伴生插件所断言的关系。

监听器失败按监听器隔离：某一个抛出或拒绝的观察者只会被记录日志，不会拖垮其余观察者；携带 INVARIANT 代码的失败仍在全部监听器执行完毕后向外抛出。

## 持久化

状态位于 `<Harness 主目录>/notifications/v1/state.json`（解析顺序为 `$DSH_HOME`，再 `~/.dsh`；可用 `dshHome` 配置覆盖）。每次变更都以原子方式整体重写文档——随机后缀独占创建的临时兄弟文件加改名——读取者与崩溃只可能看到完整文档。写入是同步的：变更返回时其状态即已落盘。

文档按整体解析：坏的 JSON、未知的格式版本或任何非法行都会使整个存储视为损坏。损坏的存储以一次警告开始于空状态，而不是暴露部分行；下一次变更会将其重写干净。文件不存在等价于空存储。

```yaml
- name: '@deepseek-ai/dsh-notifications'
  config:
    dshHome: /var/lib/dsh
```

## 模型体验

### 通知状态

#### 模型看到什么

什么都不看到。`ctx.notifications` 不注册任何工具、提示词段落、Session 事件或面向模型的上下文；除非某个单独成文的 Consumer 明确转发某条通知，通知始终位于模型请求路径之外。

#### Token 影响

为零。标题、正文、负载、时间戳与关闭状态都不会进入模型请求。

#### KV Cache 影响

相互独立。发布、已读、关闭或删除通知不会触碰任何模型请求前缀，也不会使原本可复用的提供方缓存条目失效。

## 已知限制与延期工作

- **尚无 UI 消费方** —— 本接缝只提供记录、变更与事件；渲染、角标计数与关闭流程属于另行负责的 Host 或 Client 界面。
- **单进程写入者** —— 变更在每个服务实例内经原子的整文档替换串行执行，没有跨进程锁；共享同一 Harness 主目录的两个宿主可能互相整文档覆盖。
- **无上界的保留** —— 存储会保留每一条记录直到被显式删除；配额或生存时间策略等待具体消费方给出定义。
- **整文档重写** —— 每次变更重写所有行；由于格式没有增量日志，非常大的存储每次变更都要支付线性写放大。
