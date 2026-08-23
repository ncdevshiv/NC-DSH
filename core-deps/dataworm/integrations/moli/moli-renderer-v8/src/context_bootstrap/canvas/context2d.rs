use super::backing_store::{
    canvas_like_pixels_copy, canvas_owner_from_context, with_canvas_like_pixels_mut,
};
use super::helpers::{canonical_canvas_fill_style, canvas_unrestricted_double_arg};
use super::*;
use crate::context_bootstrap::image_data::{
    build_image_data_object, build_image_data_object_with_bytes, image_data_bytes_from_object,
    image_data_dimensions_from_object,
};
use crate::native_bridge::element::image_selected_source;
use crate::util::{get_private_value, set_private_value};
use crate::webidl;
use moli_canvas::{
    DEFAULT_FILL_STYLE, DEFAULT_FONT, DrawImageBlit, ScaleFilter, blit_draw_image_filtered,
    blit_image_data, byte_len, data_image_rgba8_pixels, draw_text, extract_image_data,
    fill_style_rgba, measure_text_width, normalize_rect as canvas_normalize_rect, paint_rect,
};
use moli_webapi_declare::WebApiObject;
use std::str::FromStr;

const DEFAULT_IMAGE_SMOOTHING_QUALITY: &str = "low";
const CANVAS_CONTEXT_LINE_DASH_SLOT: &str = "__moliCanvasContextLineDash";

#[derive(WebApiObject)]
#[webapi(interface = "Object")]
struct CanvasTextMetricsDeclaration {
    #[webapi(data_property)]
    width: f64,
}

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr, webidl::WebIdlEnum,
)]
#[webidl(name = "ImageSmoothingQuality", parse_with = Self::parse)]
#[strum(serialize_all = "lowercase")]
enum ImageSmoothingQuality {
    Low,
    Medium,
    High,
}

impl ImageSmoothingQuality {
    fn parse(value: &str) -> Option<Self> {
        Self::from_str(value).ok()
    }

    fn label(self) -> &'static str {
        self.into()
    }
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "CanvasRenderingContext2D.fillText")]
struct CanvasContextFillTextArgs {
    #[webidl(required)]
    text: String,
    #[webidl(required)]
    x: f64,
    #[webidl(required)]
    y: f64,
    #[webidl(name = "maxWidth")]
    _max_width: Option<f64>,
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "CanvasRenderingContext2D.strokeText")]
struct CanvasContextStrokeTextArgs {
    #[webidl(required)]
    text: String,
    #[webidl(required)]
    x: f64,
    #[webidl(required)]
    y: f64,
    #[webidl(name = "maxWidth")]
    _max_width: Option<f64>,
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "CanvasRenderingContext2D.measureText")]
struct CanvasContextMeasureTextArgs {
    #[webidl(required)]
    text: String,
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "CanvasGradient.addColorStop")]
struct CanvasGradientAddColorStopArgs {
    #[webidl(required)]
    offset: f64,
    #[webidl(required)]
    _color: String,
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "CanvasRenderingContext2D.createImageData")]
struct CanvasContextCreateImageDataSizeArgs {
    #[webidl(required, converter = "enforce_range_long")]
    sw: i32,
    #[webidl(required, converter = "enforce_range_long")]
    sh: i32,
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "CanvasRenderingContext2D.getImageData")]
struct CanvasContextGetImageDataArgs {
    #[webidl(required, converter = "enforce_range_long")]
    sx: i32,
    #[webidl(required, converter = "enforce_range_long")]
    sy: i32,
    #[webidl(required, converter = "enforce_range_long")]
    sw: i32,
    #[webidl(required, converter = "enforce_range_long")]
    sh: i32,
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "CanvasRenderingContext2D.putImageData")]
struct CanvasContextPutImageDataArgs<'s> {
    #[webidl(required)]
    image_data: v8::Local<'s, v8::Object>,
    #[webidl(required, converter = "enforce_range_long")]
    dx: i32,
    #[webidl(required, converter = "enforce_range_long")]
    dy: i32,
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "CanvasRenderingContext2D.putImageData")]
struct CanvasContextPutImageDataDirtyArgs<'s> {
    #[webidl(required)]
    image_data: v8::Local<'s, v8::Object>,
    #[webidl(required, converter = "enforce_range_long")]
    dx: i32,
    #[webidl(required, converter = "enforce_range_long")]
    dy: i32,
    #[webidl(required, converter = "enforce_range_long", name = "dirtyX")]
    dirty_x: i32,
    #[webidl(required, converter = "enforce_range_long", name = "dirtyY")]
    dirty_y: i32,
    #[webidl(required, converter = "enforce_range_long", name = "dirtyWidth")]
    dirty_width: i32,
    #[webidl(required, converter = "enforce_range_long", name = "dirtyHeight")]
    dirty_height: i32,
}

