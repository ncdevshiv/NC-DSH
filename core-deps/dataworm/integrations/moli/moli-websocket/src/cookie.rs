use url::Url;

pub fn websocket_cookie_url(url: &Url) -> Url {
    let mut cookie_url = url.clone();
    match url.scheme() {
        "ws" => {
            let _ = cookie_url.set_scheme("http");
        }
        "wss" => {
            let _ = cookie_url.set_scheme("https");
        }
        _ => {}
    }
    cookie_url
}
