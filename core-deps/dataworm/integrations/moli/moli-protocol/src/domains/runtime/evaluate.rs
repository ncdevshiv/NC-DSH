use v8::inspector::{StringView, V8InspectorSession};

use crate::conn::Cmd;

pub(super) fn can_dispatch(cmd: &Cmd<'_>) -> bool {
    V8InspectorSession::can_dispatch_method(StringView::from(cmd.method.as_bytes()))
}
