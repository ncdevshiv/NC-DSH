pub const DEFAULT_MULTIPART_BLOB_CONTENT_TYPE: &str = "application/octet-stream";
pub const DEFAULT_MULTIPART_PARSED_FILE_CONTENT_TYPE: &str = "text/plain";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipartFormDataPart {
    pub name: String,
    pub value: MultipartFormDataPartValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultipartFormDataPartValue {
    Text(String),
    Blob {
        filename: String,
        content_type: String,
        body: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipartFormDataEntry {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MultipartHeaders {
    pub(crate) name: String,
    pub(crate) filename: Option<String>,
    pub(crate) content_type: Option<String>,
}
