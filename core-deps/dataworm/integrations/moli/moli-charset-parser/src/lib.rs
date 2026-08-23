//! Incremental HTML meta-charset prescan support.

mod prescan;

pub use prescan::{
    HTML_META_CHARSET_PRESCAN_LIMIT, HtmlMetaCharsetParser, HtmlMetaCharsetScanResult,
    sniff_html_meta_charset,
};

#[cfg(test)]
mod tests;
