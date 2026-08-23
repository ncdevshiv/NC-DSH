// SPDX-License-Identifier: MIT OR Apache-2.0

/// Moli's built-in HTML user-agent stylesheet.
///
/// Keep source provenance and engine-specific adaptations next to the CSS so
/// UA behavior can be audited without growing the retained-style owner.
pub(super) const HTML_STYLESHEET: &str = include_str!("ua/html.css");
