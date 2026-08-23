//! CSS gradient stop projection.
//!
//! The normalization and interpolation-hint algorithms are narrowly adapted
//! from Blink's `CSSGradientValue::NormalizeAndAddStops` pipeline in
//! `third_party/blink/renderer/core/css/css_gradient_value.cc`. Keeping this
//! logic in the layout snapshot owner is important: raster backends accept a
//! normalized `[0, 1]` stop domain and must not reinterpret CSS lengths,
//! percentages, repeating periods, or hints.

use style::{
    color::{AbsoluteColor, mix::ColorInterpolationMethod},
    values::{computed::CSSPixelLength, generics::image::GenericGradientItem},
};

use super::{GradientItem, interpolation::interpolate_color};

#[derive(Clone, Copy, Debug)]
pub(super) struct AbsoluteGradientStop {
    pub(super) offset: f32,
    pub(super) color: AbsoluteColor,
}

#[derive(Clone, Copy, Debug)]
struct PositionedGradientItem {
    offset: Option<f32>,
    color: Option<AbsoluteColor>,
}

#[derive(Debug)]
pub(super) struct NormalizedGradientStops {
    pub(super) stops: Vec<AbsoluteGradientStop>,
    pub(super) first_offset: f32,
    pub(super) last_offset: f32,
    pub(super) normalized: bool,
}

pub(super) fn resolve_stops<T>(
    items: &[GradientItem<T>],
    gradient_length: CSSPixelLength,
    current_color: &AbsoluteColor,
    interpolation_method: ColorInterpolationMethod,
    resolve_position: impl Fn(CSSPixelLength, &T) -> Option<f32>,
) -> Vec<AbsoluteGradientStop> {
    let mut positioned = items
        .iter()
        .map(|item| match item {
            GenericGradientItem::SimpleColorStop(color) => PositionedGradientItem {
                offset: None,
                color: Some(color.resolve_to_absolute(current_color)),
            },
            GenericGradientItem::ComplexColorStop { color, position } => PositionedGradientItem {
                offset: Some(finite_or_zero(resolve_position(gradient_length, position))),
                color: Some(color.resolve_to_absolute(current_color)),
            },
            GenericGradientItem::InterpolationHint(position) => PositionedGradientItem {
                offset: Some(finite_or_zero(resolve_position(gradient_length, position))),
                color: None,
            },
        })
        .collect::<Vec<_>>();

    if positioned.is_empty() {
        return solid_absolute_stops(AbsoluteColor::TRANSPARENT_BLACK);
    }
    if positioned[0].offset.is_none() {
        positioned[0].offset = Some(0.0);
    }
    let last_index = positioned.len() - 1;
    if positioned[last_index].offset.is_none() {
        positioned[last_index].offset = Some(1.0);
    }

    let mut previous_specified = positioned[0].offset.unwrap_or(0.0);
    for item in positioned.iter_mut().skip(1) {
        if let Some(offset) = item.offset.as_mut() {
            *offset = offset.max(previous_specified);
            previous_specified = *offset;
        }
    }

    let mut previous_index = 0usize;
    while previous_index < last_index {
        let next_index = (previous_index + 1..=last_index)
            .find(|index| positioned[*index].offset.is_some())
            .unwrap_or(last_index);
        let start = positioned[previous_index].offset.unwrap_or(0.0);
        let end = positioned[next_index].offset.unwrap_or(start).max(start);
        let span = (next_index - previous_index) as f32;
        for (index, item) in positioned[previous_index + 1..next_index]
            .iter_mut()
            .enumerate()
        {
            item.offset = Some(start + (end - start) * (index as f32 + 1.0) / span);
        }
        previous_index = next_index;
    }

    let mut stops = Vec::with_capacity(positioned.len());
    for (index, item) in positioned.iter().enumerate() {
        let offset = item.offset.unwrap_or(0.0);
        if let Some(color) = item.color {
            stops.push(AbsoluteGradientStop { offset, color });
            continue;
        }

        let Some(left) = stops.last().copied() else {
            continue;
        };
        let Some(right_item) = positioned.get(index + 1) else {
            continue;
        };
        let Some(right_color) = right_item.color else {
            continue;
        };
        append_interpolation_hint_stops(
            &mut stops,
            left,
            AbsoluteGradientStop {
                offset: right_item.offset.unwrap_or(offset),
                color: right_color,
            },
            offset,
            interpolation_method,
        );
    }

    if stops.is_empty() {
        solid_absolute_stops(AbsoluteColor::TRANSPARENT_BLACK)
    } else if stops.len() == 1 {
        solid_absolute_stops(stops[0].color)
    } else {
        stops
    }
}

