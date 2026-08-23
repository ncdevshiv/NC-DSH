use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use url::Url;

use crate::ConnectOptions;

pub(crate) async fn connect_websocket_via_http_proxy_tunnel(
    uri: &http::Uri,
    proxy_url: &Url,
    context: &ConnectOptions,
) -> Result<TcpStream, String> {
    let proxy_host = proxy_url
        .host_str()
        .ok_or_else(|| "WebSocket proxy URL is missing host".to_owned())?;
    let proxy_port = proxy_url
        .port_or_known_default()
        .ok_or_else(|| "WebSocket proxy URL is missing port".to_owned())?;
    let target_authority = websocket_target_authority(uri)?;
    let mut socket = TcpStream::connect(format!("{proxy_host}:{proxy_port}"))
        .await
        .map_err(|error| format!("failed to connect WebSocket proxy: {error}"))?;
    let mut connect_request = format!(
        "CONNECT {target_authority} HTTP/1.1\r\n\
         Host: {target_authority}\r\n\
         Proxy-Connection: Keep-Alive\r\n"
    );
    append_proxy_connect_header(&mut connect_request, "User-Agent", &context.user_agent)?;
    if let Some(token) = context.proxy_bearer_token.as_deref() {
        append_proxy_connect_header(
            &mut connect_request,
            "Proxy-Authorization",
            &format!("Bearer {token}"),
        )?;
    }
    connect_request.push_str("\r\n");
    socket
        .write_all(connect_request.as_bytes())
        .await
        .map_err(|error| format!("failed to write WebSocket proxy CONNECT: {error}"))?;
    socket
        .flush()
        .await
        .map_err(|error| format!("failed to flush WebSocket proxy CONNECT: {error}"))?;
    let response = read_proxy_connect_response(&mut socket).await?;
    if !proxy_connect_response_is_200(&response) {
        let status = response
            .lines()
            .next()
            .unwrap_or("HTTP proxy CONNECT failed");
        return Err(format!("WebSocket proxy CONNECT failed: {status}"));
    }
    Ok(socket)
}

fn proxy_connect_response_is_200(response: &str) -> bool {
    response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        == Some("200")
}

pub(crate) fn append_proxy_connect_header(
    request: &mut String,
    name: &str,
    value: &str,
) -> Result<(), String> {
    if value.bytes().any(|byte| matches!(byte, b'\r' | b'\n')) {
        return Err(format!(
            "invalid WebSocket proxy CONNECT header `{name}` contains a newline"
        ));
    }
    request.push_str(name);
    request.push_str(": ");
    request.push_str(value);
    request.push_str("\r\n");
    Ok(())
}

async fn read_proxy_connect_response(socket: &mut TcpStream) -> Result<String, String> {
    let mut response = Vec::new();
    let mut chunk = [0_u8; 512];
    loop {
        let count = socket
            .read(&mut chunk)
            .await
            .map_err(|error| format!("failed to read WebSocket proxy CONNECT response: {error}"))?;
        if count == 0 {
            return Err("WebSocket proxy closed during CONNECT".to_owned());
        }
        response.extend_from_slice(&chunk[..count]);
        if response.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(String::from_utf8_lossy(&response).into_owned());
        }
        if response.len() > 16 * 1024 {
            return Err("WebSocket proxy CONNECT response is too large".to_owned());
        }
    }
}

pub(crate) fn websocket_proxy_url(
    uri: &http::Uri,
    context: &ConnectOptions,
) -> Result<Option<Url>, String> {
    websocket_proxy_url_with_env(uri, context, |name| std::env::var(name).ok())
}