fn require_canvas_context_receiver<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    receiver: v8::Local<'s, v8::Object>,
    member: &str,
) -> bool {
    if get_private_value(scope, receiver, CANVAS_CONTEXT_FILL_STYLE_SLOT).is_some() {
        return true;
    }
    throw_type_error(
        scope,
        &format!("CanvasRenderingContext2D.{member} called on incompatible receiver."),
    );
    false
}

pub(crate) fn canvas_context_fill_style_getter_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_canvas_context_receiver(scope, args.this(), "fillStyle getter") {
        return;
    }
    let value = context_string_slot(scope, args.this(), CANVAS_CONTEXT_FILL_STYLE_SLOT)
        .unwrap_or_else(|| "#000000".to_owned());
    if let Some(value) = v8_string(scope, &value) {
        rv.set(value.into());
    } else {
        rv.set(v8::String::empty(scope).into());
    }
}

pub(crate) fn canvas_context_fill_style_setter_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_canvas_context_receiver(scope, args.this(), "fillStyle setter") {
        return;
    }
    let Some(raw) = canvas_context_dom_string_value(
        scope,
        args.get(0),
        "CanvasRenderingContext2D",
        "fillStyle",
    ) else {
        rv.set_undefined();
        return;
    };
    let Some(canonical) = canonical_canvas_fill_style(&raw) else {
        rv.set_undefined();
        return;
    };
    set_context_string_slot(
        scope,
        args.this(),
        CANVAS_CONTEXT_FILL_STYLE_SLOT,
        &canonical,
    );
    rv.set_undefined();
}

pub(crate) fn canvas_context_font_getter_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_canvas_context_receiver(scope, args.this(), "font getter") {
        return;
    }
    let value = context_string_slot(scope, args.this(), CANVAS_CONTEXT_FONT_SLOT)
        .unwrap_or_else(|| "10px sans-serif".to_owned());
    if let Some(value) = v8_string(scope, &value) {
        rv.set(value.into());
    } else {
        rv.set(v8::String::empty(scope).into());
    }
}

pub(crate) fn canvas_context_font_setter_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_canvas_context_receiver(scope, args.this(), "font setter") {
        return;
    }
    let Some(font) =
        canvas_context_dom_string_value(scope, args.get(0), "CanvasRenderingContext2D", "font")
    else {
        rv.set_undefined();
        return;
    };
    set_context_string_slot(scope, args.this(), CANVAS_CONTEXT_FONT_SLOT, &font);
    rv.set_undefined();
}

pub(crate) fn canvas_context_image_smoothing_enabled_getter_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_canvas_context_receiver(scope, args.this(), "imageSmoothingEnabled getter") {
        return;
    }
    let value = context_bool_slot(
        scope,
        args.this(),
        CANVAS_CONTEXT_IMAGE_SMOOTHING_ENABLED_SLOT,
    )
    .unwrap_or(true);
    rv.set(v8::Boolean::new(scope, value).into());
}

pub(crate) fn canvas_context_image_smoothing_enabled_setter_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_canvas_context_receiver(scope, args.this(), "imageSmoothingEnabled setter") {
        return;
    }
    let Some(value) = canvas_context_boolean_value(
        scope,
        args.get(0),
        "CanvasRenderingContext2D",
        "imageSmoothingEnabled",
    ) else {
        rv.set_undefined();
        return;
    };
    set_context_bool_slot(
        scope,
        args.this(),
        CANVAS_CONTEXT_IMAGE_SMOOTHING_ENABLED_SLOT,
        value,
    );
    rv.set_undefined();
}

