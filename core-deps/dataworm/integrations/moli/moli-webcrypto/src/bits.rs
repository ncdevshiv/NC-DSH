use crate::WebCryptoError;

pub(crate) fn truncate_derived_bits(
    bytes: &[u8],
    length_bits: usize,
) -> Result<Vec<u8>, WebCryptoError> {
    if length_bits > bytes.len() * 8 {
        return Err(WebCryptoError::Operation);
    }
    let mut truncated = bytes[..length_bits.div_ceil(8)].to_vec();
    let used_bits_in_last_byte = length_bits % 8;
    if used_bits_in_last_byte != 0
        && let Some(last) = truncated.last_mut()
    {
        let mask = u8::MAX << (8 - used_bits_in_last_byte);
        *last &= mask;
    }
    Ok(truncated)
}
