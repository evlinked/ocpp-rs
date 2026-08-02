//! Pure decision + construction logic for OCPP 2.0.1 CSMS→CP commands.
//!
//! The 1.6J command set (`Reset`, `RemoteStartTransaction`, `TriggerMessage`, …)
//! is wired into the live [`ActionDispatcher`](ocpp_messages::ActionDispatcher)
//! in [`crate`]'s `ChargePoint`. Their 2.0.1 successors differ in shape and
//! semantics, so a `for_version(V201)` station needs its own handlers. This
//! module ports those command semantics one message at a time, following the
//! same **builder-first → wiring** cadence #419 used for the boot handshake and
//! #424/#425 used for `TransactionEvent`: the *pure* decision and
//! message-construction logic lands here first (unit-testable without a runtime
//! or a socket), and branching the live dispatcher registration on
//! `protocol_version` to route an inbound 2.0.1 CALL to it is the runtime
//! follow-up.
//!
//! ## `Reset`
//!
//! Ports `ocpp.v201.call.Reset` / `ocpp.v201.call_result.Reset`. In 2.0.1 the
//! 1.6J `Hard`/`Soft` distinction is replaced by a [`ResetEnumType`]
//! (`Immediate` / `OnIdle`), the request may target a single `evseId` instead of
//! the whole station, and the response gains a `Scheduled` status (the station
//! accepts but defers the reset until it is idle).
//!
//! ## `TriggerMessage`
//!
//! Ports `ocpp.v201.call.TriggerMessage` / `ocpp.v201.call_result.TriggerMessage`.
//! The CSMS asks the station to *proactively* send one specific message now
//! (e.g. a fresh `BootNotification`, `StatusNotification`, `Heartbeat`, or
//! `TransactionEvent`), optionally scoped to a single `evse`. The station replies
//! [`Accepted`](TriggerMessageStatusEnumType::Accepted) for a message it can
//! produce, or [`NotImplemented`](TriggerMessageStatusEnumType::NotImplemented)
//! for a recognized message it has no way to emit.
//! [`Rejected`](TriggerMessageStatusEnumType::Rejected), like `Reset`'s, is a
//! runtime *capability* outcome (the trigger was accepted in policy but the
//! side-effect could not be enqueued), decided by the slice-5b wiring layer.

use ocpp_types::common::Reason;
use ocpp_types::v16j::ResetType;
use ocpp_types::v201::{
    MessageTriggerEnumType, ResetEnumType, ResetStatusEnumType, StatusInfoType,
    TriggerMessageStatusEnumType,
};

use ocpp_messages::v201::{ResetResponse, TriggerMessageResponse};

/// Decide the [`ResetStatusEnumType`] a `V201` station reports for an inbound
/// `Reset.req`, given the requested [`kind`](ResetEnumType) and whether a
/// transaction is currently in progress on the targeted scope.
///
/// Faithful to OCPP 2.0.1 (Part 2, `Reset`) and the `ResetStatusEnumType`
/// contract:
///
/// - `Immediate` — the station resets as soon as possible, interrupting any
///   ongoing transaction, so the station can always start acting on it:
///   [`Accepted`](ResetStatusEnumType::Accepted).
/// - `OnIdle` — the station must wait until no transaction is ongoing. If it is
///   already idle it acts now ([`Accepted`](ResetStatusEnumType::Accepted));
///   otherwise it accepts but defers, reporting
///   [`Scheduled`](ResetStatusEnumType::Scheduled).
///
/// This is the *policy* decision, which depends only on the request and the
/// station's idle state. [`Rejected`](ResetStatusEnumType::Rejected) is a
/// *capability* outcome — the station accepted the policy but cannot carry the
/// reset out (e.g. its command queue is full) — and is decided by the runtime
/// wiring layer (mirroring the 1.6J handler's `Err(_) => Rejected` on a failed
/// channel send), not here.
#[must_use]
pub fn v201_reset_status(
    kind: ResetEnumType,
    transaction_in_progress: bool,
) -> ResetStatusEnumType {
    match kind {
        ResetEnumType::Immediate => ResetStatusEnumType::Accepted,
        ResetEnumType::OnIdle => {
            if transaction_in_progress {
                ResetStatusEnumType::Scheduled
            } else {
                ResetStatusEnumType::Accepted
            }
        }
    }
}

