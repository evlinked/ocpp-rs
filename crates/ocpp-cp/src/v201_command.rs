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
//!
//! ## `ChangeAvailability`
//!
//! Ports `ocpp.v201.call.ChangeAvailability` /
//! `ocpp.v201.call_result.ChangeAvailability`. The CSMS asks the station (or a
//! single `evse`, whole-station when `evse` is omitted) to become
//! [`Operative`](OperationalStatusEnumType::Operative) or
//! [`Inoperative`](OperationalStatusEnumType::Inoperative). The station replies
//! [`Accepted`](ChangeAvailabilityStatusEnumType::Accepted) when it can apply the
//! change now, or [`Scheduled`](ChangeAvailabilityStatusEnumType::Scheduled) when
//! a transaction is in progress and the change must wait until the connector is
//! idle — directly analogous to the `Reset(OnIdle)` → `Scheduled` decision.
//! [`Rejected`](ChangeAvailabilityStatusEnumType::Rejected), like the others', is
//! a runtime *capability* outcome decided by the slice-6b wiring layer.
//!
//! ## `RequestStartTransaction`
//!
//! Ports `ocpp.v201.call.RequestStartTransaction` /
//! `ocpp.v201.call_result.RequestStartTransaction` — the 2.0.1 successor to the
//! 1.6J `RemoteStartTransaction` the CP already answers on the `V16J` path. A
//! CSMS remotely starts a session for a driver's `idToken`, optionally targeting
//! a single `evseId`. The station decides whether it can honor the start —
//! [`Accepted`](RequestStartStopStatusEnumType::Accepted) for a targeted EVSE
//! that exists and is free to charge,
//! [`Rejected`](RequestStartStopStatusEnumType::Rejected) for a busy, unknown, or
//! structurally-invalid EVSE — before actually beginning the transaction,
//! exactly as the 1.6J handler does. A missing `evseId` defaults to EVSE 1,
//! matching the 1.6J handler's `connector_id.unwrap_or(1)`. The
//! `remoteStartId` / `groupIdToken` / `chargingProfile` fields are carried
//! through by the later wire slice (7b), not decided here.
//!
//! ## `UnlockConnector`
//!
//! Ports `ocpp.v201.call.UnlockConnector` / `ocpp.v201.call_result.UnlockConnector`
//! — the 2.0.1 successor to the 1.6J `UnlockConnector` the CP already answers on
//! the `V16J` path. Where 1.6J addressed a flat `connectorId`, 2.0.1 names both an
//! `evseId` and a `connectorId`. An operator sends it when a driver's cable is
//! stuck: the station refuses to release the cable while a transaction is still
//! authorized on the targeted connector
//! ([`OngoingAuthorizedTransaction`](UnlockStatusEnumType::OngoingAuthorizedTransaction)),
//! reports [`UnknownConnector`](UnlockStatusEnumType::UnknownConnector) for a
//! connector it does not have, and otherwise attempts the unlock, reporting the
//! mechanical [`Unlocked`](UnlockStatusEnumType::Unlocked) /
//! [`UnlockFailed`](UnlockStatusEnumType::UnlockFailed) outcome off the shared
//! [`UnlockConnectorOutcome`] seam the 1.6J handler
//! already uses. Note 2.0.1's `UnlockStatusEnumType` drops 1.6J's `NotSupported`,
//! so a connector with no controllable lock folds to `UnlockFailed` here (see
//! [`v201_unlock_status`]). The refusal is scoped to a *still-authorized*
//! session: a live-but-deauthorized transaction is instead stoppable, and the
//! wiring layer stops it first (reason `UnlockCommand`) before releasing the
//! cable — mirroring the 1.6J stop-then-unlock. Resolving the target against the
//! live topology, reading authorization state, and driving the actuator off the
//! CALL path is the wiring layer's job.

use ocpp_types::common::Reason;
use ocpp_types::v16j::ResetType;
use ocpp_types::v201::{
    ChangeAvailabilityStatusEnumType, ChargingProfilePurposeEnumType,
    ChargingProfileStatusEnumType, ChargingProfileType, ClearChargingProfileStatusEnumType,
    ClearChargingProfileType, MessageTriggerEnumType, OperationalStatusEnumType,
    RequestStartStopStatusEnumType, ResetEnumType, ResetStatusEnumType, StatusInfoType,
    TriggerMessageStatusEnumType, UnlockStatusEnumType,
};

use ocpp_messages::v201::{
    ChangeAvailabilityResponse, ClearChargingProfileResponse, RequestStartTransactionResponse,
    RequestStopTransactionResponse, ResetResponse, SetChargingProfileResponse,
    TriggerMessageResponse, UnlockConnectorResponse,
};

use crate::UnlockConnectorOutcome;

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

/// Decide the [`ChangeAvailabilityStatusEnumType`] a `V201` station reports for
/// an inbound `ChangeAvailability.req`, given the requested
/// [`target`](OperationalStatusEnumType) availability and whether a transaction
/// is currently in progress on the targeted scope.
///
/// Faithful to OCPP 2.0.1 (Part 2, `ChangeAvailability`): the station applies an
/// availability change only while the affected EVSE/connector is idle. So —
/// independent of the *direction* of the change (`Operative` or `Inoperative`):
///
/// - **Idle** — the change takes effect immediately:
///   [`Accepted`](ChangeAvailabilityStatusEnumType::Accepted).
/// - **Transaction in progress** — the station accepts the change but defers it
///   until the transaction finishes, so a paying driver is never cut off:
///   [`Scheduled`](ChangeAvailabilityStatusEnumType::Scheduled).
///
/// The direction does not change the decision: a busy connector cannot be taken
/// `Inoperative` mid-charge, and per the spec a change to `Operative` that
/// coincides with an ongoing transaction is likewise reported `Scheduled` (it is
/// applied at the same idle boundary) rather than silently taking effect — the
/// station reports one honest "deferred until idle" status for both.
///
/// This is the *policy* decision, depending only on the request and the station's
/// idle state — the same policy/capability split
/// [`v201_reset_status`] uses.
/// [`Rejected`](ChangeAvailabilityStatusEnumType::Rejected) is a runtime
/// *capability* outcome (the change was accepted in policy but the side-effect
/// could not be enqueued) decided by the slice-6b wiring layer, not here.
///
/// Total, exhaustive `match` — both [`OperationalStatusEnumType`] variants are
/// classified explicitly, with no wildcard arm, so a future spec-added
/// operational status is a compile error here rather than a silent default.
#[must_use]
pub fn v201_change_availability_status(
    target: OperationalStatusEnumType,
    transaction_in_progress: bool,
) -> ChangeAvailabilityStatusEnumType {
    match target {
        OperationalStatusEnumType::Operative | OperationalStatusEnumType::Inoperative => {
            if transaction_in_progress {
                ChangeAvailabilityStatusEnumType::Scheduled
            } else {
                ChangeAvailabilityStatusEnumType::Accepted
            }
        }
    }
}

