use crate::types::{CanvasRect, surface_matches_len};

pub fn canonicalize_fill_style(raw: &str) -> Option<String> {
    let value = raw.trim().to_ascii_lowercase();
    match value.as_str() {
        "black" => Some("#000000".to_owned()),
        "red" => Some("#ff0000".to_owned()),
        "rebeccapurple" => Some("#663399".to_owned()),
        _ if value.starts_with('#') => canonical_hex_color(&value),
        _ => None,
    }
}

pub fn fill_style_rgba(style: &str) -> [u8; 4] {
    if let Some(hex) = style.strip_prefix('#')
        && hex.len() == 6
    {
        return [
            u8::from_str_radix(&hex[0..2], 16).unwrap_or(0),
            u8::from_str_radix(&hex[2..4], 16).unwrap_or(0),
            u8::from_str_radix(&hex[4..6], 16).unwrap_or(0),
            255,
        ];
    }
    if let Some(body) = style
        .strip_prefix("rgba(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let parts = body.split(',').map(|part| part.trim()).collect::<Vec<_>>();
        if parts.len() == 4 {
            let red = parts[0].parse::<u8>().unwrap_or(0);
            let green = parts[1].parse::<u8>().unwrap_or(0);
            let blue = parts[2].parse::<u8>().unwrap_or(0);
            let alpha =
                (parts[3].parse::<f64>().unwrap_or(1.0).clamp(0.0, 1.0) * 255.0).round() as u8;
            return [red, green, blue, alpha];
        }
    }
    [0, 0, 0, 255]
}

pub fn normalize_rect(x: f64, y: f64, width: f64, height: f64) -> Option<CanvasRect> {
    if !x.is_finite() || !y.is_finite() || !width.is_finite() || !height.is_finite() {
        return None;
    }
    let left = if width >= 0.0 { x } else { x + width };
    let top = if height >= 0.0 { y } else { y + height };
    let right = if width >= 0.0 { x + width } else { x };
    let bottom = if height >= 0.0 { y + height } else { y };
    Some((
        left.floor() as i32,
        top.floor() as i32,
        right.ceil() as i32,
        bottom.ceil() as i32,
    ))
}

pub fn paint_rect(
    pixels: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    rect: CanvasRect,
    rgba: [u8; 4],
) {
    if !surface_matches_len(pixels, canvas_width, canvas_height) {
        return;
    }
    let (left, top, right, bottom) = rect;
    if left >= right || top >= bottom {
        return;
    }
    let start_x = left.max(0).min(canvas_width as i32) as u32;
    let start_y = top.max(0).min(canvas_height as i32) as u32;
    let end_x = right.max(0).min(canvas_width as i32) as u32;
    let end_y = bottom.max(0).min(canvas_height as i32) as u32;
    for y in start_y..end_y {
        for x in start_x..end_x {
            let index = ((y * canvas_width + x) * 4) as usize;
            pixels[index..index + 4].copy_from_slice(&rgba);
        }
    }
}

fn canonical_hex_color(value: &str) -> Option<String> {
    let hex = value.strip_prefix('#')?;
    if !hex.chars().all(|char| char.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        3 => {
            let chars = hex.chars().collect::<Vec<_>>();
            Some(format!(
                "#{0}{0}{1}{1}{2}{2}",
                chars[0].to_ascii_lowercase(),
                chars[1].to_ascii_lowercase(),
                chars[2].to_ascii_lowercase()
            ))
        }
        6 => Some(format!("#{hex}")),
        8 => {
            let rgba = u32::from_str_radix(hex, 16).ok()?;
            let red = ((rgba >> 24) & 0xff) as u8;
            let green = ((rgba >> 16) & 0xff) as u8;
            let blue = ((rgba >> 8) & 0xff) as u8;
            let alpha = (rgba & 0xff) as u8;
            if alpha == u8::MAX {
                Some(format!("#{red:02x}{green:02x}{blue:02x}"))
            } else {
                let alpha = (alpha as f64 / 255.0 * 100.0).round() / 100.0;
                Some(format!("rgba({red}, {green}, {blue}, {alpha:.2})"))
            }
        }
        _ => None,
    }
}
