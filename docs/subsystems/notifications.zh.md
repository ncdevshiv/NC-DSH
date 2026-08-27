# 通知

[English](notifications.md) | 中文

DeepSeek Harness 宿主的可关闭通知缝。任何插件都通过 [`ctx.notifications`](../../packages/host/notifications/README.md) 在一个稳定 id 下发布用户可见通知;该缝为每个 id 拥有已读、已关闭和已删除状态,将其作为一份 JSON 文档持久化在 harness home 下,并将每次已提交的变更作为事件宣布。生产者无需关心存储和生命周期,未来的 Host 或 Client 界面渲染相同的记录而无需自己的存储。发布已有 id 会替换其内容,将 `dismissed` 和 `read` 重置为 `false`,并保留原始的 `createdAt`;输入在发布时克隆,因此后续调用方变更永远不会触及已存储或发出的视图。

来源:[`packages/host/notifications/src/index.ts`](../../packages/host/notifications/src/index.ts)

<!-- BEGIN GENERATED cordis-surface (gen-cordis-catalog.ts) — do not edit between markers -->

<a id="cordis-surface"></a>

## Cordis API

Generated from source by `scripts/gen-cordis-catalog.ts` (verified fresh by `bun run verify-cordis-catalog` in doc-sync; regenerate with `bun run gen-cordis-catalog`) — this section is byte-identical in both language sides of the page. Signature blocks use a `ts cordis-catalog` fence and keep the original source JSDoc; dispatch modes are defined in the [primer](../cordis-primer.md#dispatch-modes), and the framework-inherited `ctx` API lives in [cordis-api/inherited.md](../cordis-api/inherited.md).

<a id="ctxnotifications--notificationsservice"></a>

### `ctx.notifications` — `NotificationsService`

File-backed dismissible-notification service.

```ts cordis-catalog
/**
 * Publish one notification, or replace the content of the existing entry
 * with the same id. Replacement keeps `createdAt` and resets `dismissed`
 * and `read` to `false`; the input is cloned, so later caller mutations of
 * `data` never reach stored or handed-out views.
 * @param input - identity and content of the notification.
 */
publish(input: NotificationPublishInput): void

/**
 * Snapshot every live notification, newest first (reverse insertion order;
 * replacement retains its original position). Views are frozen clones, so
 * caller mutation never reaches the store.
 * @returns frozen views ordered newest first.
 */
list(): readonly NotificationView[]

/**
 * Mark one notification read or unread. An unknown id fails loud; setting
 * the current value again changes nothing and emits nothing.
 * @param id - the notification to update.
 * @param read - next read state; defaults to `true`.
 */
setRead(id: string, read: boolean = true): void

/**
 * Dismiss one notification (keeps it listed with `dismissed: true`). An
 * unknown id fails loud; dismissing twice changes nothing.
 * @param id - the notification to dismiss.
 */
dismiss(id: string): void

/**
 * Delete one notification. Deleting an absent id is already satisfied.
 * @param id - the notification to delete.
 */
delete(id: string): void
```

Source: [`packages/host/notifications/src/index.ts:60`](../../packages/host/notifications/src/index.ts)

<a id="notifications-events"></a>

### `notifications/*` events

<a id="notificationsremoved--emit"></a>

#### `notifications/removed` — emit

A notification was deleted: the id is absent from {@linkcode NotificationsService.list}.

```ts cordis-catalog
/**
 * A notification was deleted: the id is absent from
 * {@linkcode NotificationsService.list}.
 * @param id - the deleted notification id.
 * @mode emit
 */
'notifications/removed'(id: string): void
```

Source: [`packages/host/notifications/src/index.ts:52`](../../packages/host/notifications/src/index.ts)

<a id="notificationsupdated--emit"></a>

#### `notifications/updated` — emit

A notification was published or replaced, or its read/dismiss state changed: the id is present in {@linkcode NotificationsService.list}.

```ts cordis-catalog
/**
 * A notification was published or replaced, or its read/dismiss state
 * changed: the id is present in {@linkcode NotificationsService.list}.
 * @param id - the notification id whose entry changed.
 * @mode emit
 */
'notifications/updated'(id: string): void
```

Source: [`packages/host/notifications/src/index.ts:45`](../../packages/host/notifications/src/index.ts)
<!-- END GENERATED cordis-surface -->
