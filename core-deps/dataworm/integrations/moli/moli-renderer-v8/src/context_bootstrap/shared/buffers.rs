pub(in crate::context_bootstrap) fn text_decoder_encoding_for_label(
    label: &str,
) -> Option<&'static encoding_rs::Encoding> {
    encoding_rs::Encoding::for_label_no_replacement(label.as_bytes())
}

pub(in crate::context_bootstrap) fn canonical_text_decoder_encoding_name(
    encoding: &'static encoding_rs::Encoding,
) -> String {
    encoding.name().to_ascii_lowercase()
}

pub(in crate::context_bootstrap) fn new_uint8_array_from_bytes<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    bytes: Vec<u8>,
) -> Option<v8::Local<'s, v8::Uint8Array>> {
    let len = bytes.len();
    let backing_store = v8::ArrayBuffer::new_backing_store_from_vec(bytes).make_shared();
    let buffer = v8::ArrayBuffer::with_backing_store(scope, &backing_store);
    v8::Uint8Array::new(scope, buffer, 0, len)
}