pub(crate) fn canvas_context_image_smoothing_quality_getter_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_canvas_context_receiver(scope, args.this(), "imageSmoothingQuality getter") {
        return;
    }
    let value = context_string_slot(
        scope,
        args.this(),
        CANVAS_CONTEXT_IMAGE_SMOOTHING_QUALITY_SLOT,
    )
    .unwrap_or_else(|| DEFAULT_IMAGE_SMOOTHING_QUALITY.to_owned());
    rv.set(
        v8_string(scope, &value)
            .unwrap_or_else(|| v8::String::empty(scope))
            .into(),
    );
}

pub(crate) fn canvas_context_image_smoothing_quality_setter_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_canvas_context_receiver(scope, args.this(), "imageSmoothingQuality setter") {
        return;
    }
    let Some(quality) = canvas_context_image_smoothing_quality_value(scope, args.get(0)) else {
        rv.set_undefined();
        return;
    };
    set_context_string_slot(
        scope,
        args.this(),
        CANVAS_CONTEXT_IMAGE_SMOOTHING_QUALITY_SLOT,
        quality.label(),
    );
    rv.set_undefined();
}

fn canvas_context_dom_string_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    owner: &'static str,
    property: &'static str,
) -> Option<String> {
    match webidl::convert::<webidl::DomString>(
        scope,
        value,
        webidl::Context::member(owner, property),
    ) {
        Ok(value) => Some(value.0),
        Err(error) => {
            webidl::throw_error(scope, &error);
            None
        }
    }
}

fn canvas_required_unrestricted_double_arg<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: &v8::FunctionCallbackArguments<'s>,
    index: i32,
    prefix: &'static str,
) -> Option<f64> {
    match webidl::argument::<webidl::UnrestrictedDouble>(
        scope,
        args,
        index,
        webidl::Context::argument(prefix, (index + 1) as usize),
    ) {
        Ok(value) => Some(f64::from(value)),
        Err(error) => {
            webidl::throw_error(scope, &error);
            None
        }
    }
}

fn canvas_context_boolean_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    owner: &'static str,
    property: &'static str,
) -> Option<bool> {
    match webidl::convert::<webidl::Boolean>(scope, value, webidl::Context::member(owner, property))
    {
        Ok(value) => Some(value.0),
        Err(error) => {
            webidl::throw_error(scope, &error);
            None
        }
    }
}

fn canvas_context_image_smoothing_quality_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
) -> Option<ImageSmoothingQuality> {
    match webidl::convert::<webidl::EnumValue<ImageSmoothingQuality>>(
        scope,
        value,
        webidl::Context::member("CanvasRenderingContext2D", "imageSmoothingQuality"),
    ) {
        Ok(value) => Some(value.0),
        Err(error) => {
            webidl::throw_error(scope, &error);
            None
        }
    }
}

pub(crate) fn canvas_context_global_alpha_getter_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_canvas_context_receiver(scope, args.this(), "globalAlpha getter") {
        return;
    }
    let value = context_number_slot(scope, args.this(), CANVAS_CONTEXT_GLOBAL_ALPHA_SLOT)
        .unwrap_or(DEFAULT_GLOBAL_ALPHA);
    rv.set(v8::Number::new(scope, value).into());
}

pub(crate) fn canvas_context_global_alpha_setter_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_canvas_context_receiver(scope, args.this(), "globalAlpha setter") {
        return;
    }
    canvas_context_global_alpha_assign(scope, args.this(), args.get(0));
    rv.set_undefined();
}

fn canvas_context_global_alpha_assign<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    holder: v8::Local<'s, v8::Object>,
    value: v8::Local<'s, v8::Value>,
) {
    // Per HTML spec: globalAlpha is an `unrestricted double`. The WebIDL
    // conversion itself must throw TypeError on values that aren't
    // convertible (e.g. Symbol) — only AFTER coercion succeeds do we
    // silently ignore non-finite / out-of-range numbers per the canvas
    // setter semantics.
    let unrestricted = match webidl::convert::<webidl::UnrestrictedDouble>(
        scope,
        value,
        webidl::Context::member("CanvasRenderingContext2D", "globalAlpha"),
    ) {
        Ok(value) => value,
        Err(error) => {
            webidl::throw_error(scope, &error);
            return;
        }
    };
    let raw = f64::from(unrestricted);
    if !raw.is_finite() || !(0.0..=1.0).contains(&raw) {
        return;
    }
    set_context_number_slot(scope, holder, CANVAS_CONTEXT_GLOBAL_ALPHA_SLOT, raw);
}

