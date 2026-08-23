use data_url::DataUrl;
use url::Url;

pub(crate) fn decode_data_url_script_source(
    url: &Url,
    operation_prefix: &str,
) -> Result<String, String> {
    let raw = url.as_str();
    let data_url = DataUrl::process(raw)
        .map_err(|_| format!("{operation_prefix}: invalid data URL `{raw}`."))?;
    let (bytes, _) = data_url
        .decode_to_vec()
        .map_err(|_| format!("{operation_prefix}: invalid base64 data URL `{raw}`."))?;
    String::from_utf8(bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_url_script_source_uses_data_url_processor_for_base64_and_plain_bodies() {
        let plain = Url::parse("data:text/javascript,postMessage(%27plain%27)").unwrap();
        assert_eq!(
            decode_data_url_script_source(&plain, "worker").unwrap(),
            "postMessage('plain')"
        );

        let base64 = Url::parse("data:text/javascript;base64,cG9zdE1lc3NhZ2UoJ2I2NCcp").unwrap();
        assert_eq!(
            decode_data_url_script_source(&base64, "worker").unwrap(),
            "postMessage('b64')"
        );
    }

    #[test]
    fn data_url_script_source_rejects_invalid_data_urls_and_base64() {
        let missing_comma = Url::parse("data:text/javascript").unwrap();
        assert!(
            decode_data_url_script_source(&missing_comma, "worker")
                .unwrap_err()
                .contains("invalid data URL")
        );

        let invalid_base64 = Url::parse("data:text/javascript;base64,%%%").unwrap();
        assert!(
            decode_data_url_script_source(&invalid_base64, "worker")
                .unwrap_err()
                .contains("invalid base64 data URL")
        );
    }
}
