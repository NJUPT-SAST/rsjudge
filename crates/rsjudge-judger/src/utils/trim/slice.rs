#![allow(dead_code)]

#[inline]
pub(crate) const fn trim_ascii_start(mut bytes: &[u8]) -> &[u8] {
    // Note: A pattern matching based approach (instead of indexing) allows
    // making the function const.
    while let [first, rest @ ..] = bytes {
        if first.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    bytes
}

#[inline]
pub(crate) const fn trim_ascii_end(mut bytes: &[u8]) -> &[u8] {
    // Note: A pattern matching based approach (instead of indexing) allows
    // making the function const.
    while let [rest @ .., last] = bytes {
        if last.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    bytes
}

#[inline]
pub(crate) const fn trim_ascii(bytes: &[u8]) -> &[u8] {
    trim_ascii_end(trim_ascii_start(bytes))
}
