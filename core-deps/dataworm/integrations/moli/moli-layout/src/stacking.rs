use std::{fmt::Debug, hash::Hash};

use crate::{LayoutBoxId, LayoutPosition, LayoutWorld};

/// Source-free consumers see only the resulting numeric paint order. These
/// pass-local events are shared by geometry metadata and snapshot projection so
/// hit testing cannot silently diverge from pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintOrderEvent {
    BoxOutsetShadow(LayoutBoxId),
    PushStackingContext(LayoutBoxId),
    BoxBackground(LayoutBoxId),
    TableCollapsedBorders(LayoutBoxId),
    BoxContents(LayoutBoxId),
    BoxOutline(LayoutBoxId),
    PopStackingContext(LayoutBoxId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnitKind {
    Background,
    TableCollapsedBorders,
    Contents,
    Outline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PaintUnit {
    id: LayoutBoxId,
    kind: UnitKind,
    sequence: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChildContext {
    id: LayoutBoxId,
    z_index: i32,
    sequence: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtomicPaintEntry {
    Unit(PaintUnit),
    Context(ChildContext),
}

impl AtomicPaintEntry {
    fn sequence(self) -> usize {
        match self {
            Self::Unit(unit) => unit.sequence,
            Self::Context(context) => context.sequence,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtomicGroup {
    Normal,
    Float,
    Positioned,
}

#[derive(Default)]
struct ContextCollection {
    negative_contexts: Vec<ChildContext>,
    block_backgrounds: Vec<PaintUnit>,
    table_collapsed_borders: Vec<PaintUnit>,
    floats: Vec<PaintUnit>,
    inline_contents: Vec<PaintUnit>,
    positioned: Vec<AtomicPaintEntry>,
    positive_contexts: Vec<ChildContext>,
    outlines: Vec<PaintUnit>,
}

/// Builds one deterministic CSS stacking order for the current one-shot world.
///
/// The structure follows CSS 2.1 Appendix E's major paint levels: context root
/// background, negative stacking descendants, in-flow block backgrounds,
/// floats, inline contents, positioned/zero-level descendants, positive
/// stacking descendants, and outlines. Stacking descendants are hoisted only
/// to their nearest stacking context and remain atomic when recursively emitted.
pub(crate) fn build_paint_order<N>(world: &LayoutWorld<N>) -> Vec<PaintOrderEvent>
where
    N: Copy + Debug + Eq + Hash,
{
    let mut events = Vec::with_capacity(world.boxes.len() * 4);
    let mut sequence = 0usize;
    emit_context(world, world.root, &mut sequence, &mut events);
    events
}

fn emit_context<N>(
    world: &LayoutWorld<N>,
    root: LayoutBoxId,
    sequence: &mut usize,
    events: &mut Vec<PaintOrderEvent>,
) where
    N: Copy + Debug + Eq + Hash,
{
    events.push(PaintOrderEvent::PushStackingContext(root));
    events.push(PaintOrderEvent::BoxOutsetShadow(root));
    events.push(PaintOrderEvent::BoxBackground(root));

    let mut collection = ContextCollection::default();
    collection.inline_contents.push(PaintUnit {
        id: root,
        kind: UnitKind::Contents,
        sequence: next_sequence(sequence),
    });
    if world.boxes[root.index()].collapsed_table_borders.is_some() {
        collection.table_collapsed_borders.push(PaintUnit {
            id: root,
            kind: UnitKind::TableCollapsedBorders,
            sequence: next_sequence(sequence),
        });
    }

    for child in ordered_children(world, root) {
        collect_subtree(world, child, None, sequence, &mut collection);
    }

    collection
        .negative_contexts
        .sort_by_key(|context| (context.z_index, context.sequence));
    collection
        .positive_contexts
        .sort_by_key(|context| (context.z_index, context.sequence));
    collection
        .block_backgrounds
        .sort_by_key(|unit| unit.sequence);
    collection
        .table_collapsed_borders
        .sort_by_key(|unit| unit.sequence);
    collection.floats.sort_by_key(|unit| unit.sequence);
    collection.inline_contents.sort_by_key(|unit| unit.sequence);
    collection.positioned.sort_by_key(|entry| entry.sequence());
    collection.outlines.sort_by_key(|unit| unit.sequence);

    for context in collection.negative_contexts {
        emit_context(world, context.id, sequence, events);
    }
    emit_units(collection.block_backgrounds, events);
    emit_units(collection.table_collapsed_borders, events);
    emit_units(collection.floats, events);
    emit_units(collection.inline_contents, events);
    for entry in collection.positioned {
        match entry {
            AtomicPaintEntry::Unit(unit) => emit_unit(unit, events),
            AtomicPaintEntry::Context(context) => emit_context(world, context.id, sequence, events),
        }
    }
    for context in collection.positive_contexts {
        emit_context(world, context.id, sequence, events);
    }
    emit_units(collection.outlines, events);
    events.push(PaintOrderEvent::BoxOutline(root));
    events.push(PaintOrderEvent::PopStackingContext(root));
}

fn collect_subtree<N>(
    world: &LayoutWorld<N>,
    id: LayoutBoxId,
    inherited_group: Option<AtomicGroup>,
    sequence: &mut usize,
    collection: &mut ContextCollection,
) where
    N: Copy + Debug + Eq + Hash,
{
    let layout_box = &world.boxes[id.index()];
    let parent_is_flex_or_grid = layout_box.parent.is_some_and(|parent| {
        let display = world.boxes[parent.index()].style.display();
        display.is_flex_container() || display.is_grid_container()
    });
    if layout_box.creates_stacking_context(false, parent_is_flex_or_grid) {
        let context = ChildContext {
            id,
            z_index: layout_box.style.explicit_z_index().unwrap_or(0),
            sequence: next_sequence(sequence),
        };
        match context.z_index.cmp(&0) {
            std::cmp::Ordering::Less => collection.negative_contexts.push(context),
            std::cmp::Ordering::Equal => collection
                .positioned
                .push(AtomicPaintEntry::Context(context)),
            std::cmp::Ordering::Greater => collection.positive_contexts.push(context),
        }
        return;
    }

    let group = inherited_group.unwrap_or_else(|| {
        if layout_box.style.position() != LayoutPosition::Static {
            AtomicGroup::Positioned
        } else if layout_box.style.is_floated() {
            AtomicGroup::Float
        } else {
            AtomicGroup::Normal
        }
    });
    push_unit(
        collection,
        group,
        PaintUnit {
            id,
            kind: UnitKind::Background,
            sequence: next_sequence(sequence),
        },
    );
    push_unit(
        collection,
        if group == AtomicGroup::Normal {
            AtomicGroup::Normal
        } else {
            group
        },
        PaintUnit {
            id,
            kind: UnitKind::Contents,
            sequence: next_sequence(sequence),
        },
    );
    // Floats and positioned descendants are painted atomically at their
    // ancestor's paint level. Ordinary in-flow descendants are not: each child
    // must still classify itself as normal, floating, or positioned. Carrying
    // `Normal` down here would incorrectly bury a positioned grandchild in the
    // block-background/inline-content buckets.
    let descendant_group = match group {
        AtomicGroup::Normal => None,
        AtomicGroup::Float | AtomicGroup::Positioned => Some(group),
    };
    for child in ordered_children(world, id) {
        collect_subtree(world, child, descendant_group, sequence, collection);
    }
    if layout_box.collapsed_table_borders.is_some() {
        let unit = PaintUnit {
            id,
            kind: UnitKind::TableCollapsedBorders,
            sequence: next_sequence(sequence),
        };
        match group {
            AtomicGroup::Normal => collection.table_collapsed_borders.push(unit),
            AtomicGroup::Float => collection.floats.push(unit),
            AtomicGroup::Positioned => collection.positioned.push(AtomicPaintEntry::Unit(unit)),
        }
    }
    let outline = PaintUnit {
        id,
        kind: UnitKind::Outline,
        sequence: next_sequence(sequence),
    };
    match group {
        AtomicGroup::Normal => collection.outlines.push(outline),
        AtomicGroup::Float => collection.floats.push(outline),
        AtomicGroup::Positioned => collection.positioned.push(AtomicPaintEntry::Unit(outline)),
    }
}

fn push_unit(collection: &mut ContextCollection, group: AtomicGroup, unit: PaintUnit) {
    match group {
        AtomicGroup::Normal => match unit.kind {
            UnitKind::Background => collection.block_backgrounds.push(unit),
            UnitKind::TableCollapsedBorders => collection.table_collapsed_borders.push(unit),
            UnitKind::Contents => collection.inline_contents.push(unit),
            UnitKind::Outline => collection.outlines.push(unit),
        },
        AtomicGroup::Float => collection.floats.push(unit),
        AtomicGroup::Positioned => collection.positioned.push(AtomicPaintEntry::Unit(unit)),
    }
}

fn emit_units(units: Vec<PaintUnit>, events: &mut Vec<PaintOrderEvent>) {
    for unit in units {
        emit_unit(unit, events);
    }
}

fn emit_unit(unit: PaintUnit, events: &mut Vec<PaintOrderEvent>) {
    let event = match unit.kind {
        UnitKind::Background => {
            events.push(PaintOrderEvent::BoxOutsetShadow(unit.id));
            PaintOrderEvent::BoxBackground(unit.id)
        }
        UnitKind::TableCollapsedBorders => PaintOrderEvent::TableCollapsedBorders(unit.id),
        UnitKind::Contents => PaintOrderEvent::BoxContents(unit.id),
        UnitKind::Outline => PaintOrderEvent::BoxOutline(unit.id),
    };
    events.push(event);
}

fn ordered_children<N>(world: &LayoutWorld<N>, id: LayoutBoxId) -> Vec<LayoutBoxId>
where
    N: Copy + Debug + Eq + Hash,
{
    let layout_box = &world.boxes[id.index()];
    let display = layout_box.style.display();
    if !display.is_flex_container() && !display.is_grid_container() {
        return layout_box.children.clone();
    }
    let mut children = layout_box
        .children
        .iter()
        .copied()
        .enumerate()
        .collect::<Vec<_>>();
    children.sort_by_key(|(document_order, child)| {
        let child = &world.boxes[child.index()];
        let order = if matches!(
            child.style.position(),
            LayoutPosition::Absolute | LayoutPosition::Fixed
        ) {
            0
        } else {
            child.style.order()
        };
        (order, *document_order)
    });
    children.into_iter().map(|(_, child)| child).collect()
}

fn next_sequence(sequence: &mut usize) -> usize {
    let current = *sequence;
    *sequence = sequence.saturating_add(1);
    current
}