pub(crate) fn canvas_context_global_composite_operation_getter_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_canvas_context_receiver(scope, args.this(), "globalCompositeOperation getter") {
        return;
    }
    let value = context_string_slot(
        scope,
        args.this(),
        CANVAS_CONTEXT_GLOBAL_COMPOSITE_OPERATION_SLOT,
    )
    .unwrap_or_else(|| DEFAULT_GLOBAL_COMPOSITE_OPERATION.to_owned());
    rv.set(
        v8_string(scope, &value)
            .unwrap_or_else(|| v8::String::empty(scope))
            .into(),
    );
}

pub(crate) fn canvas_context_global_composite_operation_setter_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !require_canvas_context_receiver(scope, args.this(), "globalCompositeOperation setter") {
        return;
    }
    canvas_context_global_composite_operation_assign(scope, args.this(), args.get(0));
    rv.set_undefined();
}

fn canvas_context_global_composite_operation_assign<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    holder: v8::Local<'s, v8::Object>,
    value: v8::Local<'s, v8::Value>,
) {
    // Per HTML spec: setting an unrecognised value for globalCompositeOperation
    // (legacy aliases like `darker`/`clear`/`highlight`, capitalised variants,
    // unknown strings) must leave the property unchanged. Only the canonical
    // lowercase tokens listed in VALID_GLOBAL_COMPOSITE_OPERATIONS are accepted.
    let Some(string) = canvas_context_dom_string_value(
        scope,
        value,
        "CanvasRenderingContext2D",
        "globalCompositeOperation",
    ) else {
        return;
    };
    let Some(canonical) = canvas_composite_operation_canonical(&string) else {
        return;
    };
    set_context_string_slot(
        scope,
        holder,
        CANVAS_CONTEXT_GLOBAL_COMPOSITE_OPERATION_SLOT,
        canonical,
    );
}

fn context_string_slot<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
    slot: &'static str,
) -> Option<String> {
    get_private_value(scope, object, slot)
        .and_then(|value| value.to_string(scope))
        .map(|value| value.to_rust_string_lossy(scope))
}

fn context_bool_slot<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
    slot: &'static str,
) -> Option<bool> {
    get_private_value(scope, object, slot).map(|value| value.boolean_value(scope))
}

fn context_number_slot<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
    slot: &'static str,
) -> Option<f64> {
    get_private_value(scope, object, slot).and_then(|value| value.number_value(scope))
}

fn set_context_string_slot(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    slot: &'static str,
    value: &str,
) {
    if let Some(value) = v8_string(scope, value) {
        set_private_value(scope, object, slot, value.into());
    }
}

fn set_context_bool_slot(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    slot: &'static str,
    value: bool,
) {
    let value = v8::Boolean::new(scope, value);
    set_private_value(scope, object, slot, value.into());
}

fn set_context_number_slot(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    slot: &'static str,
    value: f64,
) {
    let value = v8::Number::new(scope, value);
    set_private_value(scope, object, slot, value.into());
}

pub(crate) fn canvas_context_fill_rect_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(canvas) = canvas_owner_from_context(scope, args.this()) else {
        return;
    };
    let Some(rect) = normalized_rect(scope, &args, "CanvasRenderingContext2D.fillRect") else {
        return;
    };
    let fill_style = context_string_slot(scope, args.this(), CANVAS_CONTEXT_FILL_STYLE_SLOT)
        .unwrap_or_else(|| DEFAULT_FILL_STYLE.to_owned());
    let color = fill_style_rgba(&fill_style);
    let _ = with_canvas_like_pixels_mut(scope, canvas, |pixels, width, height| {
        paint_rect(pixels, width, height, rect, color);
    });
}

