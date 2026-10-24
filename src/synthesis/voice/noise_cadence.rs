pub(super) fn retained_only(version: &str) -> bool {
    version.bytes().filter(u8::is_ascii_digit).fold(0_u32, |value, digit| {
        value.saturating_mul(10).saturating_add(u32::from(digit - b'0'))
    }) > 203
}
