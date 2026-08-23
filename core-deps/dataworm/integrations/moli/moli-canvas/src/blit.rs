use crate::pixel::{copy_rgba8_rect, scale_rgba8};
use crate::types::{DrawImageBlit, Rgba8Rect, ScaleFilter, byte_len, surface_matches_len};

pub fn extract_image_data(
    pixels: &[u8],
    canvas_width: u32,
    canvas_height: u32,
    source_x: i32,
    source_y: i32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut out = vec![0; byte_len(width, height).unwrap_or(0)];
    if !surface_matches_len(pixels, canvas_width, canvas_height) {
        return out;
    }
    let Some((source_rect, dest_x, dest_y)) = clipped_source_rect_for_extract(
        source_x,
        source_y,
        width,
        height,
        canvas_width,
        canvas_height,
    ) else {
        return out;
    };
    let _ = copy_rgba8_rect(
        pixels,
        canvas_width,
        canvas_height,
        source_rect,
        &mut out,
        width,
        height,
        dest_x,
        dest_y,
    );
    out
}

fn clipped_source_rect_for_extract(
    source_x: i32,
    source_y: i32,
    width: u32,
    height: u32,
    canvas_width: u32,
    canvas_height: u32,
) -> Option<(Rgba8Rect, u32, u32)> {
    let left = source_x as i64;
    let top = source_y as i64;
    let right = left.checked_add(width as i64)?;
    let bottom = top.checked_add(height as i64)?;
    let copy_left = left.max(0).min(canvas_width as i64);
    let copy_top = top.max(0).min(canvas_height as i64);
    let copy_right = right.max(0).min(canvas_width as i64);
    let copy_bottom = bottom.max(0).min(canvas_height as i64);
    if copy_left >= copy_right || copy_top >= copy_bottom {
        return None;
    }
    let source_rect = Rgba8Rect::new(
        copy_left as u32,
        copy_top as u32,
        (copy_right - copy_left) as u32,
        (copy_bottom - copy_top) as u32,
    )?;
    Some((
        source_rect,
        (copy_left - left) as u32,
        (copy_top - top) as u32,
    ))
}

#[allow(clippy::too_many_arguments)]
pub fn blit_image_data(
    pixels: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    source: &[u8],
    source_width: u32,
    source_height: u32,
    dx: i32,
    dy: i32,
    dirty_x: i32,
    dirty_y: i32,
    dirty_width: i32,
    dirty_height: i32,
) {
    if !surface_matches_len(pixels, canvas_width, canvas_height)
        || !surface_matches_len(source, source_width, source_height)
        || dirty_width <= 0
        || dirty_height <= 0
    {
        return;
    }
    let Some((source_rect, dest_x, dest_y)) = clipped_blit_rect(
        source_width,
        source_height,
        canvas_width,
        canvas_height,
        dx,
        dy,
        dirty_x,
        dirty_y,
        dirty_width,
        dirty_height,
    ) else {
        return;
    };
    let _ = copy_rgba8_rect(
        source,
        source_width,
        source_height,
        source_rect,
        pixels,
        canvas_width,
        canvas_height,
        dest_x,
        dest_y,
    );
}

#[allow(clippy::too_many_arguments)]
fn clipped_blit_rect(
    source_width: u32,
    source_height: u32,
    dest_width: u32,
    dest_height: u32,
    dest_x: i32,
    dest_y: i32,
    dirty_x: i32,
    dirty_y: i32,
    dirty_width: i32,
    dirty_height: i32,
) -> Option<(Rgba8Rect, u32, u32)> {
    let mut source_left = (dirty_x as i64).max(0).min(source_width as i64);
    let mut source_top = (dirty_y as i64).max(0).min(source_height as i64);
    let mut source_right = (dirty_x as i64)
        .checked_add(dirty_width as i64)?
        .max(0)
        .min(source_width as i64);
    let mut source_bottom = (dirty_y as i64)
        .checked_add(dirty_height as i64)?
        .max(0)
        .min(source_height as i64);
    let mut target_x = dest_x as i64;
    let mut target_y = dest_y as i64;

    if target_x < 0 {
        let skip = -target_x;
        source_left += skip;
        target_x = 0;
    }
    if target_y < 0 {
        let skip = -target_y;
        source_top += skip;
        target_y = 0;
    }
    if target_x + (source_right - source_left) > dest_width as i64 {
        source_right -= target_x + (source_right - source_left) - dest_width as i64;
    }
    if target_y + (source_bottom - source_top) > dest_height as i64 {
        source_bottom -= target_y + (source_bottom - source_top) - dest_height as i64;
    }
    if source_left >= source_right || source_top >= source_bottom {
        return None;
    }
    let source_rect = Rgba8Rect::new(
        source_left as u32,
        source_top as u32,
        (source_right - source_left) as u32,
        (source_bottom - source_top) as u32,
    )?;
    Some((source_rect, target_x as u32, target_y as u32))
}

pub fn blit_draw_image(
    pixels: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    source: &[u8],
    source_width: u32,
    source_height: u32,
    blit: DrawImageBlit,
) {
    blit_draw_image_filtered(
        pixels,
        canvas_width,
        canvas_height,
        source,
        source_width,
        source_height,
        blit,
        ScaleFilter::Nearest,
    );
}

pub fn blit_draw_image_filtered(
    pixels: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    source: &[u8],
    source_width: u32,
    source_height: u32,
    blit: DrawImageBlit,
    filter: ScaleFilter,
) {
    if !surface_matches_len(pixels, canvas_width, canvas_height)
        || !surface_matches_len(source, source_width, source_height)
    {
        return;
    }
    let dest_left = blit.dest_x.floor() as i32;
    let dest_top = blit.dest_y.floor() as i32;
    let dest_right = (blit.dest_x + blit.dest_width).ceil() as i32;
    let dest_bottom = (blit.dest_y + blit.dest_height).ceil() as i32;
    if dest_left >= dest_right || dest_top >= dest_bottom {
        return;
    }

    let start_x = dest_left.max(0).min(canvas_width as i32);
    let start_y = dest_top.max(0).min(canvas_height as i32);
    let end_x = dest_right.max(0).min(canvas_width as i32);
    let end_y = dest_bottom.max(0).min(canvas_height as i32);
    if start_x >= end_x || start_y >= end_y {
        return;
    }

    let visible_width = (end_x - start_x) as u32;
    let visible_height = (end_y - start_y) as u32;
    let Some(scaled) = scale_rgba8(
        source,
        source_width,
        source_height,
        blit,
        start_x,
        start_y,
        visible_width,
        visible_height,
        filter,
    ) else {
        return;
    };
    let Some(source_rect) = Rgba8Rect::new(0, 0, visible_width, visible_height) else {
        return;
    };
    let _ = copy_rgba8_rect(
        &scaled,
        visible_width,
        visible_height,
        source_rect,
        pixels,
        canvas_width,
        canvas_height,
        start_x as u32,
        start_y as u32,
    );
}
