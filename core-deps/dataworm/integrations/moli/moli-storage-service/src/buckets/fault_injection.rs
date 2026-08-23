use std::cell::Cell;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CrashPoint {
    CacheNextDurable,
    CachePreviousDurable,
    CacheCurrentDurable,
    CachePreviousRemovedBeforeSync,
    BucketTombstoneDurable,
    BucketCleanupComplete,
    BucketTombstoneRemovedDurable,
}

thread_local! {
    static ARMED_CRASH_POINT: Cell<Option<CrashPoint>> = const { Cell::new(None) };
}

pub(super) struct ArmedCrashPoint;

impl Drop for ArmedCrashPoint {
    fn drop(&mut self) {
        ARMED_CRASH_POINT.with(|armed| armed.set(None));
    }
}

pub(super) fn arm(point: CrashPoint) -> ArmedCrashPoint {
    ARMED_CRASH_POINT.with(|armed| {
        assert!(
            armed.replace(Some(point)).is_none(),
            "a StorageBucket crash point is already armed"
        );
    });
    ArmedCrashPoint
}

pub(super) fn crash_if_armed(point: CrashPoint) {
    let should_crash = ARMED_CRASH_POINT.with(|armed| {
        if armed.get() == Some(point) {
            armed.set(None);
            true
        } else {
            false
        }
    });
    if should_crash {
        panic!("injected StorageBucket crash at {point:?}");
    }
}
