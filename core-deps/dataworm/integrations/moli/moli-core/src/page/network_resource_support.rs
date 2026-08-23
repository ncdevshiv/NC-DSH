use anyhow::Result;
use url::Url;

use super::{
    CompletedPageCommand, Page, PendingPageCommand, RendererNetworkResourceLoadPreparation,
    RendererPageCommand, RendererPageReply,
};

impl Page {
    pub fn start_prepare_network_resource_load(
        &self,
        frame_id: String,
        url: Url,
        disable_cache: bool,
        include_credentials: bool,
    ) -> Result<PendingPageCommand> {
        self.start_page_command(RendererPageCommand::PrepareNetworkResourceLoad {
            frame_id,
            url,
            disable_cache,
            include_credentials,
        })
    }

    pub fn finish_prepare_network_resource_load(
        &mut self,
        completion: CompletedPageCommand,
    ) -> Result<RendererNetworkResourceLoadPreparation> {
        let reply = self.finish_page_command(completion);
        expect_page_reply!(
            reply,
            "prepare DevTools network resource load",
            "a network resource load preparation",
            RendererPageReply::NetworkResourceLoadPreparation(preparation) => Ok(preparation),
        )
    }
}
