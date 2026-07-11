//! `ChargePoint.call()` unique-id contract — ports the mobilityhouse/ocpp
//! reference's charge-point call tests
//! ([`tests/v201/test_v201_charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/v201/test_v201_charge_point.py)
//! and the v16 analog
//! [`tests/v16/test_v16_charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/tests/v16/test_v16_charge_point.py)),
//! backed by the `ChargePoint.call()` logic in
//! [`ocpp/charge_point.py`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/charge_point.py).
//!
//! ## What the reference pins
//!
//! The reference's `call()` has a two-branch **unique-id contract**:
//!
//! ```python
//! unique_id = (
//!     unique_id if unique_id is not None else str(self._unique_id_generator())
//! )
//! ```
//! (`_unique_id_generator` defaults to `uuid.uuid4`.) Two tests pin the branches:
//!
//!   - `test_call_with_unique_id_should_return_same_id` — a caller-supplied
//!     `unique_id` (`"12345"`) is used **verbatim** on the outgoing CALL, so the
//!     caller can correlate the CALLRESULT it will later receive.
//!   - `test_call_without_unique_id_should_return_a_random_value` — an omitted id
//!     is **generated fresh** (a random UUID); successive calls differ.
//!
//! ## How the Rust model maps
//!
//! ocpp-rs has no stateful `ChargePoint.call()`; the equivalent surface is the
//! CALL-framing constructors. The auto-generate branch is
//! [`utils::create_call`] / [`CallMessage::new`] (both mint a fresh UUIDv4); the
//! caller-supplied branch is [`utils::create_call_with_id`] /
//! [`CallMessage::with_id`], which uses the supplied id exactly as given. This
//! suite pins both branches through that public API, exactly as an embedding
//! CSMS/CP correlating an outgoing CALL against its own id would exercise it.
//!
//! **The framing is version-generic.** [`CallMessage`] carries no OCPP-version
//! marker, so one suite covers both 1.6J and 2.0.1: the caller-supplied-id
//! branch is pinned with a v16 `BootNotification` (porting
//! `test_v16_charge_point.py`) *and* a v201 `BootNotification` (porting
//! `test_v201_charge_point.py`), demonstrating the id contract holds identically
//! regardless of the message version.

use ocpp_messages::utils;
use ocpp_messages::CallMessage;
use ocpp_messages::{v16j, v201};
use ocpp_types::v201::{BootReasonEnumType, ChargingStationType};

/// The caller-supplied id both reference tests use (`expected_unique_id = "12345"`).
const SUPPLIED_ID: &str = "12345";

fn v16_boot_request() -> v16j::BootNotificationRequest {
    v16j::BootNotificationRequest {
        charge_point_vendor: "VendorX".to_string(),
        charge_point_model: "ModelY".to_string(),
        charge_point_serial_number: None,
        charge_box_serial_number: None,
        firmware_version: None,
        iccid: None,
        imsi: None,
        meter_type: None,
        meter_serial_number: None,
    }
}

fn v201_boot_request() -> v201::BootNotificationRequest {
    v201::BootNotificationRequest {
        charging_station: ChargingStationType {
            vendor_name: "VendorX".to_string(),
            model: "ModelY".to_string(),
            serial_number: None,
            firmware_version: None,
            modem: None,
            custom_data: None,
        },
        reason: BootReasonEnumType::PowerUp,
        custom_data: None,
    }
}

// ─── caller-supplied id → used verbatim ─────────────────────────────────────

/// Ports `tests/v16/test_v16_charge_point.py::test_call_with_unique_id_should_return_same_id`.
/// A caller-supplied id survives verbatim onto a v16 CALL frame.
#[test]
fn v16_call_with_unique_id_uses_it_verbatim() {
    let call = utils::create_call_with_id(SUPPLIED_ID.to_string(), v16_boot_request()).unwrap();

    assert_eq!(call.unique_id, SUPPLIED_ID);
    assert_eq!(call.action, "BootNotification");
}

/// Ports `tests/v201/test_v201_charge_point.py::test_call_with_unique_id_should_return_same_id`.
/// The same contract holds for a v201 CALL frame — the framing is version-generic.
#[test]
fn v201_call_with_unique_id_uses_it_verbatim() {
    let call = utils::create_call_with_id(SUPPLIED_ID.to_string(), v201_boot_request()).unwrap();

    assert_eq!(call.unique_id, SUPPLIED_ID);
    assert_eq!(call.action, "BootNotification");
}

/// The supplied id is used *exactly* as given — no normalization or truncation,
/// even for a value that violates the schema's `maxLength: 36` (that bound is a
/// wire/schema-validation concern, enforced there, not in the constructor). This
/// pins the "used verbatim" half of the contract against silent mangling.
#[test]
fn supplied_id_is_not_normalized_or_truncated() {
    let long_id = "x".repeat(64);
    let call = CallMessage::with_id(long_id.clone(), "BootNotification".to_string(), ()).unwrap();

    assert_eq!(call.unique_id, long_id);
    assert_eq!(call.unique_id.len(), 64);
}

// ─── omitted id → fresh random value ────────────────────────────────────────

/// Ports the v16 + v201 `test_call_without_unique_id_should_return_a_random_value`
/// (and the intent of `test_generate_message_id`): the auto-generate branch mints
/// a fresh, non-empty id, and two successive calls differ. This is unchanged from
/// the existing [`utils::create_call`] default — pinned here at the call boundary
/// so the two-branch contract lives in one place.
#[test]
fn call_without_unique_id_generates_a_fresh_random_value() {
    let call1 = utils::create_call(v201_boot_request()).unwrap();
    let call2 = utils::create_call(v201_boot_request()).unwrap();

    assert!(!call1.unique_id.is_empty());
    assert!(!call2.unique_id.is_empty());
    assert_ne!(
        call1.unique_id, call2.unique_id,
        "successive auto-generated ids must differ (random UUIDv4)"
    );

    // The auto-generate default holds for v16 framing too.
    let v16_call = utils::create_call(v16_boot_request()).unwrap();
    assert!(!v16_call.unique_id.is_empty());
}
