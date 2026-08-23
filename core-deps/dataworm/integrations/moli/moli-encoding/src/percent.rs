pub(crate) fn append_percent_encoded_ncr(output: &mut String, ch: char) {
    output.push_str("%26%23");
    output.push_str(&(ch as u32).to_string());
    output.push_str("%3B");
}

pub(crate) fn append_percent_encoded_byte(output: &mut String, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    output.push('%');
    output.push(HEX[(byte >> 4) as usize] as char);
    output.push(HEX[(byte & 0x0F) as usize] as char);
}
