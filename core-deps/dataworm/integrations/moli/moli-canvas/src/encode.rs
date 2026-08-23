use moli_web_mime::{
    MimeSniffingContext, data_url_body_and_computed_mime_type, is_image_mime_essence,
    is_svg_image_mime_essence,
};

use crate::types::byte_len;

pub fn encode_data_url(bytes: &[u8], width: u32, height: u32) -> Option<String> {
    if width == 0 || height == 0 {
        return Some("data:,".to_owned());
    }
    if byte_len(width, height)? != bytes.len() {
        return None;
    }
    let png_bytes = moli_image::encode_png_rgba8(width, height, bytes)
        .ok()?
        .bytes;
    let mut data_url = String::from("data:image/png;base64,");
    base64::Engine::encode_string(
        &base64::engine::general_purpose::STANDARD,
        png_bytes,
        &mut data_url,
    );
    Some(data_url)
}

pub fn data_image_intrinsic_dimensions(src: &str) -> Option<(u32, u32)> {
    let (bytes, computed_mime_type) =
        data_url_body_and_computed_mime_type(src, MimeSniffingContext::Image)?;
    image_intrinsic_dimensions_from_bytes(&bytes, &computed_mime_type)
}

pub fn image_intrinsic_dimensions_from_bytes(
    bytes: &[u8],
    computed_mime_type: &str,
) -> Option<(u32, u32)> {
    if !is_image_mime_essence(computed_mime_type) {
        return None;
    }
    if is_svg_image_mime_essence(computed_mime_type) {
        return moli_image::probe_svg_image(bytes)
            .ok()?
            .concrete_dimensions();
    }
    image_dimensions_from_bytes(bytes)
}

pub fn data_image_rgba8_pixels(src: &str) -> Option<(Vec<u8>, u32, u32)> {
    let (bytes, computed_mime_type) =
        data_url_body_and_computed_mime_type(src, MimeSniffingContext::Image)?;
    if !is_image_mime_essence(&computed_mime_type) || is_svg_image_mime_essence(&computed_mime_type)
    {
        return None;
    }
    let decoded = moli_image::decode_raster_image(&bytes).ok()?.image;
    Some((decoded.rgba, decoded.width, decoded.height))
}

pub fn image_dimensions_from_bytes(bytes: &[u8]) -> Option<(u32, u32)> {
    moli_image::raster_image_dimensions(bytes).ok()
}
