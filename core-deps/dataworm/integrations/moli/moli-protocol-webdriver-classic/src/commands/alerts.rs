use moli_protocol::devtools_runtime::{
    DevToolsCommand, DevToolsGetJavaScriptDialogCommand, DevToolsHandleJavaScriptDialogCommand,
    DevToolsSetJavaScriptDialogPromptTextCommand,
};
use serde_json::Value;

use crate::{ClassicDevToolsCommandContext, ClassicError};

use super::parsing::required_string;

pub fn alert_text_command(context: &ClassicDevToolsCommandContext) -> DevToolsCommand {
    DevToolsCommand::GetJavaScriptDialog(DevToolsGetJavaScriptDialogCommand {
        context: context.command_context(),
    })
}

pub fn alert_handle_command(
    context: &ClassicDevToolsCommandContext,
    accept: bool,
) -> DevToolsCommand {
    DevToolsCommand::HandleJavaScriptDialog(DevToolsHandleJavaScriptDialogCommand {
        context: context.command_context(),
        accept,
        prompt_text: String::new(),
    })
}

pub fn alert_send_text_command(
    context: &ClassicDevToolsCommandContext,
    params: &Value,
) -> Result<DevToolsCommand, ClassicError> {
    Ok(DevToolsCommand::SetJavaScriptDialogPromptText(
        DevToolsSetJavaScriptDialogPromptTextCommand {
            context: context.command_context(),
            prompt_text: required_string(params, "text")?.to_owned(),
        },
    ))
}
