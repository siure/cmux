pub fn file_url_for_path(path: &str) -> String {
    let encoded = percent_encode_file_path(path);
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

fn percent_encode_file_path(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(*byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::file_url_for_path;

    #[test]
    fn file_url_preserves_absolute_slashes_and_encodes_reserved_bytes() {
        assert_eq!(
            file_url_for_path("/tmp/cmux drop/a#b?.txt"),
            "file:///tmp/cmux%20drop/a%23b%3F.txt"
        );
    }

    #[test]
    fn file_url_encodes_utf8_path_bytes() {
        assert_eq!(
            file_url_for_path("/tmp/cmux/cafe\u{301}.txt"),
            "file:///tmp/cmux/cafe%CC%81.txt"
        );
    }
}
