use std::cell::Cell;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CrashPoint {
    WritableStagingSynced,
    WritableStagingPromoted,
    WritableContentDurable,
    ContentInstalledBeforeCatalog,
    CatalogNextDurable,
    CatalogPreviousDurable,
    CatalogCurrentDurable,
    CatalogPreviousRemoved,
    CatalogCommittedBeforeMemorySwap,
    CatalogMemorySwappedBeforeOldContentDelete,
}

thread_local! {
    static ARMED_CRASH_POINT: Cell<Option<CrashPoint>> = const { Cell::new(None) };
}

pub(crate) struct ArmedCrashPoint;

impl Drop for ArmedCrashPoint {
    fn drop(&mut self) {
        ARMED_CRASH_POINT.with(|armed| armed.set(None));
    }
}

pub(crate) fn arm(point: CrashPoint) -> ArmedCrashPoint {
    ARMED_CRASH_POINT.with(|armed| {
        assert!(
            armed.replace(Some(point)).is_none(),
            "a crash point is already armed"
        );
    });
    ArmedCrashPoint
}

pub(crate) fn crash_if_armed(point: CrashPoint, prepare_for_crash: impl FnOnce()) {
    let should_crash = ARMED_CRASH_POINT.with(|armed| {
        if armed.get() == Some(point) {
            armed.set(None);
            true
        } else {
            false
        }
    });
    if should_crash {
        prepare_for_crash();
        panic!("injected OPFS crash at {point:?}");
    }
}
