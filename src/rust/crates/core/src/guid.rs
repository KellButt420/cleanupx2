/// Validates a braced GUID such as an MSI product code and returns it in the
/// canonical upper-case form Windows Installer uses. Anything else, including
/// unbraced or padded input, is rejected.
pub fn normalize_braced_guid(value: &str) -> Option<String> {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];

    let inner = value.strip_prefix('{')?.strip_suffix('}')?;
    let mut parts = inner.split('-');
    for expected in GROUPS {
        let part = parts.next()?;
        if part.len() != expected || !part.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
    }
    if parts.next().is_some() {
        return None;
    }
    Some(format!("{{{}}}", inner.to_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::normalize_braced_guid;

    #[test]
    fn accepts_and_uppercases_valid_guids() {
        assert_eq!(
            normalize_braced_guid("{a1b2c3d4-0000-1111-2222-abcdefabcdef}").as_deref(),
            Some("{A1B2C3D4-0000-1111-2222-ABCDEFABCDEF}")
        );
    }

    #[test]
    fn rejects_malformed_guids() {
        for bad in [
            "",
            "A1B2C3D4-0000-1111-2222-ABCDEFABCDEF",
            "{A1B2C3D4-0000-1111-2222-ABCDEFABCDEF",
            "{A1B2C3D4-0000-1111-2222-ABCDEFABCDE}",
            "{A1B2C3D4-0000-1111-2222-ABCDEFABCDEFF}",
            "{A1B2C3D4-0000-1111-2222-ABCDEFABCDEG}",
            "{A1B2C3D4-0000-1111-2222-ABCD-EFABCDEF}",
            " {A1B2C3D4-0000-1111-2222-ABCDEFABCDEF}",
        ] {
            assert_eq!(normalize_braced_guid(bad), None, "{bad:?}");
        }
    }
}