pub(crate) fn canvas_context_clear_rect_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(canvas) = canvas_owner_from_context(scope, args.this()) else {
        return;
    };
    let Some(rect) = normalized_rect(scope, &args, "CanvasRenderingContext2D.clearRect") else {
        return;
    };
    let _ = with_canvas_like_pixels_mut(scope, canvas, |pixels, width, height| {
        paint_rect(pixels, width, height, rect, [0, 0, 0, 0]);
    });
}

pub(crate) fn canvas_context_rect_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let _ = normalized_rect(scope, &args, "CanvasRenderingContext2D.rect");
}

pub(crate) fn canvas_context_is_point_in_path_callback(
    scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    rv.set(v8::Boolean::new(scope, false).into());
}

pub(crate) fn canvas_context_fill_text_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(canvas) = canvas_owner_from_context(scope, args.this()) else {
        return;
    };
    let Some(parsed) = webidl::parse_args::<CanvasContextFillTextArgs>(scope, &args) else {
        return;
    };
    draw_canvas_context_text(scope, args.this(), canvas, &parsed.text, parsed.x, parsed.y);
}

pub(crate) fn canvas_context_stroke_text_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(canvas) = canvas_owner_from_context(scope, args.this()) else {
        return;
    };
    let Some(parsed) = webidl::parse_args::<CanvasContextStrokeTextArgs>(scope, &args) else {
        return;
    };
    draw_canvas_context_text(scope, args.this(), canvas, &parsed.text, parsed.x, parsed.y);
}

fn draw_canvas_context_text<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    context: v8::Local<'s, v8::Object>,
    canvas: v8::Local<'s, v8::Object>,
    text: &str,
    x: f64,
    y: f64,
) {
    let fill_style = context_string_slot(scope, context, CANVAS_CONTEXT_FILL_STYLE_SLOT)
        .unwrap_or_else(|| DEFAULT_FILL_STYLE.to_owned());
    let font = context_string_slot(scope, context, CANVAS_CONTEXT_FONT_SLOT)
        .unwrap_or_else(|| DEFAULT_FONT.to_owned());
    let color = fill_style_rgba(&fill_style);
    let _ = with_canvas_like_pixels_mut(scope, canvas, |pixels, width, height| {
        draw_text(pixels, width, height, text, x, y, &font, color);
    });
}

pub(crate) fn canvas_context_draw_image_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(canvas) = canvas_owner_from_context(scope, args.this()) else {
        return;
    };
    let Ok(source) = v8::Local::<v8::Object>::try_from(args.get(0)) else {
        return;
    };
    let Some((source_pixels, source_width, source_height)) =
        html_image_pixels_copy(scope, source).or_else(|| canvas_like_pixels_copy(scope, source))
    else {
        return;
    };
    let Some(blit) =
        normalized_draw_image_args(scope, &args, source_width as f64, source_height as f64)
    else {
        return;
    };

    let filter = if context_bool_slot(
        scope,
        args.this(),
        CANVAS_CONTEXT_IMAGE_SMOOTHING_ENABLED_SLOT,
    )
    .unwrap_or(true)
    {
        ScaleFilter::Bilinear
    } else {
        ScaleFilter::Nearest
    };
    let _ = with_canvas_like_pixels_mut(scope, canvas, |pixels, width, height| {
        blit_draw_image_filtered(
            pixels,
            width,
            height,
            &source_pixels,
            source_width,
            source_height,
            blit,
            filter,
        );
    });
}

fn html_image_pixels_copy<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    source: v8::Local<'s, v8::Object>,
) -> Option<(Vec<u8>, u32, u32)> {
    let src = native_html_image_data_src(scope, source)?;
    data_image_rgba8_pixels(&src)
}

fn native_html_image_data_src<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    source: v8::Local<'s, v8::Object>,
) -> Option<String> {
    let (runtime_ptr, handle) =
        crate::native_bridge::node_runtime_and_handle_from_object(scope, source).ok()?;
    let runtime = unsafe { &*runtime_ptr };
    image_selected_source(runtime, handle)
}

pub(crate) fn canvas_context_noop_callback(
    _scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments<'_>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
}

