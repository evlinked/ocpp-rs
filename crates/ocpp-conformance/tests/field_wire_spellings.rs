//! Acronym / edge-case serde field wire-spelling conformance suite — ports the
//! `test_camel_to_snake_case` / `test_snake_to_camel_case` tables from
//! [`tests/test_charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/test_charge_point.py).
//!
//! ## Why this suite exists
//!
//! The Python reference converts field names between `camelCase` (wire) and
//! `snake_case` (dataclass) at runtime with `camel_to_snake_case` /
//! `snake_to_camel_case` ([`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py)).
//! The interesting rows those two tables pin are the *acronyms* a naive splitter
//! gets wrong — `fullSoC`, `responderURL`, `ocppCSMSURL` ⇄ `ocppCsmsUrl`,
//! `CSMSRootCertificate`, and friends.
//!
//! Rust has no runtime converter: every field's wire spelling is fixed once, by
//! its `#[serde(rename = "…")]`. So the faithful port is to pin, for each
//! modelled acronym-heavy field, the *exact* camelCase key it serializes to (and
//! that the same key deserializes back) — a byte-for-byte guard against a silent
//! serde-rename drift that a round-trip through our own types alone would not
//! catch (serialize and deserialize would drift together and still agree).
//!
//! ## Coverage
//!
//! Only fields **modelled in `ocpp-types`** can be pinned. Verified present and
//! covered here:
//!
//! | reference table row | wire key | Rust field / variant |
//! |---|---|---|
//! | `full_soc` ⇄ `fullSoC` | `fullSoC` | [`DCChargingParametersType::full_soc`] |
//! | `responder_url` ⇄ `responderURL` | `responderURL` | [`OCSPRequestDataType::responder_url`] |
//! | `ocpp_csms_url` ⇄ `ocppCsmsUrl` | `ocppCsmsUrl` | [`NetworkConnectionProfileType::ocpp_csms_url`] |
//! | `csms_root_certificate` ⇄ `CSMSRootCertificate` | `CSMSRootCertificate` | [`InstallCertificateUseEnumType::CSMSRootCertificate`] |
//!
//! The SoC-acronym family (`stateOfCharge`, `fullSoC`, `bulkSoC`) is pinned
//! together, since they share the tricky `SoC` casing the reference table calls
//! out (`full_soc` ⇄ `fullSoC`).
//!
//! ## Residual (not modelled — cannot be pinned yet)
//!
//! `webSocketPingInterval`, `signV2GCertificate`, `v2gCertificateInstallationEnabled`,
//! `evMinV2XEnergyRequest`, `v2xChargingCtrlr`, `SoCLimitReached` — these fields
//! are not modelled in `ocpp-types` today (V2X/V2G are newer reference
//! additions). They can only be pinned once modelled; noted here so a future
//! modelling pass can pick them up.

use serde_json::{from_value, json, to_value};

use ocpp_types::v201::{
    DCChargingParametersType, GetCertificateIdUseEnumType, HashAlgorithmEnumType,
    InstallCertificateUseEnumType, NetworkConnectionProfileType, OCPPInterfaceEnumType,
    OCPPTransportEnumType, OCPPVersionEnumType, OCSPRequestDataType,
};

/// `full_soc` must serialize to the acronym-cased wire key `fullSoC` — the row
/// the reference's `test_camel_to_snake_case` / `test_snake_to_camel_case`
/// tables both call out (`{"fullSoC": 100}` ⇄ `{"full_soc": 100}`). The whole
/// SoC family (`stateOfCharge`, `fullSoC`, `bulkSoC`) is pinned together.
#[test]
fn soc_family_uses_the_acronym_wire_casing() {
    let value = DCChargingParametersType {
        ev_max_current: 16,
        ev_max_voltage: 400,
        energy_amount: None,
        ev_max_power: None,
        state_of_charge: Some(20),
        ev_energy_capacity: None,
        full_soc: Some(80),
        bulk_soc: Some(95),
        custom_data: None,
    };

    let wire = to_value(&value).expect("serialize");
    assert_eq!(
        wire,
        json!({
            "evMaxCurrent": 16,
            "evMaxVoltage": 400,
            "stateOfCharge": 20,
            "fullSoC": 80,
            "bulkSoC": 95,
        }),
        "SoC-family wire keys drifted from their acronym casing",
    );

    // Deserialize direction must accept the exact acronym keys (guards against
    // a rename that only agrees with itself).
    let back: DCChargingParametersType = from_value(wire).expect("deserialize");
    assert_eq!(back, value);
}

