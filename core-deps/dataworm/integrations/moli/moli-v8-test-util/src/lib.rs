pub fn ensure_v8() {
    ensure_v8_with_platform(|| v8::new_default_platform(0, false).make_shared());
}

pub fn ensure_v8_with_platform(create_platform: impl FnOnce() -> v8::SharedRef<v8::Platform>) {
    moli_v8_init::ensure_v8_initialized(create_platform);
}

pub fn ensure_v8_with_flags_and_platform(
    flags: Option<&'static str>,
    create_platform: impl FnOnce() -> v8::SharedRef<v8::Platform>,
) {
    moli_v8_init::ensure_v8_initialized_with_flags(flags, create_platform);
}