pub(crate) fn canvas_context_set_line_dash_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    if canvas_owner_from_context(scope, args.this()).is_none() {
        throw_type_error(
            scope,
            "CanvasRenderingContext2D.setLineDash: Illegal invocation",
        );
        return;
    }
    let sequence = match webidl::argument::<webidl::Sequence<webidl::UnrestrictedDouble>>(
        scope,
        &args,
        0,
        webidl::Context::argument("CanvasRenderingContext2D.setLineDash", 1),
    ) {
        Ok(sequence) => sequence,
        Err(error) => {
            webidl::throw_error(scope, &error);
            return;
        }
    };
    let mut segments = sequence
        .0
        .into_iter()
        .map(|segment| segment.0)
        .collect::<Vec<_>>();
    if segments
        .iter()
        .any(|segment| !segment.is_finite() || *segment < 0.0)
    {
        return;
    }
    if segments.len() % 2 == 1 {
        let repeated = segments.clone();
        segments.extend(repeated);
    }
    let values = segments
        .into_iter()
        .map(|segment| v8::Number::new(scope, segment).into())
        .collect::<Vec<_>>();
    let line_dash = v8::Array::new_with_elements(scope, &values);
    set_private_value(
        scope,
        args.this(),
        CANVAS_CONTEXT_LINE_DASH_SLOT,
        line_dash.into(),
    );
}

pub(crate) fn canvas_context_get_line_dash_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if canvas_owner_from_context(scope, args.this()).is_none() {
        throw_type_error(
            scope,
            "CanvasRenderingContext2D.getLineDash: Illegal invocation",
        );
        return;
    }
    let Some(line_dash) = get_private_value(scope, args.this(), CANVAS_CONTEXT_LINE_DASH_SLOT)
        .and_then(|value| v8::Local::<v8::Array>::try_from(value).ok())
    else {
        rv.set(v8::Array::new(scope, 0).into());
        return;
    };
    let values = (0..line_dash.length())
        .filter_map(|index| line_dash.get_index(scope, index))
        .collect::<Vec<_>>();
    rv.set(v8::Array::new_with_elements(scope, &values).into());
}

pub(crate) fn canvas_context_create_linear_gradient_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let prefix = "CanvasRenderingContext2D.createLinearGradient";
    let Some(x0) = canvas_required_unrestricted_double_arg(scope, &args, 0, prefix) else {
        return;
    };
    let Some(y0) = canvas_required_unrestricted_double_arg(scope, &args, 1, prefix) else {
        return;
    };
    let Some(x1) = canvas_required_unrestricted_double_arg(scope, &args, 2, prefix) else {
        return;
    };
    let Some(y1) = canvas_required_unrestricted_double_arg(scope, &args, 3, prefix) else {
        return;
    };
    if !x0.is_finite() || !y0.is_finite() || !x1.is_finite() || !y1.is_finite() {
        webidl::throw_dom_exception(
            scope,
            "NotSupportedError",
            "Canvas gradient coordinates must be finite.",
        );
        return;
    }

    let gradient = v8::Object::new(scope);
    if let Some(prototype) = global_constructor_prototype(scope, "CanvasGradient") {
        let _ = gradient.set_prototype(scope, prototype.into());
    }
    rv.set(gradient.into());
}

pub(crate) fn canvas_gradient_add_color_stop_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parsed) = webidl::parse_args::<CanvasGradientAddColorStopArgs>(scope, &args) else {
        return;
    };
    if !(0.0..=1.0).contains(&parsed.offset) {
        webidl::throw_index_size_error(scope);
        return;
    }
    rv.set_undefined();
}

pub(crate) fn canvas_context_measure_text_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parsed) = webidl::parse_args::<CanvasContextMeasureTextArgs>(scope, &args) else {
        return;
    };
    let font = context_string_slot(scope, args.this(), CANVAS_CONTEXT_FONT_SLOT)
        .unwrap_or_else(|| DEFAULT_FONT.to_owned());
    let declaration = CanvasTextMetricsDeclaration {
        width: measure_text_width(&parsed.text, &font),
    };
    let Ok(metrics) = declaration.bind(scope) else {
        return;
    };
    rv.set(metrics.into());
}

