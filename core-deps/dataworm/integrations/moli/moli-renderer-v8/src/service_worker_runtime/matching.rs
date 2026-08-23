use url::Url;

pub(super) fn service_worker_scope_matches_url(scope_url: &Url, candidate_url: &Url) -> bool {
    let scope = scope_url.as_str();
    let candidate = candidate_url.as_str();
    let Some(remainder) = candidate.strip_prefix(scope) else {
        return false;
    };
    if scope.ends_with('/') {
        return true;
    }
    matches!(
        remainder.as_bytes().first(),
        None | Some(b'/' | b'?' | b'#')
    )
}
