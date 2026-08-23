pub(crate) fn attribute_reprocesses_connected_stylesheet(name: &str) -> bool {
    matches!(name, "href" | "rel" | "as" | "type" | "disabled" | "sizes")
}

#[cfg(test)]
mod tests {
    use super::attribute_reprocesses_connected_stylesheet;

    #[test]
    fn media_and_blocking_update_the_current_load_without_reprocessing_it() {
        assert!(!attribute_reprocesses_connected_stylesheet("media"));
        assert!(!attribute_reprocesses_connected_stylesheet("blocking"));
    }

    #[test]
    fn request_and_sheet_identity_attributes_reprocess_the_link() {
        for name in ["href", "rel", "as", "type", "disabled", "sizes"] {
            assert!(attribute_reprocesses_connected_stylesheet(name), "{name}");
        }
    }

    #[test]
    fn request_metadata_changes_do_not_restart_an_existing_link_resource() {
        for name in [
            "crossorigin",
            "referrerpolicy",
            "integrity",
            "nonce",
            "charset",
            "fetchpriority",
        ] {
            assert!(!attribute_reprocesses_connected_stylesheet(name), "{name}");
        }
    }
}