/// `responder_url` must serialize to `responderURL` — the reference row
/// `{"responderURL": "foo.com"}` ⇄ `{"responder_url": "foo.com"}`.
#[test]
fn responder_url_uses_the_url_acronym_wire_casing() {
    let value = OCSPRequestDataType {
        hash_algorithm: HashAlgorithmEnumType::Sha256,
        issuer_name_hash: "abc".to_string(),
        issuer_key_hash: "def".to_string(),
        serial_number: "01".to_string(),
        responder_url: "https://ocsp.example.com".to_string(),
        custom_data: None,
    };

    let wire = to_value(&value).expect("serialize");
    assert_eq!(
        wire,
        json!({
            "hashAlgorithm": "SHA256",
            "issuerNameHash": "abc",
            "issuerKeyHash": "def",
            "serialNumber": "01",
            "responderURL": "https://ocsp.example.com",
        }),
        "responderURL wire key drifted from its acronym casing",
    );

    let back: OCSPRequestDataType = from_value(wire).expect("deserialize");
    assert_eq!(back, value);
}

/// `ocpp_csms_url` must serialize to `ocppCsmsUrl`. This mirrors the exact
/// payload the reference's `test_nested_remove_nones` builds
/// (`ocpp_csms_url="wss://localhost:9000"`, `message_timeout=60`,
/// `security_profile=1`), so the whole `NetworkConnectionProfileType` wire
/// object is pinned, not just the acronym key.
#[test]
fn ocpp_csms_url_uses_the_csms_acronym_wire_casing() {
    let value = NetworkConnectionProfileType {
        ocpp_version: OCPPVersionEnumType::Ocpp20,
        ocpp_transport: OCPPTransportEnumType::Json,
        ocpp_csms_url: "wss://localhost:9000".to_string(),
        message_timeout: 60,
        security_profile: 1,
        ocpp_interface: OCPPInterfaceEnumType::Wired0,
        apn: None,
        vpn: None,
        custom_data: None,
    };

    let wire = to_value(&value).expect("serialize");
    assert_eq!(
        wire,
        json!({
            "ocppVersion": "OCPP20",
            "ocppTransport": "JSON",
            "ocppCsmsUrl": "wss://localhost:9000",
            "messageTimeout": 60,
            "securityProfile": 1,
            "ocppInterface": "Wired0",
        }),
        "ocppCsmsUrl wire key drifted from its acronym casing",
    );

    let back: NetworkConnectionProfileType = from_value(wire).expect("deserialize");
    assert_eq!(back, value);
}

/// The `CSMSRootCertificate` enum member must serialize to the PascalCase
/// acronym wire string `"CSMSRootCertificate"` — the reference row
/// `{"CSMSRootCertificate": "foo.com"}` ⇄ `{"csms_root_certificate": "foo.com"}`.
/// Pinned in both certificate enums that carry the member.
#[test]
fn csms_root_certificate_uses_the_pascalcase_acronym_wire_string() {
    assert_eq!(
        to_value(InstallCertificateUseEnumType::CSMSRootCertificate).expect("serialize"),
        json!("CSMSRootCertificate"),
    );
    let back: InstallCertificateUseEnumType =
        from_value(json!("CSMSRootCertificate")).expect("deserialize");
    assert_eq!(back, InstallCertificateUseEnumType::CSMSRootCertificate);

    // Same acronym in the superset enum used by GetInstalledCertificateIds.
    assert_eq!(
        to_value(GetCertificateIdUseEnumType::CSMSRootCertificate).expect("serialize"),
        json!("CSMSRootCertificate"),
    );
    let back: GetCertificateIdUseEnumType =
        from_value(json!("CSMSRootCertificate")).expect("deserialize");
    assert_eq!(back, GetCertificateIdUseEnumType::CSMSRootCertificate);
}
