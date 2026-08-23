use super::*;

impl JsContextHost {
    pub(crate) fn node_iterators_is_empty(&self) -> bool {
        self.native_bridge().node_iterators_is_empty()
    }
}
