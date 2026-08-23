#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserWindowBounds {
    pub left: Option<i32>,
    pub top: Option<i32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub window_state: String,
}

impl Default for BrowserWindowBounds {
    fn default() -> Self {
        Self {
            left: None,
            top: None,
            width: None,
            height: None,
            window_state: "normal".to_owned(),
        }
    }
}
