use super::*;
use crate::webidl;
use moli_canvas::{DEFAULT_FILL_STYLE, DEFAULT_FONT};
use moli_webapi_declare::WebApiObject;

#[derive(WebApiObject)]
#[webapi(interface = "CanvasRenderingContext2D")]
struct CanvasLikeContextObjectDeclaration {
    #[webapi(
        slot = CANVAS_CONTEXT_FILL_STYLE_SLOT,
        constructor_default = DEFAULT_FILL_STYLE
    )]
    fill_style: &'static str,
    #[webapi(slot = CANVAS_CONTEXT_FONT_SLOT, constructor_default = DEFAULT_FONT)]
    font: &'static str,
    #[webapi(
        slot = CANVAS_CONTEXT_IMAGE_SMOOTHING_ENABLED_SLOT,
        constructor_default = true
    )]
    image_smoothing_enabled: bool,
    #[webapi(
        slot = CANVAS_CONTEXT_IMAGE_SMOOTHING_QUALITY_SLOT,
        constructor_default = "low"
    )]
    image_smoothing_quality: &'static str,
    #[webapi(
        slot = CANVAS_CONTEXT_GLOBAL_ALPHA_SLOT,
        constructor_default = super::DEFAULT_GLOBAL_ALPHA
    )]
    global_alpha: f64,
    #[webapi(
        slot = CANVAS_CONTEXT_GLOBAL_COMPOSITE_OPERATION_SLOT,
        constructor_default = super::DEFAULT_GLOBAL_COMPOSITE_OPERATION
    )]
    global_composite_operation: &'static str,
}

pub(super) fn canvas_unrestricted_double_arg<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: &v8::FunctionCallbackArguments<'s>,
    index: i32,
    prefix: &'static str,
) -> Option<f64> {
    webidl::argument::<webidl::UnrestrictedDouble>(
        scope,
        args,
        index,
        webidl::Context::argument(prefix, (index + 1) as usize),
    )
    .ok()
    .map(f64::from)
}

pub(super) fn canonical_canvas_fill_style(raw: &str) -> Option<String> {
    moli_canvas::canonicalize_fill_style(raw)
}

pub(super) fn init_canvas_like_context_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
) {
    CanvasLikeContextObjectDeclaration::new()
        .initialize(scope, object)
        .expect("canvas context declaration should initialize object");
}
