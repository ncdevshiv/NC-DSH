//! Child Window realm state which cannot be expressed by the shared Window
//! interface metadata.
//!
//! Public WebIDL shape does not belong here. A real child realm gets the same
//! global template as the top-level Window; these modules only bind backing
//! ownership and the one remaining V8 WebAssembly realm adapter.

mod environment;
mod indexed_db;
mod webassembly_realm;

pub(super) use environment::{
    initialize_child_window_realm_environment, rebind_child_window_document_environment,
};
pub(super) use indexed_db::bind_materialized_child_window_indexed_db_factory;

pub(crate) const CALLBACK_ERROR_WINDOW_HANDLE_SLOT: &str = "__moliCallbackErrorWindowHandle";