fn append_interpolation_hint_stops(
    stops: &mut Vec<AbsoluteGradientStop>,
    left: AbsoluteGradientStop,
    right: AbsoluteGradientStop,
    hint_offset: f32,
    interpolation_method: ColorInterpolationMethod,
) {
    let left_distance = hint_offset - left.offset;
    let right_distance = right.offset - hint_offset;
    let total_distance = right.offset - left.offset;
    if nearly_equal(left_distance, right_distance) {
        return;
    }
    if nearly_equal(left_distance, 0.0) {
        stops.push(AbsoluteGradientStop {
            offset: hint_offset,
            color: right.color,
        });
        return;
    }
    if nearly_equal(right_distance, 0.0) {
        stops.push(AbsoluteGradientStop {
            offset: hint_offset,
            color: left.color,
        });
        return;
    }
    if total_distance <= 0.0 {
        return;
    }

    let mut offsets = [0.0; 9];
    if left_distance > right_distance {
        for (index, offset) in offsets[..7].iter_mut().enumerate() {
            *offset = left.offset + left_distance * (7.0 + index as f32) / 13.0;
        }
        offsets[7] = hint_offset + right_distance / 3.0;
        offsets[8] = hint_offset + right_distance * 2.0 / 3.0;
    } else {
        offsets[0] = left.offset + left_distance / 3.0;
        offsets[1] = left.offset + left_distance * 2.0 / 3.0;
        for (index, offset) in offsets[2..].iter_mut().enumerate() {
            *offset = hint_offset + right_distance * index as f32 / 13.0;
        }
    }

    let hint_relative_offset = left_distance / total_distance;
    let exponent = 0.5_f32.ln() / hint_relative_offset.ln();
    for offset in offsets {
        let point_relative_offset = (offset - left.offset) / total_distance;
        let weight = point_relative_offset.powf(exponent);
        if weight.is_finite() {
            stops.push(AbsoluteGradientStop {
                offset,
                color: interpolate_color(
                    left.color,
                    right.color,
                    weight.clamp(0.0, 1.0),
                    interpolation_method,
                ),
            });
        }
    }
}

pub(super) fn stops_require_normalization(stops: &[AbsoluteGradientStop], repeating: bool) -> bool {
    repeating
        || stops.first().is_some_and(|stop| stop.offset < 0.0)
        || stops.last().is_some_and(|stop| stop.offset > 1.0)
}

pub(super) fn normalize_stops(
    stops: Vec<AbsoluteGradientStop>,
    required: bool,
    repeating: bool,
) -> NormalizedGradientStops {
    let first_offset = stops.first().map_or(0.0, |stop| stop.offset);
    let last_offset = stops.last().map_or(1.0, |stop| stop.offset);
    if !required {
        return NormalizedGradientStops {
            stops,
            first_offset,
            last_offset,
            normalized: false,
        };
    }

    let raw_span = last_offset - first_offset;
    let span = if raw_span.is_finite() {
        raw_span.max(0.0)
    } else {
        f32::MAX
    };
    if span < f32::EPSILON {
        let first_color = stops
            .first()
            .map_or(AbsoluteColor::TRANSPARENT_BLACK, |stop| stop.color);
        let last_color = stops
            .last()
            .map_or(AbsoluteColor::TRANSPARENT_BLACK, |stop| stop.color);
        let clamped_offset = first_offset.clamp(0.0, 1.0);
        let stops = if repeating {
            solid_absolute_stops(last_color)
        } else {
            vec![
                AbsoluteGradientStop {
                    offset: clamped_offset,
                    color: first_color,
                },
                AbsoluteGradientStop {
                    offset: clamped_offset,
                    color: last_color,
                },
            ]
        };
        return NormalizedGradientStops {
            stops,
            first_offset,
            last_offset,
            normalized: false,
        };
    }

    let stops = stops
        .into_iter()
        .map(|stop| AbsoluteGradientStop {
            offset: ((stop.offset - first_offset) / span).clamp(0.0, 1.0),
            color: stop.color,
        })
        .collect();
    NormalizedGradientStops {
        stops,
        first_offset,
        last_offset,
        normalized: true,
    }
}

