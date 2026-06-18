//! AutoCharge identifier helpers.
//!
//! **AutoCharge is a de-facto industry extension, not part of the OCPP
//! specification.** It differs from ISO 15118 "Plug & Charge" (standardized,
//! and natively supported by OCPP 2.0.1): on OCPP 1.6J — which has no native
//! Plug & Charge — AutoCharge lets a vehicle start a session with no RFID/app
//! by using its **EV MAC address / EVCCID** (obtained over the HomePlug Green
//! PHY link) as the OCPP `idTag` over the *standard* `Authorize` /
//! `StartTransaction` operations, so it needs no new OCPP messages.
//!
//! The charge point and the back office must agree on the exact wire form of
//! that derived `idTag`, so the normalization lives here in `ocpp-types` where
//! both the CP simulator (`ocpp-cp`) and the CSMS-side recognizer/registry can
//! share it.
//!
//! Canonical form: **12 uppercase hex characters**, separators (`:` / `-`) and
//! surrounding whitespace stripped, with an optional configured vendor/operator
//! prefix prepended. The result must still fit OCPP's `IdToken`
//! (`CiString20Type`), so a derived id longer than 20 characters is rejected.

/// Maximum length of an OCPP 1.6J `IdToken` (`CiString20Type`).
pub const ID_TOKEN_MAX_LEN: usize = 20;

/// Normalize a raw EV MAC address / EVCCID to its canonical AutoCharge form:
/// **12 uppercase hex characters** with `:` / `-` separators and surrounding
/// whitespace removed.
///
/// Returns `None` if the input does not contain exactly 12 hex digits once
/// separators are stripped, or if it contains any non-hex, non-separator
/// character. This is intentionally strict so a malformed identifier never
/// silently turns into a bogus `idTag`.
///
/// ```
/// use ocpp_types::autocharge::normalize_mac;
/// assert_eq!(normalize_mac("aa:bb:cc:dd:ee:ff").as_deref(), Some("AABBCCDDEEFF"));
/// assert_eq!(normalize_mac("AA-BB-CC-DD-EE-FF").as_deref(), Some("AABBCCDDEEFF"));
/// assert_eq!(normalize_mac("AABBCCDDEEFF").as_deref(), Some("AABBCCDDEEFF"));
/// assert_eq!(normalize_mac("AABBCCDDEE"), None); // too short
/// assert_eq!(normalize_mac("AABBCCDDEEFG"), None); // non-hex
/// ```
pub fn normalize_mac(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(12);
    for c in raw.trim().chars() {
        match c {
            ':' | '-' => continue,
            c if c.is_ascii_hexdigit() => out.push(c.to_ascii_uppercase()),
            // Any other character (including embedded whitespace) is invalid.
            _ => return None,
        }
    }
    if out.len() == 12 {
        Some(out)
    } else {
        None
    }
}

/// Derive an AutoCharge `idTag` from a raw EV MAC address / EVCCID, optionally
/// prefixed with a configured vendor/operator string.
///
/// The MAC is normalized via [`normalize_mac`]; the optional `prefix` is
/// prepended verbatim (operators sometimes namespace AutoCharge tags, e.g.
/// `"VID:"`). Returns `None` if the MAC is malformed, or if `prefix + mac`
/// would exceed [`ID_TOKEN_MAX_LEN`] (an `idTag` that won't fit the wire type).
///
/// ```
/// use ocpp_types::autocharge::derive_autocharge_id_tag;
/// assert_eq!(
///     derive_autocharge_id_tag("aa:bb:cc:dd:ee:ff", None).as_deref(),
///     Some("AABBCCDDEEFF")
/// );
/// assert_eq!(
///     derive_autocharge_id_tag("aabbccddeeff", Some("VID")).as_deref(),
///     Some("VIDAABBCCDDEEFF")
/// );
/// ```
pub fn derive_autocharge_id_tag(mac: &str, prefix: Option<&str>) -> Option<String> {
    let normalized = normalize_mac(mac)?;
    let id_tag = match prefix {
        Some(p) if !p.is_empty() => format!("{p}{normalized}"),
        _ => normalized,
    };
    if id_tag.len() <= ID_TOKEN_MAX_LEN {
        Some(id_tag)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_plain_12_hex() {
        assert_eq!(
            normalize_mac("AABBCCDDEEFF").as_deref(),
            Some("AABBCCDDEEFF")
        );
    }

    #[test]
    fn uppercases_mixed_case() {
        assert_eq!(
            normalize_mac("aAbBcCdDeEfF").as_deref(),
            Some("AABBCCDDEEFF")
        );
    }

    #[test]
    fn strips_colon_and_dash_separators() {
        assert_eq!(
            normalize_mac("aa:bb:cc:dd:ee:ff").as_deref(),
            Some("AABBCCDDEEFF")
        );
        assert_eq!(
            normalize_mac("AA-BB-CC-DD-EE-FF").as_deref(),
            Some("AABBCCDDEEFF")
        );
        assert_eq!(
            normalize_mac("  aa:bb-cc:dd-ee:ff  ").as_deref(),
            Some("AABBCCDDEEFF")
        );
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(normalize_mac("AABBCCDDEE"), None); // 10 hex
        assert_eq!(normalize_mac("AABBCCDDEEFFAA"), None); // 14 hex
        assert_eq!(normalize_mac(""), None);
    }

    #[test]
    fn rejects_non_hex() {
        assert_eq!(normalize_mac("AABBCCDDEEFG"), None); // trailing 'G'
        assert_eq!(normalize_mac("ZZBBCCDDEEFF"), None);
        assert_eq!(normalize_mac("aa bb cc dd ee ff"), None); // embedded spaces
    }

    #[test]
    fn derives_without_prefix() {
        assert_eq!(
            derive_autocharge_id_tag("aa:bb:cc:dd:ee:ff", None).as_deref(),
            Some("AABBCCDDEEFF")
        );
        // An empty prefix behaves like no prefix.
        assert_eq!(
            derive_autocharge_id_tag("aabbccddeeff", Some("")).as_deref(),
            Some("AABBCCDDEEFF")
        );
    }

    #[test]
    fn derives_with_prefix() {
        assert_eq!(
            derive_autocharge_id_tag("aabbccddeeff", Some("VID")).as_deref(),
            Some("VIDAABBCCDDEEFF")
        );
    }

    #[test]
    fn rejects_prefix_overflowing_id_token() {
        // 12 hex + 12-char prefix = 24 > 20, won't fit CiString20Type.
        assert_eq!(
            derive_autocharge_id_tag("aabbccddeeff", Some("LONGPREFIX12")),
            None
        );
        // 8-char prefix + 12 hex = 20, exactly fits.
        assert_eq!(
            derive_autocharge_id_tag("aabbccddeeff", Some("PREFIX12")).as_deref(),
            Some("PREFIX12AABBCCDDEEFF")
        );
    }

    #[test]
    fn rejects_malformed_mac_in_derivation() {
        assert_eq!(derive_autocharge_id_tag("not-a-mac", Some("VID")), None);
        assert_eq!(derive_autocharge_id_tag("AABBCCDDEE", None), None);
    }
}
