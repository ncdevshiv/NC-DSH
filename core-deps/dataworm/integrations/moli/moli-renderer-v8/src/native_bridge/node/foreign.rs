mod arg;
mod clone;
mod detach;
mod js_values;
mod pairing;

pub(super) const FOREIGN_IDENTITY_LIVE_HANDLE_SLOT: &str = "__moliForeignIdentityLiveHandle";

pub(crate) use self::arg::node_or_foreign_arg_handle_allow_detached;
pub(in crate::native_bridge) use self::arg::{
    ExistingNodeArgument, existing_node_arg, live_delegate_arg_handle,
    node_or_existing_detached_arg_handle, node_or_foreign_arg_handle,
    node_or_foreign_arg_handle_preserve_detached,
};
