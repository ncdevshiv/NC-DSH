use url::Url;

pub fn should_request_be_blocked_due_to_bad_port(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https") && url.port().is_some_and(is_bad_port)
}

pub fn is_bad_port(port: u16) -> bool {
    BAD_PORTS.binary_search(&port).is_ok()
}

const BAD_PORTS: &[u16] = &[
    0, 1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101,
    102, 103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427,
    465, 512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990,
    993, 995, 1719, 1720, 1723, 2049, 3659, 4045, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668,
    6669, 6697, 10080,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn url(input: &str) -> Url {
        Url::parse(input).unwrap()
    }

    #[test]
    fn bad_port_table_matches_fetch_and_chromium_blocking_set() {
        for port in [0, 1, 22, 25, 554, 989, 990, 6000, 6667, 10080] {
            assert!(is_bad_port(port), "port {port} should be blocked");
        }
        for port in [80, 443, 8080, 8443] {
            assert!(!is_bad_port(port), "port {port} should be allowed");
        }
    }

    #[test]
    fn bad_port_policy_only_blocks_explicit_http_ports() {
        assert!(should_request_be_blocked_due_to_bad_port(&url(
            "http://example.test:25/"
        )));
        assert!(should_request_be_blocked_due_to_bad_port(&url(
            "https://example.test:6000/"
        )));
        assert!(!should_request_be_blocked_due_to_bad_port(&url(
            "http://example.test/"
        )));
        assert!(!should_request_be_blocked_due_to_bad_port(&url(
            "https://example.test/"
        )));
        assert!(!should_request_be_blocked_due_to_bad_port(&url(
            "ws://example.test:25/"
        )));
    }
}