/// Map a 2.0.1 reset [`kind`](ResetEnumType) onto the internal 1.6J-style stop
/// [`Reason`] the station records when the reset side-effect ends an active
/// transaction.
///
/// The inverse of [`reason_to_v201`](crate::v201_transaction::reason_to_v201)'s
/// intent, which maps `HardReset → ImmediateReset` and `SoftReset → Reboot`: an
/// `Immediate` reset restarts the station at once (a hard reset), while an
/// `OnIdle` reset is a graceful, deferred restart (a soft reset). Total
/// mapping — the two `ResetEnumType` variants are exhaustive.
#[must_use]
pub fn v201_reset_reason(kind: ResetEnumType) -> Reason {
    match kind {
        ResetEnumType::Immediate => Reason::HardReset,
        ResetEnumType::OnIdle => Reason::SoftReset,
    }
}

/// Map a 2.0.1 reset [`kind`](ResetEnumType) onto the 1.6J [`ResetType`] the CP's
/// runtime `perform_reset` side-effect is driven by, selecting the reboot
/// *behavior*:
///
/// - `Immediate → Hard` — reset the station at once (tear the session down and
///   reconnect), the 1.6J "hard" reboot.
/// - `OnIdle → Soft` — a graceful, in-place restart, the 1.6J "soft" reboot.
///
/// This is the wiring layer's companion to [`v201_reset_reason`]: `perform_reset`
/// derives the transaction stop [`Reason`] from the `ResetType` it is given
/// (`Hard → HardReset`, `Soft → SoftReset`), and this mapping is chosen so that
/// derived reason is **identical** to `v201_reset_reason(kind)` — an invariant
/// pinned by [`reset_type_reason_matches_reset_reason`](self). Keeping the pure
/// mapping here (rather than inline in the dispatcher) leaves it unit-testable
/// without a runtime. Total over the two `ResetEnumType` variants.
#[must_use]
pub fn v201_reset_reset_type(kind: ResetEnumType) -> ResetType {
    match kind {
        ResetEnumType::Immediate => ResetType::Hard,
        ResetEnumType::OnIdle => ResetType::Soft,
    }
}

