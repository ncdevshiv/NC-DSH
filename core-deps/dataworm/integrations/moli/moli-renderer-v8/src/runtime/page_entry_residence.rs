//! Owner-local residence for a page's runtime entry.
//!
//! The owner-local page slot outlives any one command or lifecycle turn. A
//! turn may temporarily check out the `PageVm` entry, but an external wait
//! must restore it before yielding back to the renderer owner. Retirement is
//! sticky so an entry returned by a cancelled or stale turn cannot resurrect
//! a removed page.

#[derive(Debug)]
pub(super) struct RendererPageEntryResidenceSlot<Entry> {
    residence: RendererPageEntryResidence<Entry>,
}

#[derive(Debug)]
enum RendererPageEntryResidence<Entry> {
    Resident(Entry),
    CheckedOut,
    Retiring,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum RendererPageEntryCheckout<Entry> {
    Entry(Entry),
    Busy,
    Retired,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum RendererPageEntryRestore<Entry> {
    Restored,
    Retire(Entry),
    Duplicate(Entry),
}

impl<Entry> RendererPageEntryResidenceSlot<Entry> {
    pub(super) fn new(entry: Entry) -> Self {
        Self {
            residence: RendererPageEntryResidence::Resident(entry),
        }
    }

    pub(super) fn resident(&self) -> Option<&Entry> {
        match &self.residence {
            RendererPageEntryResidence::Resident(entry) => Some(entry),
            RendererPageEntryResidence::CheckedOut | RendererPageEntryResidence::Retiring => None,
        }
    }

    pub(super) fn resident_mut(&mut self) -> Option<&mut Entry> {
        match &mut self.residence {
            RendererPageEntryResidence::Resident(entry) => Some(entry),
            RendererPageEntryResidence::CheckedOut | RendererPageEntryResidence::Retiring => None,
        }
    }

    pub(super) fn checkout(&mut self) -> RendererPageEntryCheckout<Entry> {
        let residence =
            std::mem::replace(&mut self.residence, RendererPageEntryResidence::CheckedOut);
        match residence {
            RendererPageEntryResidence::Resident(entry) => RendererPageEntryCheckout::Entry(entry),
            RendererPageEntryResidence::CheckedOut => {
                self.residence = RendererPageEntryResidence::CheckedOut;
                RendererPageEntryCheckout::Busy
            }
            RendererPageEntryResidence::Retiring => {
                self.residence = RendererPageEntryResidence::Retiring;
                RendererPageEntryCheckout::Retired
            }
        }
    }

    pub(super) fn restore(&mut self, entry: Entry) -> RendererPageEntryRestore<Entry> {
        let residence =
            std::mem::replace(&mut self.residence, RendererPageEntryResidence::Retiring);
        match residence {
            RendererPageEntryResidence::CheckedOut => {
                self.residence = RendererPageEntryResidence::Resident(entry);
                RendererPageEntryRestore::Restored
            }
            RendererPageEntryResidence::Retiring => RendererPageEntryRestore::Retire(entry),
            RendererPageEntryResidence::Resident(resident) => {
                self.residence = RendererPageEntryResidence::Resident(resident);
                RendererPageEntryRestore::Duplicate(entry)
            }
        }
    }

    pub(super) fn request_retirement(&mut self) -> Option<Entry> {
        let residence =
            std::mem::replace(&mut self.residence, RendererPageEntryResidence::Retiring);
        match residence {
            RendererPageEntryResidence::Resident(entry) => Some(entry),
            RendererPageEntryResidence::CheckedOut | RendererPageEntryResidence::Retiring => None,
        }
    }

    pub(super) fn is_retiring(&self) -> bool {
        matches!(self.residence, RendererPageEntryResidence::Retiring)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_keeps_the_residence_slot_busy_until_restore() {
        let mut residence = RendererPageEntryResidenceSlot::new(7_u8);

        assert_eq!(residence.checkout(), RendererPageEntryCheckout::Entry(7));
        assert_eq!(residence.checkout(), RendererPageEntryCheckout::Busy);
        assert_eq!(residence.restore(9), RendererPageEntryRestore::Restored);
        assert_eq!(residence.resident(), Some(&9));
    }

    #[test]
    fn retirement_while_checked_out_is_applied_when_the_entry_returns() {
        let mut residence = RendererPageEntryResidenceSlot::new(7_u8);

        assert_eq!(residence.checkout(), RendererPageEntryCheckout::Entry(7));
        assert_eq!(residence.request_retirement(), None);
        assert!(residence.is_retiring());
        assert_eq!(residence.restore(7), RendererPageEntryRestore::Retire(7));
        assert!(residence.is_retiring());
    }

    #[test]
    fn retirement_takes_a_resident_entry_immediately() {
        let mut residence = RendererPageEntryResidenceSlot::new(7_u8);

        assert_eq!(residence.request_retirement(), Some(7));
        assert_eq!(residence.checkout(), RendererPageEntryCheckout::Retired);
    }
}
