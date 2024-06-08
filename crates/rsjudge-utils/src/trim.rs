// SPDX-License-Identifier: Apache-2.0

//! Functions for trimming ASCII whitespace from `[u8]` and `str`.

/// Returns a byte slice with leading ASCII Space bytes removed.
///
/// ASCII Space refers to the byte `0x20`.
///
/// # Examples
///
/// ```
/// use rsjudge_utils::trim_space_start;
/// assert_eq!(trim_space_start(b" \t hello world\n"), b"\t hello world\n");
/// assert_eq!(trim_space_start(b"  "), b"");
/// assert_eq!(trim_space_start(b""), b"");
/// ```
#[inline]
#[must_use = "This function does not modify the input."]
pub const fn trim_space_start(mut bytes: &[u8]) -> &[u8] {
    // Note: A pattern matching based approach (instead of indexing) allows
    // making the function const.
    while let [b' ', rest @ ..] = bytes {
        bytes = rest;
    }
    bytes
}

/// Returns a byte slice with trailing ASCII Space bytes removed.
///
/// ASCII Space refers to the byte `0x20`.
///
/// # Examples
///
/// ```
/// use rsjudge_utils::trim_space_end;
/// assert_eq!(trim_space_end(b"\r hello world\n "), b"\r hello world\n");
/// assert_eq!(trim_space_end(b"  "), b"");
/// assert_eq!(trim_space_end(b""), b"");
/// ```
#[inline]
#[must_use = "This function does not modify the input."]
pub const fn trim_space_end(mut bytes: &[u8]) -> &[u8] {
    // Note: A pattern matching based approach (instead of indexing) allows
    // making the function const.
    while let [rest @ .., b' '] = bytes {
        bytes = rest;
    }
    bytes
}

/// Returns a byte slice with leading and trailing ASCII Space bytes removed.
///
/// ASCII Space refers to the byte `0x20`.
///
/// # Examples
///
/// ```
/// use rsjudge_utils::trim_space;
/// assert_eq!(trim_space(b" hello world "), b"hello world");
/// assert_eq!(trim_space(b"  "), b"");
/// assert_eq!(trim_space(b""), b"");
/// ```
#[inline]
#[must_use = "This function does not modify the input."]
pub const fn trim_space(bytes: &[u8]) -> &[u8] {
    trim_space_end(trim_space_start(bytes))
}