/// Build a schema-valid `ChangeAvailability.conf` ([`ChangeAvailabilityResponse`]).
///
/// Pure constructor mirroring [`v201_reset_response`]: carries the decided
/// [`status`](ChangeAvailabilityStatusEnumType) plus the optional 2.0.1
/// `statusInfo` (a vendor-agnostic `reasonCode` and human-readable detail —
/// useful, for example, to explain why an availability change was `Scheduled`).
#[must_use]
pub fn v201_change_availability_response(
    status: ChangeAvailabilityStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> ChangeAvailabilityResponse {
    ChangeAvailabilityResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Decide the [`RequestStartStopStatusEnumType`] a `V201` station reports for an
/// inbound `RequestStartTransaction.req`, given the requested
/// [`evse_id`](ocpp_messages::v201::RequestStartTransactionRequest::evse_id)
/// target and whether that targeted EVSE is currently free to start charging.
///
/// Faithful to OCPP 2.0.1 (Part 2, `RequestStartTransaction`) and a direct
/// port of the 1.6J `RemoteStartTransaction` handler's decision in
/// [`crate`]'s `ChargePoint`:
///
/// - A missing `evseId` defaults to **EVSE 1**, mirroring the 1.6J handler's
///   `connector_id.unwrap_or(1)`.
/// - An `evseId` of `0` or negative is not a chargeable EVSE — 2.0.1 EVSE ids
///   are 1-based (`0` addresses the whole station, never a physical EVSE to
///   start a session on) — so it is
///   [`Rejected`](RequestStartStopStatusEnumType::Rejected) structurally,
///   mirroring the 1.6J handler's `ConnectorId::new(..)` `Err` arm for
///   connector `0` / out of range.
/// - Otherwise the targeted EVSE's chargeability decides:
///   [`Accepted`](RequestStartStopStatusEnumType::Accepted) when it exists and
///   is free to charge, [`Rejected`](RequestStartStopStatusEnumType::Rejected)
///   when it is busy or unknown. Both the busy and unknown cases collapse to
///   `evse_available == false`, exactly as the 1.6J handler folds a
///   known-but-busy connector and an unknown connector id into one `Rejected`
///   arm.
///
/// This is the *pure* decision, depending only on the request target and the
/// station's chargeability read — no runtime handles, so it is unit-testable in
/// isolation. Resolving `evse_id` to a concrete EVSE, reading its live status,
/// and queuing the local `StartTransaction` off the CALL path is the slice-7b
/// wiring layer's job. `RequestStartStopStatusEnumType` has exactly `Accepted` /
/// `Rejected` (no `Scheduled`), so — unlike `Reset` / `ChangeAvailability` —
/// this is a clean two-way split with no deferred outcome.
#[must_use]
pub fn v201_request_start_status(
    evse_id: Option<i32>,
    evse_available: bool,
) -> RequestStartStopStatusEnumType {
    // Default a missing evseId to EVSE 1 (1.6J `connector_id.unwrap_or(1)`).
    let target = evse_id.unwrap_or(1);
    // EVSE 0 / negative is not a chargeable EVSE: reject before consulting
    // availability, mirroring the 1.6J `ConnectorId::new` Err arm.
    if target < 1 {
        return RequestStartStopStatusEnumType::Rejected;
    }
    if evse_available {
        RequestStartStopStatusEnumType::Accepted
    } else {
        // Busy or unknown EVSE — both surface as "not free to charge".
        RequestStartStopStatusEnumType::Rejected
    }
}

/// Build a schema-valid `RequestStartTransaction.conf`
/// ([`RequestStartTransactionResponse`]).
///
/// Pure constructor mirroring [`v201_reset_response`]: carries the decided
/// [`status`](RequestStartStopStatusEnumType) plus the optional 2.0.1
/// `statusInfo` (a vendor-agnostic `reasonCode` and human-readable detail —
/// useful, for example, to explain why a remote start was `Rejected`).
///
/// `transactionId` is left `None`: it is only set when the transaction had
/// *already* started (e.g. the cable was plugged in first) and the station is
/// reporting the id of that already-running transaction — a live-state concern
/// the slice-7b wiring layer owns, not this pure builder.
#[must_use]
pub fn v201_request_start_response(
    status: RequestStartStopStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> RequestStartTransactionResponse {
    RequestStartTransactionResponse {
        status,
        status_info,
        transaction_id: None,
        custom_data: None,
    }
}

/// Decide the [`RequestStartStopStatusEnumType`] a `V201` station reports for an
/// inbound `RequestStopTransaction.req`, given the requested
/// [`transaction_id`](ocpp_messages::v201::RequestStopTransactionRequest::transaction_id)
/// and the ids of every transaction the station currently has live.
///
/// Faithful to OCPP 2.0.1 (Part 2, `RequestStopTransaction`) and a direct port
/// of the 1.6J `RemoteStopTransaction` handler's decision in [`crate`]'s
/// `ChargePoint`:
///
/// - [`Accepted`](RequestStartStopStatusEnumType::Accepted) iff `requested`
///   equals one of `live_transaction_ids` — the CSMS named a transaction this
///   station is actually running, so it can honor the stop. This mirrors the
///   1.6J handler's `active_transactions.contains_key(&transaction_id)` guard,
///   re-expressed over the 2.0.1 string `transactionId`.
/// - [`Rejected`](RequestStartStopStatusEnumType::Rejected) otherwise — an
///   unknown id, *or* an idle station (`live_transaction_ids` empty), both fold
///   to "no such live transaction to stop", exactly as the 1.6J handler folds
///   them into one `Rejected` arm.
///
/// Matching is exact string equality, not a numeric parse: a 2.0.1
/// `transactionId` is an opaque string (here the station-minted decimal), and a
/// CSMS echoes back the exact id the station issued — so a non-canonical
/// spelling (`"01"`, whitespace, a huge value) simply fails to match and is
/// `Rejected`, never parsed and never panicking. Taking the *set* of live ids
/// (rather than a single `Option`) is deliberate: a station can run one
/// transaction per EVSE concurrently, so the requested id is checked against all
/// of them.
///
/// This is the *pure* decision, depending only on the requested id and the
/// station's live-id read — no runtime handles, so it is unit-testable in
/// isolation. Resolving the live ids from the transaction table and queuing the
/// stop off the CALL path is the wiring layer's job.
/// `RequestStartStopStatusEnumType` has exactly `Accepted` / `Rejected`, so —
/// like [`v201_request_start_status`] — this is a clean two-way split.
#[must_use]
pub fn v201_request_stop_status(
    requested: &str,
    live_transaction_ids: &[&str],
) -> RequestStartStopStatusEnumType {
    if live_transaction_ids.contains(&requested) {
        RequestStartStopStatusEnumType::Accepted
    } else {
        // Unknown id or idle station — nothing to stop.
        RequestStartStopStatusEnumType::Rejected
    }
}

/// Build a schema-valid `RequestStopTransaction.conf`
/// ([`RequestStopTransactionResponse`]).
///
/// Pure constructor mirroring [`v201_request_start_response`]: carries the
/// decided [`status`](RequestStartStopStatusEnumType) plus the optional 2.0.1
/// `statusInfo` (a vendor-agnostic `reasonCode` and human-readable detail —
/// useful, for example, to explain why a stop was `Rejected`).
#[must_use]
pub fn v201_request_stop_response(
    status: RequestStartStopStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> RequestStopTransactionResponse {
    RequestStopTransactionResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Decide the [`UnlockStatusEnumType`] a `V201` station reports for an inbound
/// `UnlockConnector.req` targeting `(evse_id, connector_id)`, given whether that
/// target is a known connector, whether a live authorized transaction is on it,
/// and the station's injectable mechanical [`UnlockConnectorOutcome`].
///
/// Faithful to OCPP 2.0.1 (Part 2, `UnlockConnector`) and a direct port of the
/// 1.6J `UnlockConnector` handler's decision in [`crate`]'s `ChargePoint`,
/// re-expressed against the richer 2.0.1 [`UnlockStatusEnumType`]:
///
/// - **Structurally-invalid target** — an `evse_id` or `connector_id` below `1`
///   addresses no physical connector (2.0.1 ids are 1-based; `0` is the whole
///   station, never a connector to unlock) →
///   [`UnknownConnector`](UnlockStatusEnumType::UnknownConnector). This mirrors
///   the 1.6J handler's `ConnectorId::new(..)` `Err` arm for connector `0` / out
///   of range, and — like [`v201_request_start_status`]'s structural guard — is
///   decided *before* the knownness read, so even a `connector_known == true`
///   cannot rescue an invalid id.
/// - **Unmapped target** — a structurally-valid id the station has no connector
///   for (`connector_known == false`) →
///   [`UnknownConnector`](UnlockStatusEnumType::UnknownConnector), the 2.0.1
///   analogue of the 1.6J "connector this CP does not have" arm (which, lacking a
///   dedicated status, answered `UnlockFailed`).
/// - **Ongoing authorized transaction** — a live transaction that is *still
///   authorized* (`transaction_active && transaction_authorized`) on the
///   targeted connector, so the cable must not be released →
///   [`OngoingAuthorizedTransaction`](UnlockStatusEnumType::OngoingAuthorizedTransaction).
///   This is a 2.0.1 refinement with no 1.6J equivalent: the 1.6J handler *always*
///   stops the transaction first and then unlocks, whereas 2.0.1 gives the station
///   an explicit "refused, a session is still authorized here" verdict.
/// - **Stoppable transaction** — a live transaction whose id token is *no longer*
///   authorized (`transaction_active && !transaction_authorized`; e.g. the driver
///   re-presented their card, or the app/CSMS deauthorized) falls through to the
///   mechanical outcome below. Per OCPP 2.0.1 the station may then release the
///   cable; the wiring layer stops the transaction first (reason `UnlockCommand`),
///   mirroring the 1.6J stop-then-unlock, and reports the mechanical result.
/// - **Otherwise** — an idle connector, or the stoppable case above; the injected
///   mechanical outcome decides:
///   [`Unlock`](UnlockConnectorOutcome::Unlock) →
///   [`Unlocked`](UnlockStatusEnumType::Unlocked), and both
///   [`UnlockFailed`](UnlockConnectorOutcome::UnlockFailed) and
///   [`NotSupported`](UnlockConnectorOutcome::NotSupported) →
///   [`UnlockFailed`](UnlockStatusEnumType::UnlockFailed). 2.0.1's
///   `UnlockStatusEnumType` has no `NotSupported`, so a connector with no
///   controllable lock (which 1.6J reports as `NotSupported`) folds to the honest
///   "could not release the cable" verdict `UnlockFailed` — the one place the
///   shared [`UnlockConnectorOutcome`] seam maps differently across versions.
///
/// This is the *pure* decision, depending only on the request target and three
/// plain-`bool` reads (knownness, transaction activity, transaction
/// authorization) plus the outcome seam — no runtime handles, so it is
/// unit-testable in isolation. Resolving the target against the live connector
/// topology, reading transaction + authorization state, stopping a stoppable
/// transaction (reason `UnlockCommand`), and driving the actuator off the CALL
/// path is the wiring layer's job.
#[must_use]
pub fn v201_unlock_status(
    evse_id: i32,
    connector_id: i32,
    connector_known: bool,
    transaction_active: bool,
    transaction_authorized: bool,
    outcome: UnlockConnectorOutcome,
) -> UnlockStatusEnumType {
    // Structural: 2.0.1 evse/connector ids are 1-based; a value below 1 addresses
    // no physical connector, so there is nothing to unlock — reject before the
    // knownness read (mirrors the 1.6J `ConnectorId::new` Err arm and
    // `v201_request_start_status`'s structural guard).
    if evse_id < 1 || connector_id < 1 {
        return UnlockStatusEnumType::UnknownConnector;
    }
    // Runtime existence: a structurally-valid id the station has no connector for.
    if !connector_known {
        return UnlockStatusEnumType::UnknownConnector;
    }
    // A session that is *still authorized* on the connector: refuse to release
    // the cable. A live-but-*deauthorized* transaction (the driver/app stopped
    // authorizing) is instead *stoppable* — it falls through to the mechanical
    // outcome, and the wiring layer stops it first (reason `UnlockCommand`)
    // before releasing, exactly as the 1.6J handler stops-then-unlocks.
    if transaction_active && transaction_authorized {
        return UnlockStatusEnumType::OngoingAuthorizedTransaction;
    }
    // Otherwise — an idle connector, or a live-but-stoppable one — the mechanical
    // actuator outcome decides. 2.0.1 has no `NotSupported`, so an uncontrollable
    // lock folds to `UnlockFailed`.
    match outcome {
        UnlockConnectorOutcome::Unlock => UnlockStatusEnumType::Unlocked,
        UnlockConnectorOutcome::UnlockFailed | UnlockConnectorOutcome::NotSupported => {
            UnlockStatusEnumType::UnlockFailed
        }
    }
}

/// Build a schema-valid `UnlockConnector.conf` ([`UnlockConnectorResponse`]).
///
/// Pure constructor mirroring [`v201_reset_response`]: carries the decided
/// [`status`](UnlockStatusEnumType) plus the optional 2.0.1 `statusInfo` (a
/// vendor-agnostic `reasonCode` and human-readable detail — useful, for example,
/// to explain why an unlock was refused as `OngoingAuthorizedTransaction`).
#[must_use]
pub fn v201_unlock_response(
    status: UnlockStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> UnlockConnectorResponse {
    UnlockConnectorResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Decide the [`ChargingProfileStatusEnumType`] a `V201` station reports for an
/// inbound `SetChargingProfile.req`, given the incoming profile's
/// [`purpose`](ChargingProfilePurposeEnumType) and whether an ongoing
/// transaction exists on the targeted EVSE — returning the decided status
/// together with the optional `statusInfo` explaining a rejection.
///
/// Faithful to OCPP 2.0.1 (Part 2, `SetChargingProfile`) and a direct port of
/// the `RequestStartTransaction` handler's `TxProfile` guard in [`crate`]'s
/// `ChargePoint` (`lib.rs`), which enforces the same "a `chargingProfile`
/// attached to a transaction-scoped install SHALL be a `TxProfile`" contract:
///
/// - **Non-`TxProfile` purpose** — `TxDefaultProfile`,
///   `ChargingStationMaxProfile`, and `ChargingStationExternalConstraints` are
///   station-scoped / default profiles the simulator does not yet install; they
///   are
///   [`Rejected`](ChargingProfileStatusEnumType::Rejected) with an
///   `UnsupportedPurpose` `statusInfo`, checked *before* the transaction read so
///   the reason never depends on whether a session happens to be live.
/// - **`TxProfile` with no ongoing transaction on the target EVSE** — a
///   `TxProfile` is transaction-scoped, so with nothing to bind it to it is
///   [`Rejected`](ChargingProfileStatusEnumType::Rejected) with a `NoTransaction`
///   `statusInfo`. `has_active_transaction == false` folds a `0` / out-of-range
///   `evseId` (never a chargeable EVSE) and an idle-but-valid EVSE into one arm,
///   exactly as the 1.6J handler folds unknown and idle connectors into one
///   rejection.
/// - **`TxProfile` on an EVSE with a live transaction** —
///   [`Accepted`](ChargingProfileStatusEnumType::Accepted), no `statusInfo`. The
///   wiring layer then installs (replacing any profile already bound to that
///   EVSE) so the periodic-metering resolver enforces it on the next tick.
///
/// This is the *pure* decision, depending only on the profile purpose and a
/// single `bool` read — no runtime handles, so it is unit-testable in isolation.
/// Resolving `evse_id` to a live transaction and performing the install off the
/// CALL path is the wiring layer's job.
#[must_use]
pub fn v201_set_charging_profile_status(
    purpose: ChargingProfilePurposeEnumType,
    has_active_transaction: bool,
) -> (ChargingProfileStatusEnumType, Option<StatusInfoType>) {
    // Only a TxProfile is honored: it is the sole purpose the simulator installs
    // and enforces today (the transaction-scoped store + metering resolver).
    // Checked first so a station-scoped purpose is rejected for *being*
    // unsupported, independent of the live-transaction state.
    if purpose != ChargingProfilePurposeEnumType::TxProfile {
        return (
            ChargingProfileStatusEnumType::Rejected,
            Some(StatusInfoType {
                reason_code: "UnsupportedPurpose".to_string(),
                additional_info: Some(
                    "SetChargingProfile.chargingProfile.chargingProfilePurpose must be \
                     TxProfile; TxDefaultProfile / ChargingStationMaxProfile / \
                     ChargingStationExternalConstraints are not yet handled by the \
                     simulator"
                        .to_string(),
                ),
                custom_data: None,
            }),
        );
    }
    // A TxProfile is transaction-scoped: with no ongoing transaction on the
    // targeted EVSE there is nothing to bind it to.
    if !has_active_transaction {
        return (
            ChargingProfileStatusEnumType::Rejected,
            Some(StatusInfoType {
                reason_code: "NoTransaction".to_string(),
                additional_info: Some(
                    "SetChargingProfile carrying a TxProfile requires an ongoing \
                     transaction on the targeted EVSE"
                        .to_string(),
                ),
                custom_data: None,
            }),
        );
    }
    (ChargingProfileStatusEnumType::Accepted, None)
}

/// Build a schema-valid `SetChargingProfile.conf` ([`SetChargingProfileResponse`]).
///
/// Pure constructor mirroring [`v201_request_start_response`]: carries the
/// decided [`status`](ChargingProfileStatusEnumType) plus the optional 2.0.1
/// `statusInfo` (a vendor-agnostic `reasonCode` and human-readable detail —
/// useful, for example, to explain why an install was `Rejected`).
#[must_use]
pub fn v201_set_charging_profile_response(
    status: ChargingProfileStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> SetChargingProfileResponse {
    SetChargingProfileResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Decide which installed `TxProfile` slots an inbound `ClearChargingProfile.req`
/// removes, returning the EVSE keys to clear.
///
/// The teardown counterpart to [`v201_set_charging_profile_status`]: given the
/// request's selector and a `(evse_id, profile)` snapshot of the
/// [`V201TxProfileStore`](crate::v201_charging_profiles::V201TxProfileStore), it
/// returns every EVSE key whose installed profile matches — the wiring layer
/// then [`clear`](crate::v201_charging_profiles::V201TxProfileStore::clear)s each
/// and reports [`Accepted`](ClearChargingProfileStatusEnumType::Accepted) when
/// the returned slice is non-empty, [`Unknown`] otherwise.
///
/// Matching is faithful to OCPP 2.0.1 (Part 2, `ClearChargingProfile`) over the
/// simulator's one-`TxProfile`-per-EVSE store:
///
/// - **`chargingProfileId` present** — an exclusive selector (spec J01 note): the
///   `chargingProfileCriteria` are ignored and a slot matches iff its stored
///   `profile.id` equals it. A profile id that names nothing installed matches
///   nothing → `Unknown`.
/// - **`chargingProfileCriteria` present** (and no id) — a *filter*, each field
///   independently narrowing: `evseId` against the slot's EVSE key,
///   `chargingProfilePurpose` and `stackLevel` against the stored profile. An
///   absent field means "any", so it does not exclude. `evseId == 0` targets the
///   station-wide profile, which the transaction-scoped store never holds, so it
///   matches nothing here (faithful — no station-scoped install exists to clear).
/// - **Neither present** (an empty `{}` request) — matches *every* installed
///   profile, clearing the whole store (the "clear all" wildcard the message
///   documents).
///
/// Pure over its inputs (the selector plus an owned snapshot), so it is
/// unit-testable without a runtime or the store lock; taking the snapshot and
/// performing the removals is the wiring layer's job.
#[must_use]
pub fn v201_clear_charging_profile_matches(
    charging_profile_id: Option<i32>,
    criteria: Option<&ClearChargingProfileType>,
    installed: &[(i32, ChargingProfileType)],
) -> Vec<i32> {
    installed
        .iter()
        .filter(|(evse_id, profile)| {
            if let Some(id) = charging_profile_id {
                // An explicit profile id is the exclusive selector: the criteria
                // are ignored (spec J01), a slot matches purely on its stored id.
                profile.id == id
            } else if let Some(c) = criteria {
                // Each criterion is an independent filter; an absent field is a
                // wildcard that never excludes.
                c.evse_id.is_none_or(|e| e == *evse_id)
                    && c.charging_profile_purpose
                        .is_none_or(|p| p == profile.charging_profile_purpose)
                    && c.stack_level.is_none_or(|s| s == profile.stack_level)
            } else {
                // Neither id nor criteria: the "clear all" wildcard.
                true
            }
        })
        .map(|(evse_id, _)| *evse_id)
        .collect()
}

/// Build a schema-valid `ClearChargingProfile.conf`
/// ([`ClearChargingProfileResponse`]).
///
/// Pure constructor mirroring [`v201_set_charging_profile_response`]: the station
/// reports [`Accepted`](ClearChargingProfileStatusEnumType::Accepted) when it
/// cleared at least one matching profile (`matched == true`), or
/// [`Unknown`](ClearChargingProfileStatusEnumType::Unknown) when the selector
/// matched nothing installed — exactly the two-value contract
/// `ocpp.v201.enums.ClearChargingProfileStatusEnumType` defines.
#[must_use]
pub fn v201_clear_charging_profile_response(matched: bool) -> ClearChargingProfileResponse {
    ClearChargingProfileResponse {
        status: if matched {
            ClearChargingProfileStatusEnumType::Accepted
        } else {
            ClearChargingProfileStatusEnumType::Unknown
        },
        status_info: None,
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

    /// While the connector is idle, an availability change applies now — in
    /// either direction (`Operative` / `Inoperative`) — so the station reports
    /// `Accepted`.
    #[test]
    fn change_availability_is_accepted_when_idle() {
        for target in [
            OperationalStatusEnumType::Operative,
            OperationalStatusEnumType::Inoperative,
        ] {
            assert_eq!(
                v201_change_availability_status(target, false),
                ChangeAvailabilityStatusEnumType::Accepted,
                "{target:?} while idle should be Accepted"
            );
        }
    }

    /// While a transaction is in progress the change must wait until the
    /// connector is idle, so the station accepts but defers — `Scheduled` — in
    /// either direction, never cutting off a paying driver.
    #[test]
    fn change_availability_is_scheduled_when_transaction_in_progress() {
        for target in [
            OperationalStatusEnumType::Operative,
            OperationalStatusEnumType::Inoperative,
        ] {
            assert_eq!(
                v201_change_availability_status(target, true),
                ChangeAvailabilityStatusEnumType::Scheduled,
                "{target:?} while busy should be Scheduled"
            );
        }
    }

    /// The decision is total over the full `{Operative, Inoperative} × {idle,
    /// busy}` matrix: every combination classifies to `Accepted` (idle) or
    /// `Scheduled` (busy), split exactly 2/2, and `Rejected` — a runtime
    /// capability outcome — is never a policy result.
    #[test]
    fn change_availability_decision_is_total_and_never_rejected() {
        let mut accepted = 0;
        let mut scheduled = 0;
        for target in [
            OperationalStatusEnumType::Operative,
            OperationalStatusEnumType::Inoperative,
        ] {
            for in_progress in [false, true] {
                match v201_change_availability_status(target, in_progress) {
                    ChangeAvailabilityStatusEnumType::Accepted => accepted += 1,
                    ChangeAvailabilityStatusEnumType::Scheduled => scheduled += 1,
                    ChangeAvailabilityStatusEnumType::Rejected => panic!(
                        "{target:?} (in_progress={in_progress}) must not map to the \
                         runtime-only Rejected status"
                    ),
                }
            }
        }
        assert_eq!(accepted, 2, "expected exactly 2 idle → Accepted");
        assert_eq!(scheduled, 2, "expected exactly 2 busy → Scheduled");
    }

    #[test]
    fn change_availability_response_carries_status_and_optional_status_info() {
        let bare =
            v201_change_availability_response(ChangeAvailabilityStatusEnumType::Accepted, None);
        assert_eq!(bare.status, ChangeAvailabilityStatusEnumType::Accepted);
        assert!(bare.status_info.is_none());

        let info = StatusInfoType {
            reason_code: "Deferred".to_string(),
            additional_info: Some("availability change deferred until idle".to_string()),
            custom_data: None,
        };
        let scheduled = v201_change_availability_response(
            ChangeAvailabilityStatusEnumType::Scheduled,
            Some(info),
        );
        assert_eq!(
            scheduled.status,
            ChangeAvailabilityStatusEnumType::Scheduled
        );
        assert_eq!(
            scheduled
                .status_info
                .as_ref()
                .map(|i| i.reason_code.as_str()),
            Some("Deferred")
        );
    }

    /// Wire fidelity: every built `ChangeAvailability.conf` — with and without
    /// `statusInfo`, across all three status values — satisfies the bundled OCPP
    /// 2.0.1 `ChangeAvailabilityResponse` JSON Schema, the same guarantee the
    /// CP's version-aware validator gives on the live path.
    #[test]
    fn built_change_availability_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "Deferred".to_string(),
            additional_info: Some("availability change deferred until idle".to_string()),
            custom_data: None,
        };
        for status in [
            ChangeAvailabilityStatusEnumType::Accepted,
            ChangeAvailabilityStatusEnumType::Rejected,
            ChangeAvailabilityStatusEnumType::Scheduled,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_change_availability_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("ChangeAvailability", &payload)
                        .is_ok(),
                    "built {status:?} ChangeAvailabilityResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    /// A targeted EVSE that exists and is free to charge is `Accepted`, whether
    /// the id is explicit or defaulted — the 2.0.1 twin of the 1.6J handler's
    /// "free connector → Accepted".
    #[test]
    fn request_start_is_accepted_for_a_free_targeted_evse() {
        assert_eq!(
            v201_request_start_status(Some(1), true),
            RequestStartStopStatusEnumType::Accepted
        );
        // A missing evseId resolves to EVSE 1, so it behaves identically.
        assert_eq!(
            v201_request_start_status(None, true),
            RequestStartStopStatusEnumType::Accepted
        );
    }

    /// A busy or unknown EVSE (`evse_available == false`) is `Rejected` — the
    /// 1.6J handler folds both the known-but-busy connector and the unknown
    /// connector id into the same `Rejected` arm.
    #[test]
    fn request_start_is_rejected_for_a_busy_or_unknown_evse() {
        assert_eq!(
            v201_request_start_status(Some(2), false),
            RequestStartStopStatusEnumType::Rejected
        );
        assert_eq!(
            v201_request_start_status(None, false),
            RequestStartStopStatusEnumType::Rejected
        );
    }

    /// A structurally-invalid target (`evseId` of 0 or negative) is `Rejected`
    /// *before* availability is even consulted — 2.0.1 EVSE ids are 1-based, so
    /// there is no chargeable EVSE there. Mirrors the 1.6J `ConnectorId::new`
    /// `Err` arm for connector 0 / out of range: even a "would-be-available"
    /// read cannot rescue it.
    #[test]
    fn request_start_is_rejected_for_a_structurally_invalid_evse_id() {
        for id in [0, -1, i32::MIN] {
            assert_eq!(
                v201_request_start_status(Some(id), true),
                RequestStartStopStatusEnumType::Rejected,
                "evseId {id} is not a chargeable EVSE and must be Rejected \
                 regardless of the availability read"
            );
        }
    }

    /// A missing `evseId` resolves to EVSE 1: the `None` decision matches the
    /// `Some(1)` decision across the availability axis (acceptance criterion),
    /// while `Some(0)` — a real but invalid target — must *not* be treated like
    /// the `None` default.
    #[test]
    fn missing_evse_id_resolves_to_evse_1() {
        for available in [false, true] {
            assert_eq!(
                v201_request_start_status(None, available),
                v201_request_start_status(Some(1), available),
                "a missing evseId (available={available}) must decide exactly as \
                 an explicit EVSE 1"
            );
        }
        // The default is EVSE 1, not EVSE 0: an explicit 0 is still Rejected.
        assert_eq!(
            v201_request_start_status(Some(0), true),
            RequestStartStopStatusEnumType::Rejected
        );
    }

    #[test]
    fn request_start_response_carries_status_and_optional_status_info() {
        let bare = v201_request_start_response(RequestStartStopStatusEnumType::Accepted, None);
        assert_eq!(bare.status, RequestStartStopStatusEnumType::Accepted);
        assert!(bare.status_info.is_none());
        // The pure builder never fabricates a transactionId (a live-state concern).
        assert!(bare.transaction_id.is_none());

        let info = StatusInfoType {
            reason_code: "EvseBusy".to_string(),
            additional_info: Some("targeted EVSE is not free to charge".to_string()),
            custom_data: None,
        };
        let rejected =
            v201_request_start_response(RequestStartStopStatusEnumType::Rejected, Some(info));
        assert_eq!(rejected.status, RequestStartStopStatusEnumType::Rejected);
        assert_eq!(
            rejected
                .status_info
                .as_ref()
                .map(|i| i.reason_code.as_str()),
            Some("EvseBusy")
        );
    }

    /// Wire fidelity: every built `RequestStartTransaction.conf` — with and
    /// without `statusInfo`, across both status values — satisfies the bundled
    /// OCPP 2.0.1 `RequestStartTransactionResponse` JSON Schema, the same
    /// guarantee the CP's version-aware validator gives on the live path.
    /// `validate_call_result` keys on the base `"RequestStartTransaction"`
    /// action (it appends `Response` internally).
    #[test]
    fn built_request_start_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "EvseBusy".to_string(),
            additional_info: Some("targeted EVSE is not free to charge".to_string()),
            custom_data: None,
        };
        for status in [
            RequestStartStopStatusEnumType::Accepted,
            RequestStartStopStatusEnumType::Rejected,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_request_start_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("RequestStartTransaction", &payload)
                        .is_ok(),
                    "built {status:?} RequestStartTransactionResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    #[test]
    fn request_stop_is_accepted_only_for_a_live_transaction_id() {
        // The requested id names a transaction the station is running.
        assert_eq!(
            v201_request_stop_status("1", &["1"]),
            RequestStartStopStatusEnumType::Accepted
        );
        // ...even when several sessions are live concurrently (one per EVSE).
        assert_eq!(
            v201_request_stop_status("3", &["1", "2", "3"]),
            RequestStartStopStatusEnumType::Accepted
        );
    }

    #[test]
    fn request_stop_is_rejected_for_an_unknown_id_or_idle_station() {
        // Unknown id while other transactions are live.
        assert_eq!(
            v201_request_stop_status("9", &["1", "2"]),
            RequestStartStopStatusEnumType::Rejected
        );
        // Idle station — no live transaction to stop.
        assert_eq!(
            v201_request_stop_status("1", &[]),
            RequestStartStopStatusEnumType::Rejected
        );
    }

    #[test]
    fn request_stop_matches_exactly_never_by_numeric_value_or_substring() {
        // Exact string equality: a non-canonical spelling of a live id does not
        // match (the station only ever issued the canonical decimal), and a
        // substring / prefix relationship is not a match either. Nothing is
        // parsed, so a huge or malformed id simply fails to match — never panics.
        for requested in [
            "01",
            " 1",
            "1 ",
            "10",
            "",
            "not-a-number",
            "99999999999999999999",
        ] {
            assert_eq!(
                v201_request_stop_status(requested, &["1"]),
                RequestStartStopStatusEnumType::Rejected,
                "{requested:?} must not match the live id \"1\""
            );
        }
    }

    #[test]
    fn request_stop_response_carries_status_and_optional_status_info() {
        let bare = v201_request_stop_response(RequestStartStopStatusEnumType::Accepted, None);
        assert_eq!(bare.status, RequestStartStopStatusEnumType::Accepted);
        assert!(bare.status_info.is_none());
        assert!(bare.custom_data.is_none());

        let info = StatusInfoType {
            reason_code: "NoTransaction".to_string(),
            additional_info: Some("no live transaction with that id".to_string()),
            custom_data: None,
        };
        let rejected =
            v201_request_stop_response(RequestStartStopStatusEnumType::Rejected, Some(info));
        assert_eq!(rejected.status, RequestStartStopStatusEnumType::Rejected);
        assert_eq!(rejected.status_info.unwrap().reason_code, "NoTransaction");
    }

    /// Wire fidelity: every built `RequestStopTransaction.conf` — with and
    /// without `statusInfo`, across both status values — satisfies the bundled
    /// OCPP 2.0.1 `RequestStopTransactionResponse` JSON Schema.
    #[test]
    fn built_request_stop_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "NoTransaction".to_string(),
            additional_info: Some("no live transaction with that id".to_string()),
            custom_data: None,
        };
        for status in [
            RequestStartStopStatusEnumType::Accepted,
            RequestStartStopStatusEnumType::Rejected,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_request_stop_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("RequestStopTransaction", &payload)
                        .is_ok(),
                    "built {status:?} RequestStopTransactionResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    /// A known, idle connector unlocks per the injected mechanical outcome:
    /// `Unlock` → `Unlocked`, `UnlockFailed` → `UnlockFailed`. The two mechanical
    /// values 2.0.1 keeps.
    #[test]
    fn unlock_known_idle_connector_follows_mechanical_outcome() {
        assert_eq!(
            v201_unlock_status(1, 1, true, false, false, UnlockConnectorOutcome::Unlock),
            UnlockStatusEnumType::Unlocked
        );
        assert_eq!(
            v201_unlock_status(
                1,
                1,
                true,
                false,
                false,
                UnlockConnectorOutcome::UnlockFailed
            ),
            UnlockStatusEnumType::UnlockFailed
        );
    }

    /// 2.0.1's `UnlockStatusEnumType` has no `NotSupported`, so a connector whose
    /// lock is not controllable (`UnlockConnectorOutcome::NotSupported`, which
    /// 1.6J reports verbatim) folds to the honest `UnlockFailed` — the one place
    /// the shared actuator seam maps differently across versions.
    #[test]
    fn unlock_not_supported_outcome_folds_to_unlock_failed_in_v201() {
        assert_eq!(
            v201_unlock_status(
                1,
                1,
                true,
                false,
                false,
                UnlockConnectorOutcome::NotSupported
            ),
            UnlockStatusEnumType::UnlockFailed
        );
    }

    /// A live *authorized* transaction on the targeted connector refuses the
    /// unlock with `OngoingAuthorizedTransaction`, independent of the mechanical
    /// outcome (the cable must not be released while a session is authorized).
    #[test]
    fn unlock_is_refused_while_a_transaction_is_authorized() {
        for outcome in [
            UnlockConnectorOutcome::Unlock,
            UnlockConnectorOutcome::UnlockFailed,
            UnlockConnectorOutcome::NotSupported,
        ] {
            assert_eq!(
                v201_unlock_status(1, 1, true, true, true, outcome),
                UnlockStatusEnumType::OngoingAuthorizedTransaction,
                "{outcome:?} on a connector with a still-authorized transaction \
                 must be OngoingAuthorizedTransaction"
            );
        }
    }

    /// A live but *deauthorized* (stoppable) transaction no longer refuses: the
    /// station releases the cable per the mechanical outcome, exactly as an idle
    /// connector would. The wiring layer stops it first (reason `UnlockCommand`);
    /// the pure decision only reports the mechanical result.
    #[test]
    fn unlock_of_a_deauthorized_transaction_follows_mechanical_outcome() {
        assert_eq!(
            v201_unlock_status(1, 1, true, true, false, UnlockConnectorOutcome::Unlock),
            UnlockStatusEnumType::Unlocked,
            "a deauthorized (stoppable) transaction releases the cable"
        );
        assert_eq!(
            v201_unlock_status(
                1,
                1,
                true,
                true,
                false,
                UnlockConnectorOutcome::UnlockFailed
            ),
            UnlockStatusEnumType::UnlockFailed,
            "a stoppable transaction still surfaces a mechanical unlock failure"
        );
        // 2.0.1 has no `NotSupported`: it folds to `UnlockFailed` here too.
        assert_eq!(
            v201_unlock_status(
                1,
                1,
                true,
                true,
                false,
                UnlockConnectorOutcome::NotSupported
            ),
            UnlockStatusEnumType::UnlockFailed
        );
    }

    /// A structurally-valid id the station has no connector for
    /// (`connector_known == false`) is `UnknownConnector`, regardless of
    /// transaction state or mechanical outcome — nothing can rescue an unmapped
    /// target.
    #[test]
    fn unlock_unmapped_target_is_unknown_connector() {
        for transaction_active in [false, true] {
            for outcome in [
                UnlockConnectorOutcome::Unlock,
                UnlockConnectorOutcome::UnlockFailed,
                UnlockConnectorOutcome::NotSupported,
            ] {
                assert_eq!(
                    v201_unlock_status(9, 1, false, transaction_active, true, outcome),
                    UnlockStatusEnumType::UnknownConnector,
                    "an unmapped (evse=9) target must be UnknownConnector \
                     (transaction_active={transaction_active}, {outcome:?})"
                );
            }
        }
    }

    /// A structurally-invalid target (`evseId` or `connectorId` below `1`) is
    /// `UnknownConnector` *before* the knownness read — even a `connector_known ==
    /// true` cannot rescue it, mirroring the 1.6J `ConnectorId::new` Err arm and
    /// `v201_request_start_status`'s structural guard.
    #[test]
    fn unlock_structurally_invalid_target_is_unknown_connector() {
        for (evse_id, connector_id) in [(0, 1), (1, 0), (-1, 1), (1, -1), (i32::MIN, i32::MIN)] {
            assert_eq!(
                v201_unlock_status(
                    evse_id,
                    connector_id,
                    true,
                    false,
                    false,
                    UnlockConnectorOutcome::Unlock
                ),
                UnlockStatusEnumType::UnknownConnector,
                "evse={evse_id}, connector={connector_id} is not a physical \
                 connector and must be UnknownConnector regardless of the \
                 knownness read"
            );
        }
    }

    #[test]
    fn unlock_response_carries_status_and_optional_status_info() {
        let bare = v201_unlock_response(UnlockStatusEnumType::Unlocked, None);
        assert_eq!(bare.status, UnlockStatusEnumType::Unlocked);
        assert!(bare.status_info.is_none());

        let info = StatusInfoType {
            reason_code: "OngoingTx".to_string(),
            additional_info: Some("a session is still authorized on this connector".to_string()),
            custom_data: None,
        };
        let refused = v201_unlock_response(
            UnlockStatusEnumType::OngoingAuthorizedTransaction,
            Some(info),
        );
        assert_eq!(
            refused.status,
            UnlockStatusEnumType::OngoingAuthorizedTransaction
        );
        assert_eq!(
            refused.status_info.as_ref().map(|i| i.reason_code.as_str()),
            Some("OngoingTx")
        );
    }

    /// Wire fidelity: every built `UnlockConnector.conf` — with and without
    /// `statusInfo`, across all four status values — satisfies the bundled OCPP
    /// 2.0.1 `UnlockConnectorResponse` JSON Schema, the same guarantee the CP's
    /// version-aware validator gives on the live path. `validate_call_result`
    /// keys on the base `"UnlockConnector"` action (it appends `Response`
    /// internally).
    #[test]
    fn built_unlock_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "OngoingTx".to_string(),
            additional_info: Some("a session is still authorized on this connector".to_string()),
            custom_data: None,
        };
        for status in [
            UnlockStatusEnumType::Unlocked,
            UnlockStatusEnumType::UnlockFailed,
            UnlockStatusEnumType::OngoingAuthorizedTransaction,
            UnlockStatusEnumType::UnknownConnector,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_unlock_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("UnlockConnector", &payload)
                        .is_ok(),
                    "built {status:?} UnlockConnectorResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    #[test]
    fn set_charging_profile_accepts_a_txprofile_with_an_active_transaction() {
        let (status, info) = v201_set_charging_profile_status(
            ChargingProfilePurposeEnumType::TxProfile,
            /* has_active_transaction */ true,
        );
        assert_eq!(status, ChargingProfileStatusEnumType::Accepted);
        // An accepted install carries no rejection detail.
        assert!(info.is_none());
    }

    #[test]
    fn set_charging_profile_rejects_a_txprofile_with_no_active_transaction() {
        let (status, info) = v201_set_charging_profile_status(
            ChargingProfilePurposeEnumType::TxProfile,
            /* has_active_transaction */ false,
        );
        assert_eq!(status, ChargingProfileStatusEnumType::Rejected);
        assert_eq!(
            info.as_ref().map(|i| i.reason_code.as_str()),
            Some("NoTransaction"),
            "a TxProfile with nothing to bind to is rejected with NoTransaction"
        );
    }

    #[test]
    fn set_charging_profile_rejects_every_non_txprofile_purpose_before_the_transaction_read() {
        // The purpose guard is checked first: a station-scoped / default profile is
        // rejected for being unsupported regardless of whether a session is live.
        for purpose in [
            ChargingProfilePurposeEnumType::TxDefaultProfile,
            ChargingProfilePurposeEnumType::ChargingStationMaxProfile,
            ChargingProfilePurposeEnumType::ChargingStationExternalConstraints,
        ] {
            for has_tx in [false, true] {
                let (status, info) = v201_set_charging_profile_status(purpose, has_tx);
                assert_eq!(
                    status,
                    ChargingProfileStatusEnumType::Rejected,
                    "{purpose:?} is not honored by the simulator (has_tx={has_tx})"
                );
                assert_eq!(
                    info.as_ref().map(|i| i.reason_code.as_str()),
                    Some("UnsupportedPurpose"),
                    "{purpose:?} is rejected with UnsupportedPurpose (has_tx={has_tx})"
                );
            }
        }
    }

    #[test]
    fn set_charging_profile_response_carries_status_and_optional_status_info() {
        let bare =
            v201_set_charging_profile_response(ChargingProfileStatusEnumType::Accepted, None);
        assert_eq!(bare.status, ChargingProfileStatusEnumType::Accepted);
        assert!(bare.status_info.is_none());

        let info = StatusInfoType {
            reason_code: "NoTransaction".to_string(),
            additional_info: Some("no ongoing transaction on the targeted EVSE".to_string()),
            custom_data: None,
        };
        let rejected =
            v201_set_charging_profile_response(ChargingProfileStatusEnumType::Rejected, Some(info));
        assert_eq!(rejected.status, ChargingProfileStatusEnumType::Rejected);
        assert_eq!(
            rejected
                .status_info
                .as_ref()
                .map(|i| i.reason_code.as_str()),
            Some("NoTransaction")
        );
    }

    /// Wire fidelity: every built `SetChargingProfile.conf` — with and without
    /// `statusInfo`, across both status values — satisfies the bundled OCPP 2.0.1
    /// `SetChargingProfileResponse` JSON Schema, the same guarantee the CP's
    /// version-aware validator gives on the live path. `validate_call_result` keys
    /// on the base `"SetChargingProfile"` action (it appends `Response`
    /// internally).
    #[test]
    fn built_set_charging_profile_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "UnsupportedPurpose".to_string(),
            additional_info: Some("only TxProfile is handled by the simulator".to_string()),
            custom_data: None,
        };
        for status in [
            ChargingProfileStatusEnumType::Accepted,
            ChargingProfileStatusEnumType::Rejected,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_set_charging_profile_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("SetChargingProfile", &payload)
                        .is_ok(),
                    "built {status:?} SetChargingProfileResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    // ---- ClearChargingProfile (#474) ----

    /// A minimal but schema-shaped `TxProfile` whose selector fields (`id`,
    /// `stackLevel`, `chargingProfilePurpose`) are set from the arguments; the
    /// schedule is an unremarkable single 11 kW period, since the matcher never
    /// reads it.
    fn clear_test_profile(
        id: i32,
        stack_level: i32,
        purpose: ChargingProfilePurposeEnumType,
    ) -> ChargingProfileType {
        use ocpp_types::v201::{
            ChargingProfileKindEnumType, ChargingRateUnitEnumType, ChargingSchedulePeriodType,
            ChargingScheduleType,
        };
        ChargingProfileType {
            id,
            stack_level,
            charging_profile_purpose: purpose,
            charging_profile_kind: ChargingProfileKindEnumType::Relative,
            charging_schedule: vec![ChargingScheduleType {
                id: 1,
                charging_rate_unit: ChargingRateUnitEnumType::W,
                charging_schedule_period: vec![ChargingSchedulePeriodType {
                    start_period: 0,
                    limit: 11_000.0,
                    number_phases: None,
                    phase_to_use: None,
                    custom_data: None,
                }],
                start_schedule: None,
                duration: None,
                min_charging_rate: None,
                sales_tariff: None,
                custom_data: None,
            }],
            recurrency_kind: None,
            valid_from: None,
            valid_to: None,
            transaction_id: None,
            custom_data: None,
        }
    }

    /// A two-EVSE store: EVSE 1 holds profile id 10 (TxProfile, stack 0), EVSE 2
    /// holds profile id 20 (TxProfile, stack 3).
    fn clear_test_store() -> Vec<(i32, ChargingProfileType)> {
        vec![
            (
                1,
                clear_test_profile(10, 0, ChargingProfilePurposeEnumType::TxProfile),
            ),
            (
                2,
                clear_test_profile(20, 3, ChargingProfilePurposeEnumType::TxProfile),
            ),
        ]
    }

    #[test]
    fn clear_by_profile_id_matches_only_that_slot() {
        let store = clear_test_store();
        // The id names the profile installed on EVSE 2.
        assert_eq!(
            v201_clear_charging_profile_matches(Some(20), None, &store),
            vec![2]
        );
        // A profile id nothing holds matches nothing.
        assert!(v201_clear_charging_profile_matches(Some(999), None, &store).is_empty());
    }

    #[test]
    fn clear_by_profile_id_ignores_the_criteria() {
        let store = clear_test_store();
        // Spec J01: with a `chargingProfileId` present, the criteria are ignored —
        // even a criteria block that would exclude EVSE 2 (wrong evseId + wrong
        // stackLevel) does not stop the id from matching it.
        let criteria = ClearChargingProfileType {
            evse_id: Some(1),
            charging_profile_purpose: Some(ChargingProfilePurposeEnumType::TxDefaultProfile),
            stack_level: Some(99),
            custom_data: None,
        };
        assert_eq!(
            v201_clear_charging_profile_matches(Some(20), Some(&criteria), &store),
            vec![2]
        );
    }

    #[test]
    fn clear_by_evse_id_criterion_selects_that_evse() {
        let store = clear_test_store();
        let criteria = ClearChargingProfileType {
            evse_id: Some(1),
            charging_profile_purpose: None,
            stack_level: None,
            custom_data: None,
        };
        assert_eq!(
            v201_clear_charging_profile_matches(None, Some(&criteria), &store),
            vec![1]
        );
    }

    #[test]
    fn clear_by_stack_level_criterion_selects_the_matching_profile() {
        let store = clear_test_store();
        let criteria = ClearChargingProfileType {
            evse_id: None,
            charging_profile_purpose: None,
            stack_level: Some(3),
            custom_data: None,
        };
        // Only EVSE 2's profile is at stack level 3.
        assert_eq!(
            v201_clear_charging_profile_matches(None, Some(&criteria), &store),
            vec![2]
        );
    }

    #[test]
    fn clear_by_purpose_criterion_matches_txprofiles_and_excludes_others() {
        let store = clear_test_store();
        // The store holds only TxProfiles, so a TxProfile purpose filter matches
        // every slot...
        let tx = ClearChargingProfileType {
            evse_id: None,
            charging_profile_purpose: Some(ChargingProfilePurposeEnumType::TxProfile),
            stack_level: None,
            custom_data: None,
        };
        assert_eq!(
            v201_clear_charging_profile_matches(None, Some(&tx), &store),
            vec![1, 2]
        );
        // ...and a non-TxProfile purpose matches nothing installed.
        let default = ClearChargingProfileType {
            evse_id: None,
            charging_profile_purpose: Some(ChargingProfilePurposeEnumType::TxDefaultProfile),
            stack_level: None,
            custom_data: None,
        };
        assert!(v201_clear_charging_profile_matches(None, Some(&default), &store).is_empty());
    }

    #[test]
    fn clear_criteria_are_conjunctive() {
        let store = clear_test_store();
        // evseId 1 AND stackLevel 3 — EVSE 1's profile is at stack 0, so the two
        // criteria together exclude it, matching nothing.
        let criteria = ClearChargingProfileType {
            evse_id: Some(1),
            charging_profile_purpose: None,
            stack_level: Some(3),
            custom_data: None,
        };
        assert!(v201_clear_charging_profile_matches(None, Some(&criteria), &store).is_empty());
    }

    #[test]
    fn clear_evse_id_zero_matches_nothing_in_the_txprofile_store() {
        let store = clear_test_store();
        // evseId 0 targets the station-wide profile the transaction-scoped store
        // never holds (its keys are real EVSEs ≥ 1).
        let criteria = ClearChargingProfileType {
            evse_id: Some(0),
            charging_profile_purpose: None,
            stack_level: None,
            custom_data: None,
        };
        assert!(v201_clear_charging_profile_matches(None, Some(&criteria), &store).is_empty());
    }

    #[test]
    fn clear_empty_request_matches_every_installed_profile() {
        let store = clear_test_store();
        // Neither id nor criteria: the "clear all" wildcard.
        assert_eq!(
            v201_clear_charging_profile_matches(None, None, &store),
            vec![1, 2]
        );
        // An empty request against an empty store matches nothing (→ Unknown).
        assert!(v201_clear_charging_profile_matches(None, None, &[]).is_empty());
    }

    #[test]
    fn clear_response_maps_matched_to_accepted_and_unmatched_to_unknown() {
        assert_eq!(
            v201_clear_charging_profile_response(true).status,
            ClearChargingProfileStatusEnumType::Accepted
        );
        assert_eq!(
            v201_clear_charging_profile_response(false).status,
            ClearChargingProfileStatusEnumType::Unknown
        );
        // The builder carries no rejection detail on either arm.
        assert!(v201_clear_charging_profile_response(true)
            .status_info
            .is_none());
    }

    /// Wire fidelity: both built `ClearChargingProfile.conf` values satisfy the
    /// bundled OCPP 2.0.1 `ClearChargingProfileResponse` JSON Schema.
    #[test]
    fn built_clear_charging_profile_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        for matched in [true, false] {
            let resp = v201_clear_charging_profile_response(matched);
            let payload = serde_json::to_value(&resp).unwrap();
            assert!(
                validator
                    .validate_call_result("ClearChargingProfile", &payload)
                    .is_ok(),
                "built ClearChargingProfileResponse (matched={matched}) should be schema-valid, got: {payload}"
            );
        }
    }
}
