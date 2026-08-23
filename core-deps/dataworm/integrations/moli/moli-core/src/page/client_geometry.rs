#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClientRect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub width: f64,
    pub height: f64,
}

impl From<crate::renderer::RendererClientRect> for ClientRect {
    fn from(value: crate::renderer::RendererClientRect) -> Self {
        Self {
            left: value.left,
            top: value.top,
            right: value.right,
            bottom: value.bottom,
            width: value.width,
            height: value.height,
        }
    }
}
