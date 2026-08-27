# Notifications

English | [中文](notifications.zh.md)

The dismissible-notification seam of the DeepSeek Harness host. Any plugin publishes a user-visible notification under a stable id through [`ctx.notifications`](../../packages/host/notifications/README.md); the seam owns read, dismissed, and deleted state for every id, persists it as one JSON document under the harness home, and announces each committed mutation as an event. Producers stay free of storage and lifecycle concerns, and future Host or Client surfaces render the same records without their own stores. Publishing an existing id replaces its content, resets `dismissed` and `read` to `false`, and preserves the original `createdAt`; inputs are cloned at publish time so later caller mutations never reach stored or handed-out views.

Source: [`packages/host/notifications/src/index.ts`](../../packages/host/notifications/src/index.ts)

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