/// Build a schema-valid `Reset.conf` ([`ResetResponse`]).
///
/// Pure constructor mirroring the 1.6J `Ok(ResetResponse { status })` the
/// existing handler returns, extended with the 2.0.1 optional `statusInfo` (a
/// vendor-agnostic `reasonCode` plus human-readable detail — useful, for
/// example, to explain why an `OnIdle` reset was `Scheduled`).
#[must_use]
pub fn v201_reset_response(
    status: ResetStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> ResetResponse {
    ResetResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Decide the [`TriggerMessageStatusEnumType`] a `V201` station reports for an
/// inbound `TriggerMessage.req`, given the [`requested`](MessageTriggerEnumType)
/// message.
///
/// This is the *policy* decision: whether the simulator has any way to produce
/// the requested message. It maps to
/// [`Accepted`](TriggerMessageStatusEnumType::Accepted) for the messages this CP
/// already emits on the live `V201` path —
/// [`BootNotification`](MessageTriggerEnumType::BootNotification),
/// [`Heartbeat`](MessageTriggerEnumType::Heartbeat),
/// [`StatusNotification`](MessageTriggerEnumType::StatusNotification),
/// [`MeterValues`](MessageTriggerEnumType::MeterValues), and
/// [`TransactionEvent`](MessageTriggerEnumType::TransactionEvent) — and to
/// [`NotImplemented`](TriggerMessageStatusEnumType::NotImplemented) for the
/// firmware-, log-, and certificate-signing triggers the simulator has no
/// support for.
///
/// `MeterValues` is `Accepted` at the policy level because the CP produces meter
/// readings; in 2.0.1 those ride inside `TransactionEvent`, so the slice-5b
/// wiring must supply the concrete emit path for a standalone `MeterValues`
/// trigger (tracked with the 5b follow-up) before it can be honored on the wire.
///
/// Total, exhaustive `match` — every [`MessageTriggerEnumType`] variant is
/// classified explicitly, with no wildcard arm, so a future spec-added trigger
/// is a compile error here rather than a silent default.
/// [`Rejected`](TriggerMessageStatusEnumType::Rejected) is a runtime *capability*
/// outcome (a failed side-effect enqueue in slice 5b), not a policy decision, and
/// is intentionally not produced by this function — mirroring
/// [`v201_reset_status`]'s split of policy from capability.
#[must_use]
pub fn v201_trigger_message_status(
    requested: MessageTriggerEnumType,
) -> TriggerMessageStatusEnumType {
    use MessageTriggerEnumType::{
        BootNotification, FirmwareStatusNotification, Heartbeat, LogStatusNotification,
        MeterValues, PublishFirmwareStatusNotification, SignChargingStationCertificate,
        SignCombinedCertificate, SignV2GCertificate, StatusNotification, TransactionEvent,
    };
    match requested {
        // Messages this CP already builds and sends on the live V201 path.
        BootNotification | Heartbeat | StatusNotification | MeterValues | TransactionEvent => {
            TriggerMessageStatusEnumType::Accepted
        }
        // Firmware-, diagnostics-log-, and certificate-signing flows the
        // simulator does not implement: recognized but not triggerable.
        LogStatusNotification
        | FirmwareStatusNotification
        | SignChargingStationCertificate
        | SignV2GCertificate
        | SignCombinedCertificate
        | PublishFirmwareStatusNotification => TriggerMessageStatusEnumType::NotImplemented,
    }
}

/// Build a schema-valid `TriggerMessage.conf` ([`TriggerMessageResponse`]).
///
/// Pure constructor mirroring [`v201_reset_response`]: carries the decided
/// [`status`](TriggerMessageStatusEnumType) plus the optional 2.0.1 `statusInfo`
/// (a vendor-agnostic `reasonCode` and human-readable detail — useful, for
/// example, to name the message a `NotImplemented` trigger declined).
#[must_use]
pub fn v201_trigger_message_response(
    status: TriggerMessageStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> TriggerMessageResponse {
    TriggerMessageResponse {
        status,
        status_info,
        custom_data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_messages::SchemaValidator;

    #[test]
    fn immediate_reset_is_always_accepted() {
        // Immediate interrupts any transaction, so the idle state is irrelevant.
        assert_eq!(
            v201_reset_status(ResetEnumType::Immediate, false),
            ResetStatusEnumType::Accepted
        );
        assert_eq!(
            v201_reset_status(ResetEnumType::Immediate, true),
            ResetStatusEnumType::Accepted
        );
    }

    #[test]
    fn on_idle_reset_is_accepted_when_idle() {
        assert_eq!(
            v201_reset_status(ResetEnumType::OnIdle, false),
            ResetStatusEnumType::Accepted
        );
    }

    #[test]
    fn on_idle_reset_is_scheduled_when_transaction_in_progress() {
        assert_eq!(
            v201_reset_status(ResetEnumType::OnIdle, true),
            ResetStatusEnumType::Scheduled
        );
    }

    #[test]
    fn reset_reason_mapping_is_total() {
        assert_eq!(
            v201_reset_reason(ResetEnumType::Immediate),
            Reason::HardReset
        );
        assert_eq!(v201_reset_reason(ResetEnumType::OnIdle), Reason::SoftReset);
    }

    #[test]
    fn reset_type_mapping_is_total() {
        assert_eq!(
            v201_reset_reset_type(ResetEnumType::Immediate),
            ResetType::Hard
        );
        assert_eq!(
            v201_reset_reset_type(ResetEnumType::OnIdle),
            ResetType::Soft
        );
    }

    /// Consistency invariant between the two mappings the wiring layer relies on:
    /// the runtime drives the side-effect via [`v201_reset_reset_type`] and
    /// `perform_reset` then derives the transaction stop `Reason` from that
    /// `ResetType` (`Hard → HardReset`, `Soft → SoftReset`). That derived reason
    /// must equal [`v201_reset_reason`] for the same kind, so a reset that ends a
    /// live transaction records exactly the reason the pure logic prescribes.
    #[test]
    fn reset_type_reason_matches_reset_reason() {
        // Mirrors `perform_reset`'s ResetType → Reason derivation.
        fn perform_reset_reason(rt: ResetType) -> Reason {
            match rt {
                ResetType::Soft => Reason::SoftReset,
                ResetType::Hard => Reason::HardReset,
            }
        }
        for kind in [ResetEnumType::Immediate, ResetEnumType::OnIdle] {
            assert_eq!(
                perform_reset_reason(v201_reset_reset_type(kind)),
                v201_reset_reason(kind),
                "the reason perform_reset derives from the mapped ResetType must \
                 match v201_reset_reason for {kind:?}"
            );
        }
    }

    #[test]
    fn response_carries_status_and_optional_status_info() {
        let bare = v201_reset_response(ResetStatusEnumType::Accepted, None);
        assert_eq!(bare.status, ResetStatusEnumType::Accepted);
        assert!(bare.status_info.is_none());

        let info = StatusInfoType {
            reason_code: "Deferred".to_string(),
            additional_info: Some("reset deferred until idle".to_string()),
            custom_data: None,
        };
        let scheduled = v201_reset_response(ResetStatusEnumType::Scheduled, Some(info));
        assert_eq!(scheduled.status, ResetStatusEnumType::Scheduled);
        assert_eq!(
            scheduled
                .status_info
                .as_ref()
                .map(|i| i.reason_code.as_str()),
            Some("Deferred")
        );
    }

    /// Wire fidelity: every built `Reset.conf` — with and without `statusInfo`,
    /// across all three status values — must satisfy the bundled OCPP 2.0.1
    /// `ResetResponse` JSON Schema, the same guarantee the CP's version-aware
    /// validator gives on the live path. `validate_call_result` keys on the base
    /// `"Reset"` action (it appends `Response` internally).
    #[test]
    fn built_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "Deferred".to_string(),
            additional_info: Some("reset deferred until idle".to_string()),
            custom_data: None,
        };
        for status in [
            ResetStatusEnumType::Accepted,
            ResetStatusEnumType::Rejected,
            ResetStatusEnumType::Scheduled,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_reset_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator.validate_call_result("Reset", &payload).is_ok(),
                    "built {status:?} ResetResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    /// Every `MessageTriggerEnumType` value the simulator can actually produce
    /// on the live V201 path resolves to `Accepted`.
    #[test]
    fn producible_triggers_are_accepted() {
        for requested in [
            MessageTriggerEnumType::BootNotification,
            MessageTriggerEnumType::Heartbeat,
            MessageTriggerEnumType::StatusNotification,
            MessageTriggerEnumType::MeterValues,
            MessageTriggerEnumType::TransactionEvent,
        ] {
            assert_eq!(
                v201_trigger_message_status(requested),
                TriggerMessageStatusEnumType::Accepted,
                "{requested:?} should be Accepted"
            );
        }
    }

    /// The firmware-, log-, and certificate-signing triggers the simulator has
    /// no way to emit resolve to `NotImplemented` (recognized, not triggerable).
    #[test]
    fn unsupported_triggers_are_not_implemented() {
        for requested in [
            MessageTriggerEnumType::LogStatusNotification,
            MessageTriggerEnumType::FirmwareStatusNotification,
            MessageTriggerEnumType::SignChargingStationCertificate,
            MessageTriggerEnumType::SignV2GCertificate,
            MessageTriggerEnumType::SignCombinedCertificate,
            MessageTriggerEnumType::PublishFirmwareStatusNotification,
        ] {
            assert_eq!(
                v201_trigger_message_status(requested),
                TriggerMessageStatusEnumType::NotImplemented,
                "{requested:?} should be NotImplemented"
            );
        }
    }

    /// The status decision is total: the `Accepted` and `NotImplemented` groups
    /// together cover every `MessageTriggerEnumType` variant, and `Rejected`
    /// (a runtime capability outcome) is never a policy result.
    #[test]
    fn trigger_status_decision_is_total_and_never_rejected() {
        let all = [
            MessageTriggerEnumType::BootNotification,
            MessageTriggerEnumType::LogStatusNotification,
            MessageTriggerEnumType::FirmwareStatusNotification,
            MessageTriggerEnumType::Heartbeat,
            MessageTriggerEnumType::MeterValues,
            MessageTriggerEnumType::SignChargingStationCertificate,
            MessageTriggerEnumType::SignV2GCertificate,
            MessageTriggerEnumType::StatusNotification,
            MessageTriggerEnumType::TransactionEvent,
            MessageTriggerEnumType::SignCombinedCertificate,
            MessageTriggerEnumType::PublishFirmwareStatusNotification,
        ];
        let mut accepted = 0;
        let mut not_implemented = 0;
        for requested in all {
            match v201_trigger_message_status(requested) {
                TriggerMessageStatusEnumType::Accepted => accepted += 1,
                TriggerMessageStatusEnumType::NotImplemented => not_implemented += 1,
                TriggerMessageStatusEnumType::Rejected => {
                    panic!("{requested:?} must not map to the runtime-only Rejected status")
                }
            }
        }
        assert_eq!(accepted, 5, "expected exactly 5 producible triggers");
        assert_eq!(
            not_implemented, 6,
            "expected exactly 6 unsupported triggers"
        );
    }

    #[test]
    fn trigger_response_carries_status_and_optional_status_info() {
        let bare = v201_trigger_message_response(TriggerMessageStatusEnumType::Accepted, None);
        assert_eq!(bare.status, TriggerMessageStatusEnumType::Accepted);
        assert!(bare.status_info.is_none());

        let info = StatusInfoType {
            reason_code: "NotSupported".to_string(),
            additional_info: Some("MeterValues trigger not yet wired".to_string()),
            custom_data: None,
        };
        let declined =
            v201_trigger_message_response(TriggerMessageStatusEnumType::NotImplemented, Some(info));
        assert_eq!(
            declined.status,
            TriggerMessageStatusEnumType::NotImplemented
        );
        assert_eq!(
            declined
                .status_info
                .as_ref()
                .map(|i| i.reason_code.as_str()),
            Some("NotSupported")
        );
    }

    /// Wire fidelity: every built `TriggerMessage.conf` — with and without
    /// `statusInfo`, across all three status values — satisfies the bundled OCPP
    /// 2.0.1 `TriggerMessageResponse` JSON Schema, the same guarantee the CP's
    /// version-aware validator gives on the live path.
    #[test]
    fn built_trigger_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "NotSupported".to_string(),
            additional_info: Some("recognized but not triggerable".to_string()),
            custom_data: None,
        };
        for status in [
            TriggerMessageStatusEnumType::Accepted,
            TriggerMessageStatusEnumType::Rejected,
            TriggerMessageStatusEnumType::NotImplemented,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_trigger_message_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("TriggerMessage", &payload)
                        .is_ok(),
                    "built {status:?} TriggerMessageResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }
}
