use crate::data_url::data_url_mime_type;
use crate::parse::mime_essence;

pub fn resource_mime_essence_for_url(url: &str, path: &str) -> Option<String> {
    let trimmed_url = url.trim_start();
    if trimmed_url.starts_with("data:") {
        return data_url_mime_type(trimmed_url).and_then(|mime| mime_essence(&mime));
    }
    resource_mime_essence_for_path(path).map(str::to_owned)
}

pub fn resource_mime_essence_for_path(path: &str) -> Option<&'static str> {
    let path = path.to_ascii_lowercase();
    if path.ends_with(".bmp") {
        Some("image/bmp")
    } else if path.ends_with(".css") {
        Some("text/css")
    } else if path.ends_with(".gif") {
        Some("image/gif")
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        Some("image/jpeg")
    } else if path.ends_with(".png") {
        Some("image/png")
    } else if path.ends_with(".txt") {
        Some("text/plain")
    } else if path.ends_with(".html") || path.ends_with(".htm") {
        Some("text/html")
    } else if path.ends_with(".svg") {
        Some("image/svg+xml")
    } else if path.ends_with(".xhtml") {
        Some("application/xhtml+xml")
    } else if path.ends_with(".xml") {
        Some("application/xml")
    } else if path.ends_with(".mp3") {
        Some("audio/mpeg")
    } else if path.ends_with(".wav") {
        Some("audio/wave")
    } else if path.ends_with(".mp4") {
        Some("video/mp4")
    } else if path.ends_with(".webm") {
        Some("video/webm")
    } else if path.ends_with(".otf") {
        Some("font/otf")
    } else if path.ends_with(".ttf") {
        Some("font/ttf")
    } else if path.ends_with(".woff") {
        Some("font/woff")
    } else if path.ends_with(".woff2") {
        Some("font/woff2")
    } else {
        None
    }
}

pub fn known_url_path_mime_essence(path: &str) -> Option<&'static str> {
    let path = path.to_ascii_lowercase();
    if path.ends_with(".bmp") {
        Some("image/bmp")
    } else if path.ends_with(".css") {
        Some("text/css")
    } else if path.ends_with(".gif") {
        Some("image/gif")
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        Some("image/jpeg")
    } else if path.ends_with(".png") {
        Some("image/png")
    } else if path.ends_with(".txt") {
        Some("text/plain")
    } else if path.ends_with(".html") || path.ends_with(".htm") {
        Some("text/html")
    } else if path.ends_with(".svg") {
        Some("image/svg+xml")
    } else if path.ends_with(".xhtml") {
        Some("application/xhtml+xml")
    } else if path.ends_with(".xml") {
        Some("application/xml")
    } else {
        None
    }
}
