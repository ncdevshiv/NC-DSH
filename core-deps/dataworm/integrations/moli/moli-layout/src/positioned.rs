use taffy::Line;

/// Resolve auto margins in one physical axis of an absolutely positioned box.
///
/// CSS Positioned Layout only distributes auto margins when both insets in
/// the axis are definite. Inline-axis negative space preserves the dominant
/// start edge; block-axis negative space is shared between both margins.
pub(crate) fn resolve_absolute_axis_margins(
    margin: Line<Option<f32>>,
    inset: Line<Option<f32>>,
    area_size: f32,
    box_size: f32,
    share_negative_space: bool,
    start_is_dominant: bool,
) -> Line<f32> {
    if inset.start.is_none() || inset.end.is_none() {
        return Line {
            start: margin.start.unwrap_or(0.0),
            end: margin.end.unwrap_or(0.0),
        };
    }

    let free_space = area_size
        - inset.start.unwrap()
        - inset.end.unwrap()
        - box_size
        - margin.start.unwrap_or(0.0)
        - margin.end.unwrap_or(0.0);
    match (margin.start, margin.end) {
        (Some(start), Some(end)) => Line { start, end },
        (None, Some(end)) => Line {
            start: free_space,
            end,
        },
        (Some(start), None) => Line {
            start,
            end: free_space,
        },
        (None, None) if free_space > 0.0 || share_negative_space => {
            let start = free_space / 2.0;
            Line {
                start,
                end: free_space - start,
            }
        }
        (None, None) if start_is_dominant => Line {
            start: 0.0,
            end: free_space,
        },
        (None, None) => Line {
            start: free_space,
            end: 0.0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTO: Line<Option<f32>> = Line {
        start: None,
        end: None,
    };
    const ZERO_INSETS: Line<Option<f32>> = Line {
        start: Some(0.0),
        end: Some(0.0),
    };

    #[test]
    fn positive_space_is_shared_even_when_the_box_is_wider_than_that_space() {
        assert_eq!(
            resolve_absolute_axis_margins(AUTO, ZERO_INSETS, 1440.0, 975.0, false, true),
            Line {
                start: 232.5,
                end: 232.5,
            }
        );
    }

    #[test]
    fn inline_negative_space_preserves_the_dominant_start_edge() {
        assert_eq!(
            resolve_absolute_axis_margins(AUTO, ZERO_INSETS, 100.0, 150.0, false, true),
            Line {
                start: 0.0,
                end: -50.0,
            }
        );
        assert_eq!(
            resolve_absolute_axis_margins(AUTO, ZERO_INSETS, 100.0, 150.0, false, false),
            Line {
                start: -50.0,
                end: 0.0,
            }
        );
    }

    #[test]
    fn block_negative_space_is_shared() {
        assert_eq!(
            resolve_absolute_axis_margins(AUTO, ZERO_INSETS, 100.0, 120.0, true, true),
            Line {
                start: -10.0,
                end: -10.0,
            }
        );
    }

    #[test]
    fn an_auto_inset_forces_auto_margins_to_zero() {
        assert_eq!(
            resolve_absolute_axis_margins(
                AUTO,
                Line {
                    start: Some(0.0),
                    end: None,
                },
                100.0,
                20.0,
                false,
                true,
            ),
            Line {
                start: 0.0,
                end: 0.0,
            }
        );
    }
}
