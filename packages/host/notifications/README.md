# @deepseek-ai/dsh-notifications

English | [中文](README.zh.md)

The dismissible-notification seam of the DeepSeek Harness host. Any plugin publishes a user-visible notification under a stable id through `ctx.notifications`; the seam owns read, dismissed, and deleted state for every id, persists it as one JSON document under the harness home, and announces each committed mutation as an event. Producers stay free of storage and lifecycle concerns, and future Host or Client surfaces render the same records without their own stores.

Publishing an existing id replaces its content (`kind`, `title`, `body`, `data`), resets `dismissed` and `read` to `false`, and preserves the original `createdAt`. Inputs are cloned at publish time, so later caller mutations never reach stored or handed-out views. `list()` returns frozen snapshots newest first in reverse insertion order; replacement retains the original position.

## Service

| Method | Behavior |
|---|---|
| `publish(input)` | Insert or replace one notification; emits `notifications/updated`. |
| `list()` | Frozen newest-first snapshot of every live entry. |
| `setRead(id, read?)` | Mark read (default `true`) or unread; unknown ids fail loud. |
| `dismiss(id)` | Set `dismissed: true`; the entry stays listed. |
| `delete(id)` | Remove the entry; deleting an absent id is already satisfied; emits `notifications/removed`. |

A `NotificationView` carries `id`, `kind`, `title`, optional `body`, optional `data`, the ISO-8601 `createdAt`, and `read`/`dismissed` flags.

## Events

`notifications/updated(id)` fires after a publish, replace, read, or dismiss commits; `notifications/removed(id)` fires after a delete commits. At dispatch time the id is present in `list()` exactly for updated events and absent for removed events — the relation the package-owned invariant companion asserts.

Listener failures are contained per listener: one throwing or rejecting observer is logged and never starves the others, while INVARIANT-coded failures still surface after every listener ran.

## Persistence

State lives at `<harness home>/notifications/v1/state.json` (`$DSH_HOME`, then `~/.dsh`; override with the `dshHome` config). Every mutation rewrites the document atomically — random-suffix exclusive-create temp sibling plus rename — so readers and crashes observe complete documents only. Writes are synchronous: when a mutation returns, its state is durable.

The document parses as a whole: bad JSON, an unknown format version, or any invalid row makes the entire store corrupt. A corrupt store starts empty with one warning instead of surfacing partial rows; the next mutation rewrites it clean. Absence is an empty store.

```yaml
- name: '@deepseek-ai/dsh-notifications'
  config:
    dshHome: /var/lib/dsh
```

## Model Experience

### Notification state

#### What the model sees

Nothing. `ctx.notifications` registers no tool, prompt section, Session event, or model-facing context; notifications live outside the model request path unless a separately documented Consumer explicitly forwards one.

#### Token effect

Zero. Titles, bodies, payloads, timestamps, and dismissal state never enter a model request.

#### KV Cache effect

Independent. Publishing, reading, dismissing, or deleting a notification does not touch any model request prefix and cannot invalidate an otherwise reusable provider cache entry.

## Known Limitations and Deferred Work

- **No UI consumer yet** — the seam exposes records, mutations, and events only; rendering, badge counts, and dismissal flows belong to separately owned Host or Client surfaces.
- **Single-process writer** — mutations are serialized per service instance through atomic whole-document replaces with no cross-process lock, so two hosts sharing one harness home can last-write-win over each other's changes.
- **Unbounded retention** — the store keeps every entry until something deletes it; a quota or time-to-live policy waits for a concrete consumer to define one.
- **Whole-document rewrite** — every mutation rewrites all rows; very large stores pay linear write cost per mutation because the format has no incremental journal.