pub(crate) fn websocket_proxy_url_with_env(
    uri: &http::Uri,
    context: &ConnectOptions,
    mut env: impl FnMut(&str) -> Option<String>,
) -> Result<Option<Url>, String> {
    let proxy = match context.http_proxy.as_deref() {
        Some("") => return Ok(None),
        Some(proxy) => Some(proxy.to_owned()),
        None => websocket_env_proxy_for_scheme(uri.scheme_str(), &mut env),
    };
    let Some(proxy) = proxy.filter(|proxy| !proxy.is_empty()) else {
        return Ok(None);
    };
    let host = uri
        .host()
        .ok_or_else(|| "WebSocket URL is missing host".to_owned())?;
    let no_proxy = match context.http_no_proxy.as_deref() {
        Some(no_proxy) => Some(no_proxy.to_owned()),
        None => websocket_env_no_proxy(&mut env),
    };
    if no_proxy_matches(host, uri.port_u16(), no_proxy.as_deref()) {
        return Ok(None);
    }
    let proxy_url = Url::parse(&proxy)
        .map_err(|error| format!("failed to parse WebSocket proxy URL `{proxy}`: {error}"))?;
    if proxy_url.scheme() != "http" {
        return Err(format!(
            "unsupported WebSocket proxy scheme `{}`; only http proxies are supported",
            proxy_url.scheme()
        ));
    }
    if proxy_url.host_str().is_none() {
        return Err("WebSocket proxy URL is missing host".to_owned());
    }
    Ok(Some(proxy_url))
}

fn websocket_env_proxy_for_scheme(
    scheme: Option<&str>,
    env: &mut impl FnMut(&str) -> Option<String>,
) -> Option<String> {
    let mut names: &[&str] = match scheme {
        Some("ws") => &["http_proxy"],
        Some("wss") => &["https_proxy", "HTTPS_PROXY"],
        _ => &[],
    };
    for name in names {
        if let Some(value) = env(name).filter(|value| !value.is_empty()) {
            return Some(value);
        }
    }
    names = &["all_proxy", "ALL_PROXY"];
    for name in names {
        if let Some(value) = env(name).filter(|value| !value.is_empty()) {
            return Some(value);
        }
    }
    None
}

fn websocket_env_no_proxy(env: &mut impl FnMut(&str) -> Option<String>) -> Option<String> {
    env("no_proxy")
        .filter(|value| !value.is_empty())
        .or_else(|| env("NO_PROXY").filter(|value| !value.is_empty()))
}

pub(crate) fn websocket_target_authority(uri: &http::Uri) -> Result<String, String> {
    let host = uri
        .host()
        .ok_or_else(|| "WebSocket URL is missing host".to_owned())?;
    let port = uri
        .port_u16()
        .or_else(|| match uri.scheme_str() {
            Some("wss") => Some(443),
            Some("ws") => Some(80),
            _ => None,
        })
        .ok_or_else(|| "WebSocket URL is missing port".to_owned())?;
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    Ok(format!("{host}:{port}"))
}

pub(crate) fn no_proxy_matches(host: &str, port: Option<u16>, no_proxy: Option<&str>) -> bool {
    let Some(no_proxy) = no_proxy else {
        return false;
    };
    let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    no_proxy.split(',').any(|token| {
        let token = token.trim();
        if token.is_empty() {
            return false;
        }
        if token == "*" {
            return true;
        }
        let (token_host, token_port) = split_no_proxy_host_port(token);
        if let Some(token_port) = token_port
            && Some(token_port) != port
        {
            return false;
        }
        let token_host = token_host
            .trim_matches(['[', ']'])
            .trim_start_matches('.')
            .to_ascii_lowercase();
        !token_host.is_empty()
            && (host == token_host
                || host
                    .strip_suffix(&token_host)
                    .is_some_and(|prefix| prefix.ends_with('.')))
    })
}

fn split_no_proxy_host_port(token: &str) -> (&str, Option<u16>) {
    let Some((host, port)) = token.rsplit_once(':') else {
        return (token, None);
    };
    match port.parse::<u16>() {
        Ok(port) if !host.contains(':') => (host, Some(port)),
        _ => (token, None),
    }
}
