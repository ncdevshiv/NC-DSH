use super::*;

#[test]
fn document_task_queue_keeps_fifo_order_and_deduplicates() {
    let mut queue = DocumentTaskQueue::default();

    assert!(queue.is_empty());
    assert!(queue.push_unique(1));
    assert!(queue.push_unique(2));
    assert!(!queue.push_unique(1));
    assert!(!queue.is_empty());

    assert_eq!(queue.pop_front(), Some(1));
    assert_eq!(queue.pop_front(), Some(2));
    assert_eq!(queue.pop_front(), None);
    assert!(queue.is_empty());
}

#[test]
fn document_task_queue_retains_matching_tasks() {
    let mut queue = DocumentTaskQueue::default();

    assert!(queue.push_unique(1));
    assert!(queue.push_unique(2));
    assert!(queue.push_unique(3));

    assert!(queue.retain(|task| *task != 2));
    assert!(!queue.retain(|task| *task != 4));

    assert_eq!(queue.pop_front(), Some(1));
    assert_eq!(queue.pop_front(), Some(3));
    assert_eq!(queue.pop_front(), None);
}

#[test]
fn document_task_queue_accepts_non_unique_fifo_tasks() {
    let mut queue = DocumentTaskQueue::default();

    queue.push_back(1);
    queue.push_back(1);
    queue.push_back(2);

    assert_eq!(queue.pop_front(), Some(1));
    assert_eq!(queue.pop_front(), Some(1));
    assert_eq!(queue.pop_front(), Some(2));
    assert_eq!(queue.pop_front(), None);
}

#[test]
fn document_task_queue_allows_in_place_updates() {
    let mut queue = DocumentTaskQueue::default();

    queue.push_back(1);
    queue.push_back(2);
    queue.push_back(3);

    assert_eq!(queue.len(), 3);
    for value in queue.iter_mut() {
        *value *= 10;
    }

    assert_eq!(queue.pop_front(), Some(10));
    assert_eq!(queue.pop_front(), Some(20));
    assert_eq!(queue.pop_front(), Some(30));
    assert!(queue.is_empty());
}

#[test]
fn document_task_queue_supports_ordered_insert_and_drain() {
    let mut queue = DocumentTaskQueue::default();

    queue.push_back(1);
    queue.push_back(3);
    queue.insert(1, 2);

    assert_eq!(queue.drain_all().collect::<Vec<_>>(), [1, 2, 3]);
    assert!(queue.is_empty());
}

#[test]
fn document_posted_task_source_drains_posted_tasks_in_fifo_order() {
    let mut source = DocumentPostedTaskSource::default();

    source.post(1);
    source.post(2);

    assert!(
        source.is_empty_local_only(),
        "posted tasks should not become local until the source is drained"
    );
    assert_eq!(source.pop_front(), Some(1));
    assert_eq!(source.pop_front(), Some(2));
    assert_eq!(source.pop_front(), None);
}

#[test]
fn document_realm_task_carries_owner_realm_and_payload() {
    let task = DocumentRealmTask::new(10_u32, 20_u32, "payload");

    assert_eq!(task.owner(), 10);
    assert_eq!(task.realm_id(), 20);
    assert_eq!(task.payload(), &"payload");
    assert_eq!(task.into_parts(), (10, 20, "payload"));
}

#[test]
fn document_posted_task_source_can_update_ready_tasks() {
    let mut source = DocumentPostedTaskSource::default();

    source.post(1);
    source.post(2);
    source.post(3);
    source.update_ready_tasks(|tasks| tasks.retain(|task| *task != 2));

    assert_eq!(source.pop_front(), Some(1));
    assert_eq!(source.pop_front(), Some(3));
    assert_eq!(source.pop_front(), None);
}
