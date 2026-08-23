// SPDX-License-Identifier: MIT OR Apache-2.0
//
// SVG presentation-attribute synthesis is narrowly ported from
// DioxusLabs/blitz packages/blitz-dom/src/stylo.rs. Keeping it in the Stylo
// adapter lets the normal cascade, inheritance, and relative-length resolver
// own the result instead of teaching layout about authored attribute strings.

use selectors::sink::Push;
use style::{
    applicable_declarations::ApplicableDeclarationBlock,
    properties::{Importance, PropertyDeclaration, PropertyDeclarationBlock},
    rule_tree::{CascadeLevel, CascadeOrigin},
    servo_arc::Arc,
    shared_lock::SharedRwLock,
    stylesheets::layer_rule::LayerOrder,
    values::specified::{LengthPercentage, NoCalcLength, NoCalcPercentage},
};
use style_traits::ParsingMode;

use crate::dom::native::Element;

const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";

pub(super) fn synthesize_svg_presentational_hints<V>(
    element: &Element,
    shared_lock: &SharedRwLock,
    hints: &mut V,
) where
    V: Push<ApplicableDeclarationBlock>,
{
    if element.namespace() != SVG_NAMESPACE || element.local_name() != "svg" {
        return;
    }

    for (attribute, is_width) in [("width", true), ("height", false)] {
        let Some(value) = element.attribute(attribute) else {
            continue;
        };
        let Some(size) = parse_svg_size_attribute(value) else {
            continue;
        };
        use style::values::generics::{NonNegative, length::Size};
        let size = Size::LengthPercentage(NonNegative(size));
        let declaration = if is_width {
            PropertyDeclaration::Width(size)
        } else {
            PropertyDeclaration::Height(size)
        };
        hints.push(ApplicableDeclarationBlock::from_declarations(
            Arc::new(shared_lock.wrap(PropertyDeclarationBlock::with_one(
                declaration,
                Importance::Normal,
            ))),
            CascadeLevel::new(CascadeOrigin::PresHints),
            LayerOrder::root(),
        ));
    }
}

/// Parses the SVG 2 root `width`/`height` presentation attributes.
///
/// These are CSS `<length-percentage>` values, unlike legacy HTML dimension
/// attributes. Unitless numbers are SVG user units and therefore CSS pixels;
/// relative units such as `em`, `rem`, and viewport units remain specified
/// lengths here so Stylo resolves them in the element's real style context.
fn parse_svg_size_attribute(value: &str) -> Option<LengthPercentage> {
    let value = value.trim();
    if let Some(number) = value.strip_suffix('%') {
        let value = number.trim().parse::<f32>().ok()?;
        return (value.is_finite() && value >= 0.0)
            .then(|| LengthPercentage::Percentage(NoCalcPercentage::new(value / 100.0)));
    }

    // A CSS dimension has no whitespace between its number and unit. Taking
    // only a trailing alphabetic run leaves scientific notation such as
    // `1e3` intact because it ends in a digit.
    let number_len = value
        .trim_end_matches(|character: char| character.is_ascii_alphabetic())
        .len();
    let (number, unit) = value.split_at(number_len);
    let value = number
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)?;
    let length = if unit.is_empty() {
        NoCalcLength::from_px(value)
    } else {
        NoCalcLength::parse_dimension_with_flags(ParsingMode::DEFAULT, false, value, unit).ok()?
    };
    Some(LengthPercentage::Length(length))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_size_attribute_preserves_relative_units_for_stylo() {
        assert!(parse_svg_size_attribute("1em").is_some());
        assert!(parse_svg_size_attribute("1.5rem").is_some());
        assert!(parse_svg_size_attribute("24").is_some());
        assert!(parse_svg_size_attribute("50%").is_some());
        assert!(parse_svg_size_attribute("auto").is_none());
        assert!(parse_svg_size_attribute("-1em").is_none());
    }
}
