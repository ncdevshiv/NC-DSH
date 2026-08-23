use crate::conn::{CdpConnection, TargetEmulationSessionStateMut};

pub(super) fn mutate_page_session_state(
    conn: &mut CdpConnection,
    session_id: Option<&str>,
    f: impl FnOnce(TargetEmulationSessionStateMut<'_>),
) -> Result<(), String> {
    if conn.mutate_emulation_session_state_for_session_owner(session_id, |state| {
        if let Some(state) = state {
            f(state);
        }
    }) {
        return Ok(());
    }
    Err("BrowserContextNotLoaded".to_owned())
}