pub(crate) fn canvas_context_create_image_data_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let dimensions = if let Ok(object) = v8::Local::<v8::Object>::try_from(args.get(0)) {
        image_data_dimensions_from_object(scope, object)
            .or_else(|| image_data_width_height_from_webidl_args(scope, &args))
    } else {
        image_data_width_height_from_webidl_args(scope, &args)
    };
    let Some((width, height)) = dimensions else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    if let Some(image_data) = build_image_data_object(scope, width, height) {
        rv.set(image_data.into());
    } else {
        rv.set(v8::undefined(scope).into());
    }
}

pub(crate) fn canvas_context_put_image_data_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(canvas) = canvas_owner_from_context(scope, args.this()) else {
        return;
    };
    let (image_data, dx, dy, dirty_rect) = if args.length() >= 7 {
        let Some(parsed) = webidl::parse_args::<CanvasContextPutImageDataDirtyArgs>(scope, &args)
        else {
            return;
        };
        let Some(dirty_rect) = normalized_image_data_dirty_rect(
            scope,
            parsed.dirty_x,
            parsed.dirty_y,
            parsed.dirty_width,
            parsed.dirty_height,
        ) else {
            return;
        };
        (parsed.image_data, parsed.dx, parsed.dy, Some(dirty_rect))
    } else {
        let Some(parsed) = webidl::parse_args::<CanvasContextPutImageDataArgs>(scope, &args) else {
            return;
        };
        (parsed.image_data, parsed.dx, parsed.dy, None)
    };
    let Some((source_width, source_height)) = image_data_dimensions_from_object(scope, image_data)
    else {
        throw_type_error(
            scope,
            "CanvasRenderingContext2D.putImageData: argument 1 is not an ImageData object.",
        );
        return;
    };
    let Some(bytes) = image_data_bytes_from_object(scope, image_data) else {
        return;
    };

    let (dirty_x, dirty_y, dirty_width, dirty_height) = if args.length() >= 7 {
        dirty_rect.unwrap_or((0, 0, 0, 0))
    } else {
        (0, 0, source_width as i32, source_height as i32)
    };

    let _ = with_canvas_like_pixels_mut(scope, canvas, |pixels, width, height| {
        blit_image_data(
            pixels,
            width,
            height,
            &bytes,
            source_width,
            source_height,
            dx,
            dy,
            dirty_x,
            dirty_y,
            dirty_width,
            dirty_height,
        );
    });
}

pub(crate) fn canvas_context_get_image_data_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parsed) = webidl::parse_args::<CanvasContextGetImageDataArgs>(scope, &args) else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    let Some(width) = positive_image_data_dimension_from_long(scope, parsed.sw) else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    let Some(height) = positive_image_data_dimension_from_long(scope, parsed.sh) else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    if byte_len(width, height).is_none() {
        webidl::throw_index_size_error(scope);
        rv.set(v8::undefined(scope).into());
        return;
    }
    let source_x = parsed.sx;
    let source_y = parsed.sy;
    let bytes = if let Some(canvas) = canvas_owner_from_context(scope, args.this()) {
        canvas_like_pixels_copy(scope, canvas)
            .map(|(pixels, canvas_width, canvas_height)| {
                extract_image_data(
                    &pixels,
                    canvas_width,
                    canvas_height,
                    source_x,
                    source_y,
                    width,
                    height,
                )
            })
            .unwrap_or_else(|| blank_image_data(width, height))
    } else {
        blank_image_data(width, height)
    };
    let Some(image_data) = build_image_data_object_with_bytes(scope, width, height, bytes)
        .or_else(|| build_image_data_object(scope, width, height))
    else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    rv.set(image_data.into());
}

fn image_data_width_height_from_webidl_args<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: &v8::FunctionCallbackArguments<'s>,
) -> Option<(u32, u32)> {
    let parsed = webidl::parse_args::<CanvasContextCreateImageDataSizeArgs>(scope, args)?;
    let width = image_data_dimension_from_long(scope, parsed.sw)?;
    let height = image_data_dimension_from_long(scope, parsed.sh)?;
    if byte_len(width, height).is_none() {
        webidl::throw_index_size_error(scope);
        return None;
    }
    Some((width, height))
}

fn image_data_dimension_from_long(scope: &mut v8::PinScope<'_, '_>, value: i32) -> Option<u32> {
    if value == 0 || value == i32::MIN {
        webidl::throw_index_size_error(scope);
        return None;
    }
    Some(value.unsigned_abs())
}

