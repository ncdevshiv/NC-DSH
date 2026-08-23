use std::cell::RefCell;

use crate::protocol_types::EmulatedMediaOverrides;

#[cfg(test)]
use super::cause::PendingStyleInvalidationWorkKind;
use super::{StyleMutationEffect, StyleViewport};

#[derive(Default)]
pub(super) struct PendingStructuralStyleMutations {
    groups: RefCell<Vec<PendingStructuralStyleMutationGroup>>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PendingStructuralStyleMutationGroup {
    pub(super) effects: Vec<StyleMutationEffect>,
    pub(super) emulated_media: EmulatedMediaOverrides,
    pub(super) viewport: StyleViewport,
}

impl PendingStructuralStyleMutations {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn push(
        &self,
        effects: &[StyleMutationEffect],
        emulated_media: &EmulatedMediaOverrides,
        viewport: StyleViewport,
    ) {
        if effects.is_empty() {
            return;
        }
        let mut groups = self.groups.borrow_mut();
        if let Some(group) = groups.last_mut()
            && group.emulated_media == *emulated_media
            && group.viewport == viewport
        {
            group.effects.extend_from_slice(effects);
            return;
        }
        groups.push(PendingStructuralStyleMutationGroup {
            effects: effects.to_vec(),
            emulated_media: emulated_media.clone(),
            viewport,
        });
    }

    pub(super) fn take(&self) -> Vec<PendingStructuralStyleMutationGroup> {
        self.groups.take()
    }

    pub(super) fn clear(&self) {
        self.groups.borrow_mut().clear();
    }

    #[cfg(test)]
    pub(super) fn work_item_count_for_test(&self) -> usize {
        self.groups.borrow().len()
    }

    #[cfg(test)]
    pub(super) fn effect_count_for_test(&self) -> usize {
        self.groups
            .borrow()
            .iter()
            .map(|group| group.effects.len())
            .sum()
    }

    #[cfg(test)]
    pub(super) fn work_kind_names_for_test(&self) -> Vec<&'static str> {
        self.groups
            .borrow()
            .iter()
            .map(|_| PendingStyleInvalidationWorkKind::Mutation.name_for_test())
            .collect()
    }
}
