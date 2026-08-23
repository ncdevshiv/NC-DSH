/// CDP `Target.TargetInfo.type` wire values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CdpTargetType {
    Browser,
    Tab,
    Page,
    Frame,
    Worker,
    SharedWorker,
    ServiceWorker,
    Other,
}

impl CdpTargetType {
    pub fn from_wire_value(target_type: Option<&str>) -> Self {
        match target_type {
            Some("browser") => Self::Browser,
            Some("tab") => Self::Tab,
            Some("page") => Self::Page,
            Some("iframe") => Self::Frame,
            Some("worker") => Self::Worker,
            Some("shared_worker") => Self::SharedWorker,
            Some("service_worker") => Self::ServiceWorker,
            _ => Self::Other,
        }
    }

    pub fn as_wire_value(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::Tab => "tab",
            Self::Page => "page",
            Self::Frame => "iframe",
            Self::Worker => "worker",
            Self::SharedWorker => "shared_worker",
            Self::ServiceWorker => "service_worker",
            Self::Other => "other",
        }
    }
}

/// Protocol-neutral target kinds that can be rendered as CDP
/// `Target.TargetInfo.type` wire values.
pub trait CdpTargetKindWire {
    fn to_cdp_target_type(self) -> CdpTargetType;
}

pub fn cdp_target_type_wire_value(kind: impl CdpTargetKindWire) -> &'static str {
    kind.to_cdp_target_type().as_wire_value()
}

#[cfg(test)]
mod tests {
    use super::{CdpTargetKindWire, CdpTargetType, cdp_target_type_wire_value};

    #[derive(Debug, Clone, Copy)]
    enum TestTargetKind {
        Page,
        SharedWorker,
    }

    impl CdpTargetKindWire for TestTargetKind {
        fn to_cdp_target_type(self) -> CdpTargetType {
            match self {
                Self::Page => CdpTargetType::Page,
                Self::SharedWorker => CdpTargetType::SharedWorker,
            }
        }
    }

    #[test]
    fn cdp_target_type_maps_chrome_wire_values() {
        assert_eq!(
            CdpTargetType::from_wire_value(Some("browser")),
            CdpTargetType::Browser
        );
        assert_eq!(
            CdpTargetType::from_wire_value(Some("tab")),
            CdpTargetType::Tab
        );
        assert_eq!(
            CdpTargetType::from_wire_value(Some("iframe")),
            CdpTargetType::Frame
        );
        assert_eq!(
            CdpTargetType::from_wire_value(Some("shared_worker")),
            CdpTargetType::SharedWorker
        );
        assert_eq!(
            CdpTargetType::from_wire_value(Some("unknown")),
            CdpTargetType::Other
        );
        assert_eq!(
            CdpTargetType::ServiceWorker.as_wire_value(),
            "service_worker"
        );
    }

    #[test]
    fn cdp_target_kind_wire_trait_centralizes_wire_values() {
        assert_eq!(cdp_target_type_wire_value(TestTargetKind::Page), "page");
        assert_eq!(
            cdp_target_type_wire_value(TestTargetKind::SharedWorker),
            "shared_worker"
        );
    }
}
