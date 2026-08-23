mod descriptors;
mod types;

pub(crate) use descriptors::{node_bridge_descriptor, node_bridge_descriptors};
pub(crate) use types::{
    BridgeDescriptor, InstallGroups, RuntimeInstallGroups, SpecializedTemplateInstaller,
    WrapperKind,
};
