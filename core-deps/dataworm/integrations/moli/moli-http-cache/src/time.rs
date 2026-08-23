use std::{
    hash::Hasher,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use rustc_hash::FxHasher;

static UNIQUE_SUFFIX_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn stable_cache_hash(input: &str) -> u64 {
    let mut hasher = FxHasher::default();
    hasher.write(input.as_bytes());
    hasher.finish()
}

pub(crate) fn unique_suffix() -> String {
    let now = unix_now_ms_nanos();
    let counter = UNIQUE_SUFFIX_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{now}-{counter}", std::process::id())
}

pub(crate) fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn unix_now_ms_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
