use crate::conn::Cmd;
use crate::domains::actions::WebMcpAction;
use crate::domains::command_output::CommandOutputPlan;

pub(crate) fn command_output_plan(cmd: &Cmd<'_>) -> CommandOutputPlan {
    match cmd.parse_action::<WebMcpAction>() {
        Some(WebMcpAction::Enable | WebMcpAction::Disable) => CommandOutputPlan::success(),
        None => CommandOutputPlan::error(-32601, "UnknownMethod"),
    }
}
