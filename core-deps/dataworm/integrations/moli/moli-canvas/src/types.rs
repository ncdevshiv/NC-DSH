pub const DEFAULT_FILL_STYLE: &str = "#000000";
pub const DEFAULT_FONT: &str = "10px sans-serif";
pub const MAX_RGBA8_BYTE_LENGTH: usize = 2_147_483_647;

pub type CanvasRect = (i32, i32, i32, i32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgba8Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rgba8Rect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        Some(Self {
            x,
            y,
            width,
            height,
        })
    }

    pub(crate) fn fits_within(self, width: u32, height: u32) -> bool {
        self.x
            .checked_add(self.width)
            .is_some_and(|right| right <= width)
            && self
                .y
                .checked_add(self.height)
                .is_some_and(|bottom| bottom <= height)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleFilter {
    Nearest,
    Bilinear,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawImageBlit {
    pub source_x: f64,
    pub source_y: f64,
    pub source_width: f64,
    pub source_height: f64,
    pub dest_x: f64,
    pub dest_y: f64,
    pub dest_width: f64,
    pub dest_height: f64,
}

impl DrawImageBlit {
    pub fn new(
        source_x: f64,
        source_y: f64,
        source_width: f64,
        source_height: f64,
        dest_x: f64,
        dest_y: f64,
        dest_width: f64,
        dest_height: f64,
    ) -> Option<Self> {
        let blit = Self {
            source_x,
            source_y,
            source_width,
            source_height,
            dest_x,
            dest_y,
            dest_width,
            dest_height,
        };
        if !blit.source_x.is_finite()
            || !blit.source_y.is_finite()
            || !blit.source_width.is_finite()
            || !blit.source_height.is_finite()
            || !blit.dest_x.is_finite()
            || !blit.dest_y.is_finite()
            || !blit.dest_width.is_finite()
            || !blit.dest_height.is_finite()
            || blit.source_width <= 0.0
            || blit.source_height <= 0.0
            || blit.dest_width <= 0.0
            || blit.dest_height <= 0.0
        {
            return None;
        }
        Some(blit)
    }
}

pub fn byte_len(width: u32, height: u32) -> Option<usize> {
    let len = 4usize
        .checked_mul(width as usize)?
        .checked_mul(height as usize)?;
    (len <= MAX_RGBA8_BYTE_LENGTH).then_some(len)
}

pub(crate) fn surface_matches_len(pixels: &[u8], width: u32, height: u32) -> bool {
    byte_len(width, height) == Some(pixels.len())
}
