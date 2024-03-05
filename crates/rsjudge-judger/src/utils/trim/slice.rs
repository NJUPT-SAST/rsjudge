#![allow(dead_code)]

#[inline]
pub(crate) const fn trim_ascii_start(input: &[u8]) -> &[u8] {
    let mut bytes = input;
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
pub(crate) const fn trim_ascii_end(input: &[u8]) -> &[u8] {
    let mut bytes = input;
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
pub(crate) const fn trim_ascii(input: &[u8]) -> &[u8] {
    trim_ascii_end(trim_ascii_start(input))
}
