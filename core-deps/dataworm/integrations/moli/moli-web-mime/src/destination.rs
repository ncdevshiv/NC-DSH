#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchDestination {
    AudioWorklet,
    PaintWorklet,
    Script,
    ServiceWorker,
    SharedWorker,
    Style,
    Worker,
    Xslt,
    Other,
}

impl FetchDestination {
    pub fn from_token(token: &str) -> Self {
        match token {
            "audioworklet" => Self::AudioWorklet,
            "paintworklet" => Self::PaintWorklet,
            "script" => Self::Script,
            "serviceworker" => Self::ServiceWorker,
            "sharedworker" => Self::SharedWorker,
            "style" => Self::Style,
            "worker" => Self::Worker,
            "xslt" => Self::Xslt,
            _ => Self::Other,
        }
    }

    pub fn is_script_like(self) -> bool {
        matches!(
            self,
            Self::AudioWorklet
                | Self::PaintWorklet
                | Self::Script
                | Self::ServiceWorker
                | Self::SharedWorker
                | Self::Worker
                | Self::Xslt
        )
    }
}