pub(super) fn clamp_negative_radial_offsets(
    stops: &mut [AbsoluteGradientStop],
    interpolation_method: ColorInterpolationMethod,
) {
    let mut last_negative_offset = 0.0;
    for index in 0..stops.len() {
        let current_offset = stops[index].offset;
        if current_offset >= 0.0 {
            if index > 0 && last_negative_offset < 0.0 {
                let ratio = -last_negative_offset / (current_offset - last_negative_offset);
                stops[index - 1].color = interpolate_color(
                    stops[index - 1].color,
                    stops[index].color,
                    ratio.clamp(0.0, 1.0),
                    interpolation_method,
                );
            }
            break;
        }
        stops[index].offset = 0.0;
        last_negative_offset = current_offset;
    }
}

pub(super) fn clip_stops_to_unit_domain(
    stops: Vec<AbsoluteGradientStop>,
    interpolation_method: ColorInterpolationMethod,
) -> Vec<AbsoluteGradientStop> {
    let Some(first) = stops.first().copied() else {
        return stops;
    };
    let Some(last) = stops.last().copied() else {
        return stops;
    };
    if first.offset >= 0.0 && last.offset <= 1.0 {
        return stops;
    }

    let mut clipped = Vec::with_capacity(stops.len() + 2);
    clipped.push(AbsoluteGradientStop {
        offset: 0.0,
        color: color_at_offset(&stops, 0.0, interpolation_method),
    });
    clipped.extend(
        stops
            .iter()
            .copied()
            .filter(|stop| stop.offset > 0.0 && stop.offset < 1.0),
    );
    clipped.push(AbsoluteGradientStop {
        offset: 1.0,
        color: color_at_offset(&stops, 1.0, interpolation_method),
    });
    clipped
}

fn color_at_offset(
    stops: &[AbsoluteGradientStop],
    offset: f32,
    interpolation_method: ColorInterpolationMethod,
) -> AbsoluteColor {
    let first = stops.first().copied().unwrap_or(AbsoluteGradientStop {
        offset: 0.0,
        color: AbsoluteColor::TRANSPARENT_BLACK,
    });
    if offset < first.offset {
        return first.color;
    }
    let last = stops.last().copied().unwrap_or(first);
    if offset > last.offset {
        return last.color;
    }

    let right_index = stops
        .iter()
        .position(|stop| stop.offset >= offset)
        .unwrap_or(stops.len() - 1);
    let mut equal_index = right_index;
    while equal_index + 1 < stops.len() && nearly_equal(stops[equal_index + 1].offset, offset) {
        equal_index += 1;
    }
    if nearly_equal(stops[equal_index].offset, offset) {
        return stops[equal_index].color;
    }
    if right_index == 0 {
        return stops[0].color;
    }
    let left = stops[right_index - 1];
    let right = stops[right_index];
    let span = right.offset - left.offset;
    if span <= f32::EPSILON {
        return right.color;
    }
    interpolate_color(
        left.color,
        right.color,
        ((offset - left.offset) / span).clamp(0.0, 1.0),
        interpolation_method,
    )
}

fn solid_absolute_stops(color: AbsoluteColor) -> Vec<AbsoluteGradientStop> {
    vec![
        AbsoluteGradientStop { offset: 0.0, color },
        AbsoluteGradientStop { offset: 1.0, color },
    ]
}

fn finite_or_zero(value: Option<f32>) -> f32 {
    value.filter(|value| value.is_finite()).unwrap_or(0.0)
}

fn nearly_equal(left: f32, right: f32) -> bool {
    (left - right).abs() <= f32::EPSILON * 8.0 * left.abs().max(right.abs()).max(1.0)
}
