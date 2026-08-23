#[cfg(test)]
mod diagnostics;
mod kind;
mod lifecycle;
mod materialize;

pub(crate) use kind::WindowLazySurface;
pub(super) use lifecycle::ensure_window_lazy_surface_value;
pub(crate) use lifecycle::{
    ensure_window_lazy_surface_object, rematerialize_window_lazy_surface_if_cached,
};

#[cfg(test)]
pub(crate) use diagnostics::window_lazy_surface_diagnostics;