fn positive_image_data_dimension_from_long(
    scope: &mut v8::PinScope<'_, '_>,
    value: i32,
) -> Option<u32> {
    if value <= 0 {
        webidl::throw_index_size_error(scope);
        return None;
    }
    Some(value as u32)
}

fn normalized_image_data_dirty_rect(
    scope: &mut v8::PinScope<'_, '_>,
    dirty_x: i32,
    dirty_y: i32,
    dirty_width: i32,
    dirty_height: i32,
) -> Option<(i32, i32, i32, i32)> {
    let (dirty_x, dirty_width) = normalized_dirty_axis(scope, dirty_x, dirty_width)?;
    let (dirty_y, dirty_height) = normalized_dirty_axis(scope, dirty_y, dirty_height)?;
    Some((dirty_x, dirty_y, dirty_width, dirty_height))
}

fn normalized_dirty_axis(
    scope: &mut v8::PinScope<'_, '_>,
    start: i32,
    span: i32,
) -> Option<(i32, i32)> {
    if span >= 0 {
        return Some((start, span));
    }
    if span == i32::MIN {
        webidl::throw_index_size_error(scope);
        return None;
    }
    let start = start.checked_add(span).or_else(|| {
        webidl::throw_index_size_error(scope);
        None
    })?;
    Some((start, -span))
}

fn normalized_rect<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: &v8::FunctionCallbackArguments<'s>,
    prefix: &'static str,
) -> Option<(i32, i32, i32, i32)> {
    canvas_normalize_rect(
        canvas_unrestricted_double_arg(scope, args, 0, prefix)?,
        canvas_unrestricted_double_arg(scope, args, 1, prefix)?,
        canvas_unrestricted_double_arg(scope, args, 2, prefix)?,
        canvas_unrestricted_double_arg(scope, args, 3, prefix)?,
    )
}

fn normalized_draw_image_args<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: &v8::FunctionCallbackArguments<'s>,
    source_width: f64,
    source_height: f64,
) -> Option<DrawImageBlit> {
    if args.length() >= 9 {
        return DrawImageBlit::new(
            canvas_unrestricted_double_arg(scope, args, 1, "CanvasRenderingContext2D.drawImage")?,
            canvas_unrestricted_double_arg(scope, args, 2, "CanvasRenderingContext2D.drawImage")?,
            canvas_unrestricted_double_arg(scope, args, 3, "CanvasRenderingContext2D.drawImage")?,
            canvas_unrestricted_double_arg(scope, args, 4, "CanvasRenderingContext2D.drawImage")?,
            canvas_unrestricted_double_arg(scope, args, 5, "CanvasRenderingContext2D.drawImage")?,
            canvas_unrestricted_double_arg(scope, args, 6, "CanvasRenderingContext2D.drawImage")?,
            canvas_unrestricted_double_arg(scope, args, 7, "CanvasRenderingContext2D.drawImage")?,
            canvas_unrestricted_double_arg(scope, args, 8, "CanvasRenderingContext2D.drawImage")?,
        );
    }
    if args.length() >= 5 {
        return DrawImageBlit::new(
            0.0,
            0.0,
            source_width,
            source_height,
            canvas_unrestricted_double_arg(scope, args, 1, "CanvasRenderingContext2D.drawImage")?,
            canvas_unrestricted_double_arg(scope, args, 2, "CanvasRenderingContext2D.drawImage")?,
            canvas_unrestricted_double_arg(scope, args, 3, "CanvasRenderingContext2D.drawImage")?,
            canvas_unrestricted_double_arg(scope, args, 4, "CanvasRenderingContext2D.drawImage")?,
        );
    }
    if args.length() >= 3 {
        return DrawImageBlit::new(
            0.0,
            0.0,
            source_width,
            source_height,
            canvas_unrestricted_double_arg(scope, args, 1, "CanvasRenderingContext2D.drawImage")?,
            canvas_unrestricted_double_arg(scope, args, 2, "CanvasRenderingContext2D.drawImage")?,
            source_width,
            source_height,
        );
    }
    None
}

fn blank_image_data(width: u32, height: u32) -> Vec<u8> {
    vec![0; byte_len(width, height).unwrap_or(0)]
}
