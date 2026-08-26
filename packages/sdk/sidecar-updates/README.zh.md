# @deepseek-ai/dsh-sidecar-updates

English | [README.md](README.md)

`ai-sidecar` 模型引擎二进制的自动更新管线。本服务轮询 GitHub 仓库的最新发布，依据发布自带的 `SHA256SUMS` 清单校验平台资产，将其实装到按版本划分的目录布局中，并原子地改指 `current.json` 指针。检查与安装结果通过通知接缝（[`ctx.notifications`](../../host/notifications/README.md)）与 `sidecar-updates/status` 事件对外呈现；这里不触碰任何模型请求——新的二进制要到模型引擎下次启动时才会生效。

## 布局

```text
<installDir>/current.json                     pointer: {tag, asset, sha256, installedAt, exePath}
<installDir>/ignored.json                     tags ignored through ignore()
<installDir>/downloads/<tag>/<asset>          staged download bytes
<installDir>/releases/<tag>/ai-sidecar[.exe]  installed executables
```

每个 tag 都安装到自己的目录，且每个文档都以原子方式替换（随机后缀独占创建的临时文件加改名），因此安装永远不会覆盖正在运行的二进制，崩溃也只可能留下完整的前一个或后一个指针。损坏的指针或忽略清单会被视为不存在，而不会阻塞安装。

## 配置

| 键 | 默认值 | 含义 |
|---|---|---|
| `repo` | `ncdevshiv/ai-sdk` | 轮询发布物的 `owner/name` 仓库。 |
| `installDir` | `<cwd>/core-deps/ai-sdk` | 安装目录根，容纳指针、各版本与下载。 |
| `checkOnStart` | `true` | 启动后执行一次检查。 |
| `intervalMs` | — | 轮询间隔毫秒数，范围 [60s, 24h]；省略则禁用轮询。 |
| `assetPrefix` | `ai-sidecar` | 资产命名形如 `{prefix}-{platform}-{arch}[.exe]`。 |
| `autoInstallOnFirstRun` | `true` | 首次观察到发布且尚未安装任何版本时自动安装。 |
| `apiBase` | `https://api.github.com` | GitHub API 基址（覆盖它可指向测试或镜像）。 |
| `ignoredVersions` | `[]` | 忽略清单的只读种子；运行时经 `ignore()` 扩充。 |

可选的 `sidecar-updates` 设置命名空间经 [`dsh-settings`](../../settings/settings/README.md) 把用户层叠加在编排入口配置之上。该节的 `ignoredVersions` 字段是忽略清单的种子；运行时的权威列表是 `<installDir>/ignored.json`，由 `ignore()` 扩充。

## 服务

`status()` 返回冻结快照 `{ installed, latest, updateAvailable, ignoredLatest, lastError? }`，每次调用都从活文档重读。`updateAvailable` 表示最新发布未被忽略、且没有安装相同或更新的版本；`lastError` 描述最近一次失败。

`checkNow()` 抓取 `releases/latest`，刷新缓存的比较状态，对账通知，并在“首次成功检查、尚无安装、且 `autoInstallOnFirstRun`”时自动安装该发布。传输、HTTP 与解析失败会写入 `lastError`、记录警告并返回状态而不是抛出。

`install(requestedTag?)` 下载目标资产与其 `SHA256SUMS`，校验摘要，暂存到 `downloads/<tag>/`，写入 `releases/<tag>/ai-sidecar[.exe]`，改指 `current.json`，发布已安装通知并发出状态。只有公开的最新发布可以安装；其他 tag 以 `UNKNOWN_RELEASE` 失败。所有失败都会抛出携带稳定机器码的 [`SidecarUpdateError`](./src/github.ts)（`UNSUPPORTED_PLATFORM`、`ASSET_MISSING`、`CHECKSUM_MANIFEST_MISSING`、`CHECKSUM_ENTRY_MISSING`、`CHECKSUM_MISMATCH`、`DOWNLOAD_FAILED` 等）。

`ignore(tag)` 把标签追加到持久化忽略清单并立即对账通知：忽略最新发布会清空 `sdk-update:*` 通知并置位 `ignoredLatest`。

## 通知

有行动价值的检查会发布 id `sdk-update:{tag}`（kind `sdk-update`）、标题 "AI SDK update available"、正文 `Installed {installed} → available {tag}` 的通知。仅当内容变化时才会刷新该条目，因此关闭一条通知后将保持关闭，直到不同的发布或安装状态使内容过时；过时的 `sdk-update:*` 条目会被删除。完成的安装发布 id `sdk-update-installed:{tag}`（kind `sdk-update-installed`）、标题 "AI SDK {tag} installed" 的通知。

## 模型体验

### Sidecar 更新基础设施

#### 模型看到什么

什么都不看到。该管线不注册任何工具、提示词段落或 Session 事件；它管理 `installDir` 下的文件与 Host 接缝中的通知，更新后的二进制只会影响下一个进程生命周期由哪个引擎提供服务。

#### Token 影响

为零。发布元数据、摘要、下载字节与状态快照都不会进入模型请求；连 `sdk-update:*` 通知也位于其外。

#### KV Cache 影响

相互独立。检查、安装与忽略不会触碰任何模型请求前缀；在会话之间替换 sidecar 二进制也不会使原本可复用的提供方缓存条目失效。

## 已知限制与延期工作

- **Windows 上被锁定的可执行文件** —— 正在运行的 `.exe` 无法被替换，因此安装始终写入全新的版本目录并改指指针；旧版本目录不会被回收，删除仍需手动进行。
- **未认证的 GitHub 访问** —— 发布查询不携带令牌，继承匿名限额（约每地址每小时 60 次）；分钟级以下的轮询间隔会耗尽配额。
- **校验和信任模型** —— 完整性只对照同一发布经 HTTPS 取回的 `SHA256SUMS` 验证；摘要之外的签名、来源证明或固定不在范围内，因此一个被攻陷的发布即意味着一次被攻陷的安装。
- **只能安装最新发布** —— 唯一的查询是 `releases/latest`，因此固定版本、降级或任意 tag 安装需要另一套发现契约。
- **没有重启编排** —— 管线报告 `restartRequired` 但从不重启或排空模型引擎本身。
