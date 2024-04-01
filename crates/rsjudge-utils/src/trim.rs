/// Returns a byte slice with leading ASCII whitespace bytes removed.
///
/// 'Whitespace' refers to the definition used by
/// `u8::is_ascii_whitespace`.
///
/// # Examples
///
/// ```
/// use rsjudge_utils::trim::trim_ascii_start;
/// assert_eq!(trim_ascii_start(b" \t hello world\n"), b"hello world\n");
/// assert_eq!(trim_ascii_start(b"  "), b"");
/// assert_eq!(trim_ascii_start(b""), b"");
/// ```
#[inline]
pub const fn trim_ascii_start(mut bytes: &[u8]) -> &[u8] {
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

/// Returns a byte slice with trailing ASCII whitespace bytes removed.
///
/// 'Whitespace' refers to the definition used by
/// `u8::is_ascii_whitespace`.
///
/// # Examples
///
/// ```
/// use rsjudge_utils::trim::trim_ascii_end;
/// assert_eq!(trim_ascii_end(b"\r hello world\n "), b"\r hello world");
/// assert_eq!(trim_ascii_end(b"  "), b"");
/// assert_eq!(trim_ascii_end(b""), b"");
/// ```
#[inline]
pub const fn trim_ascii_end(mut bytes: &[u8]) -> &[u8] {
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

/// Returns a byte slice with leading and trailing ASCII whitespace bytes
/// removed.
///
/// 'Whitespace' refers to the definition used by
/// `u8::is_ascii_whitespace`.
///
/// # Examples
///
/// ```
/// use rsjudge_utils::trim::trim_ascii;
/// assert_eq!(trim_ascii(b"\r hello world\n "), b"hello world");
/// assert_eq!(trim_ascii(b"  "), b"");
/// assert_eq!(trim_ascii(b""), b"");
/// ```
#[inline]
pub const fn trim_ascii(bytes: &[u8]) -> &[u8] {
    trim_ascii_end(trim_ascii_start(bytes))
}
