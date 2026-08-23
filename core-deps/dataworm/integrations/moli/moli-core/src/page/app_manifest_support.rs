use anyhow::Result;

use super::{
    CompletedPageCommand, Page, PendingPageCommand, RendererAppManifestLoadPublication,
    RendererCommandTurnOutput, RendererPageCommand, RendererPageReply,
};

impl Page {
    pub fn start_prepare_app_manifest_load(&self) -> Result<PendingPageCommand> {
        self.start_page_command(RendererPageCommand::PrepareAppManifestLoad)
    }

    pub fn finish_prepare_app_manifest_load(
        &mut self,
        completion: CompletedPageCommand,
    ) -> Result<moli_renderer_v8::RendererAppManifestLoadPreparation> {
        let reply = self.finish_page_command(completion);
        expect_page_reply!(
            reply,
            "prepare app manifest load",
            "an app manifest load preparation",
            RendererPageReply::AppManifestLoadPreparation(preparation) => Ok(preparation),
        )
    }

    pub fn start_publish_app_manifest_load(
        &self,
        publication: RendererAppManifestLoadPublication,
    ) -> Result<PendingPageCommand> {
        self.start_page_command(RendererPageCommand::PublishAppManifestLoad(Box::new(
            publication,
        )))
    }

    pub fn finish_publish_app_manifest_load(
        &mut self,
        completion: CompletedPageCommand,
    ) -> Result<RendererCommandTurnOutput> {
        let output = self.finish_page_command_turn(completion);
        if !matches!(output.completion().reply(), RendererPageReply::Unit) {
            anyhow::bail!("app manifest publication returned an unexpected renderer reply");
        }
        Ok(output)
    }
}
