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
//!
//! ## `CancelReservation`
//!
//! Ports `ocpp.v201.call.CancelReservation` /
//! `ocpp.v201.call_result.CancelReservation` — the 2.0.1 successor to the 1.6J
//! `CancelReservation` the CP already answers on the `V16J` path, and the
//! teardown counterpart to the v201 `ReserveNow` slice. A CSMS drops a
//! previously-made reservation by its integer `reservationId`; the station
//! reports [`Accepted`](CancelReservationStatusEnumType::Accepted) when that id
//! names a reservation it currently holds (now freed, `Reserved → Available`) or
//! [`Rejected`](CancelReservationStatusEnumType::Rejected) when the id is unknown
//! — the reserve/cancel analogue of [`v201_request_stop_status`]'s
//! member-of-the-live-set decision. Both versions key on the same integer id and
//! share the same reservation store; only the response status enum differs
//! (2.0.1's `CancelReservationStatusEnumType` is a clean `Accepted`/`Rejected`
//! two-way split). Resolving the id against the live reservation store, freeing
//! the connector, disarming the auto-expiry timer, and announcing `Available` off
//! the CALL path is the wiring layer's job.
//!
//! ## `ClearCache`
//!
//! Ports `ocpp.v201.call.ClearCache` / `ocpp.v201.call_result.ClearCache` — the
//! 2.0.1 successor to the 1.6J `ClearCache` the CP already answers on the `V16J`
//! path. The CSMS asks the station to wipe its local Authorization Cache; the
//! request carries no fields beyond the optional `customData`. Because the
//! simulator implements a cache ([`crate::auth_cache::AuthCache`]), a clear is
//! [`Accepted`](ClearCacheStatusEnumType::Accepted);
//! [`Rejected`](ClearCacheStatusEnumType::Rejected) is the modeled outcome for a
//! station that does not support caching at all — a future opt-in knob, kept as a
//! seam here so the decision needn't change shape to grow one. Both dialects
//! empty the same shared cache; only the response status enum differs (2.0.1's
//! `ClearCacheStatusEnumType` is a clean `Accepted`/`Rejected` two-way split,
//! plus an optional `statusInfo`). Emptying the shared cache off the CALL path is
//! the wiring layer's job.
//!
//! ## `SetDisplayMessage`
//!
//! Ports `ocpp.v201.call.SetDisplayMessage` /
//! `ocpp.v201.call_result.SetDisplayMessage` (OCPP 2.0.1 Part 2, E05–E08). The
//! CSMS installs one [`MessageInfoType`] for the station to show on its display;
//! the station answers synchronously with a [`DisplayMessageStatusEnumType`]. It
//! has no 1.6J twin. The *pure decision*
//! ([`v201_set_display_message_status`]) is the accept/reject half —
//! `Accepted` for a message the simulator can model, or `UnknownTransaction` when
//! the message binds a `transactionId` the station is not running — and the wiring
//! layer upserts the accepted message into the display-message store
//! ([`V201DisplayMessageStore`](crate::v201_display_message::V201DisplayMessageStore)),
//! keyed by `MessageInfoType.id` so a same-id re-install replaces rather than
//! duplicates. The remaining reserved statuses
//! (`NotSupportedMessageFormat` / `NotSupportedPriority` / `NotSupportedState` /
//! `Rejected`) are documented modeled seams the simulator does not produce — a
//! simulated display can render any schema-valid format/priority/state — kept for
//! the wire and a future capability knob, the way the monitor store (#494)
//! documents its unproduced statuses.
//!
//! ## `SignCertificate`
//!
//! Ports `ocpp.v201.call.SignCertificate` /
//! `ocpp.v201.call_result.SignCertificate` (OCPP 2.0.1 Part 2, A02). Unlike the
//! rest of this module — CSMS→CP commands the station *answers* — `SignCertificate`
//! is **CP-initiated**: the station originates the certificate-provisioning flow
//! by submitting a PEM-encoded CSR, the CSMS acknowledges synchronously with a
//! [`GenericStatusEnumType`] (`Accepted` / `Rejected`), and the CA-signed chain
//! arrives later out-of-band via the paired `CertificateSigned` CALL (the
//! delivery terminus this module already answers,
//! [`v201_certificate_signed_status`]). Because it is originated rather than
//! answered, the pure half here is a *request* builder
//! ([`v201_sign_certificate_request`]) rather than a decision + response builder,
//! and the wiring layer emits it as an outbound CALL and surfaces the
//! acknowledgement ([`ChargePoint::request_sign_certificate`](crate::ChargePoint::request_sign_certificate)).
//! The simulator does no crypto, so the CSR is an opaque, well-shaped placeholder
//! ([`v201_placeholder_csr`]) — the same "no PKI" boundary the certificate
//! decision predicates set.

use std::collections::BTreeMap;

use ocpp_types::common::Reason;
use ocpp_types::v16j::ResetType;
use ocpp_types::v201::{
    CancelReservationStatusEnumType, CertificateActionEnumType, CertificateHashDataChainType,
    CertificateHashDataType, CertificateSignedStatusEnumType, CertificateSigningUseEnumType,
    ChangeAvailabilityStatusEnumType, ChargingLimitSourceEnumType, ChargingProfileCriterionType,
    ChargingProfilePurposeEnumType, ChargingProfileStatusEnumType, ChargingProfileType,
    ClearCacheStatusEnumType, ClearChargingProfileStatusEnumType, ClearChargingProfileType,
    ClearMessageStatusEnumType, CustomerInformationStatusEnumType, DeleteCertificateStatusEnumType,
    DisplayMessageStatusEnumType, FirmwareStatusEnumType, GenericDeviceModelStatusEnumType,
    GenericStatusEnumType, GetCertificateIdUseEnumType, GetChargingProfileStatusEnumType,
    GetDisplayMessagesStatusEnumType, GetInstalledCertificateStatusEnumType, HashAlgorithmEnumType,
    InstallCertificateStatusEnumType, InstallCertificateUseEnumType, LogEnumType,
    LogStatusEnumType, MessageInfoType, MessagePriorityEnumType, MessageStateEnumType,
    MessageTriggerEnumType, OperationalStatusEnumType, PublishFirmwareStatusEnumType,
    RequestStartStopStatusEnumType, ReservationUpdateStatusEnumType, ReserveNowStatusEnumType,
    ResetEnumType, ResetStatusEnumType, SetNetworkProfileStatusEnumType, StatusInfoType,
    TriggerMessageStatusEnumType, UnlockStatusEnumType, UnpublishFirmwareStatusEnumType,
    UpdateFirmwareStatusEnumType, UploadLogStatusEnumType,
};

use ocpp_messages::v201::{
    CancelReservationResponse, CertificateSignedResponse, ChangeAvailabilityResponse,
    ClearCacheResponse, ClearChargingProfileResponse, ClearDisplayMessageResponse,
    CostUpdatedResponse, CustomerInformationRequest, CustomerInformationResponse,
    DeleteCertificateResponse, FirmwareStatusNotificationRequest, Get15118EVCertificateRequest,
    GetChargingProfilesResponse, GetDisplayMessagesResponse, GetInstalledCertificateIdsResponse,
    GetLogRequest, GetLogResponse, GetMonitoringReportResponse, GetTransactionStatusResponse,
    InstallCertificateResponse, LogStatusNotificationRequest, NotifyCustomerInformationRequest,
    NotifyDisplayMessagesRequest, PublishFirmwareRequest, PublishFirmwareResponse,
    PublishFirmwareStatusNotificationRequest, ReportChargingProfilesRequest,
    RequestStartTransactionResponse, RequestStopTransactionResponse,
    ReservationStatusUpdateRequest, ReserveNowResponse, ResetResponse, SetChargingProfileResponse,
    SetDisplayMessageResponse, SetMonitoringBaseResponse, SetMonitoringLevelResponse,
    SetNetworkProfileRequest, SetNetworkProfileResponse, SignCertificateRequest,
    TriggerMessageResponse, UnlockConnectorResponse, UnpublishFirmwareRequest,
    UnpublishFirmwareResponse, UpdateFirmwareRequest, UpdateFirmwareResponse,
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
/// - **`TxDefaultProfile` purpose** — the default schedule applied to an EVSE
///   whenever no `TxProfile` is in force (Issue #471). Station configuration, not
///   transaction-scoped, so it is
///   [`Accepted`](ChargingProfileStatusEnumType::Accepted) regardless of whether
///   a session is live; the wiring layer installs it into the
///   [`V201TxDefaultProfileStore`](crate::v201_tx_default_profile::V201TxDefaultProfileStore)
///   (an `evseId = 0` install becomes the station-wide default).
/// - **`ChargingStationMaxProfile` / `ChargingStationExternalConstraints`** —
///   station-wide *ceilings* that cap a resolved limit rather than substitute for
///   it (`min(resolved, ceiling)`, Issue #511). Like a `TxDefaultProfile` they are
///   station configuration, not transaction-scoped, so they are
///   [`Accepted`](ChargingProfileStatusEnumType::Accepted) regardless of whether a
///   session is live; the wiring layer installs them into the
///   [`V201StationCeilingStore`](crate::v201_station_ceiling::V201StationCeilingStore)
///   (an `evseId = 0` install becomes the whole-station ceiling) and the metering
///   resolver / `GetCompositeSchedule` cap by them.
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
    match purpose {
        // A TxProfile is transaction-scoped: with no ongoing transaction on the
        // targeted EVSE there is nothing to bind it to.
        ChargingProfilePurposeEnumType::TxProfile => {
            if has_active_transaction {
                (ChargingProfileStatusEnumType::Accepted, None)
            } else {
                (
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
                )
            }
        }
        // A TxDefaultProfile is station configuration, not transaction-scoped: it
        // is the default schedule applied to an EVSE whenever no TxProfile is in
        // force, so it is Accepted regardless of whether a session is live
        // (Issue #471). `evseId = 0` installs the station-wide default that
        // applies to every EVSE; the install target is the wiring layer's job.
        ChargingProfilePurposeEnumType::TxDefaultProfile => {
            (ChargingProfileStatusEnumType::Accepted, None)
        }
        // The station-ceiling purposes cap a resolved limit rather than substitute
        // for it (`min(resolved, ceiling)`); like a TxDefaultProfile they are
        // station configuration, not transaction-scoped, so they are Accepted
        // regardless of whether a session is live. `evseId = 0` installs the
        // whole-station ceiling. The wiring layer routes them into the
        // `V201StationCeilingStore`, which the metering resolver and
        // `GetCompositeSchedule` cap by (Issue #511).
        ChargingProfilePurposeEnumType::ChargingStationMaxProfile
        | ChargingProfilePurposeEnumType::ChargingStationExternalConstraints => {
            (ChargingProfileStatusEnumType::Accepted, None)
        }
    }
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

/// Decide which installed charging-profile slots an inbound
/// `ClearChargingProfile.req` removes, returning the matched `(evse_id, profile)`
/// pairs to clear.
///
/// The teardown counterpart to [`v201_set_charging_profile_status`], and the
/// removal twin of [`v201_get_charging_profiles_matches`]: given the request's
/// selector and a `(evse_id, profile)` snapshot of the installed profiles, it
/// returns every slot that matches — the wiring layer then clears each from
/// whichever store holds it and reports
/// [`Accepted`](ClearChargingProfileStatusEnumType::Accepted) when the returned
/// slice is non-empty, [`Unknown`](ClearChargingProfileStatusEnumType::Unknown)
/// otherwise.
///
/// Since #550 the wiring layer feeds this the **combined** snapshot of all three
/// v201 charging-profile stores — the per-EVSE
/// [`V201TxProfileStore`](crate::v201_charging_profiles::V201TxProfileStore), the
/// [`V201TxDefaultProfileStore`](crate::v201_tx_default_profile::V201TxDefaultProfileStore),
/// and the
/// [`V201StationCeilingStore`](crate::v201_station_ceiling::V201StationCeilingStore)
/// — so a `ClearChargingProfile` can now retract an installed default or ceiling,
/// not just a `TxProfile` (the mirror of the #519 reporting fix). Each returned
/// pair carries the full profile so the wiring layer can route the removal to the
/// right store by the profile's
/// [`charging_profile_purpose`](ChargingProfileType::charging_profile_purpose) —
/// exactly as the `SetChargingProfile` install path routes each purpose to its
/// own store. This function stays pure over whatever snapshot it is given;
/// combining the stores and performing the removals is the wiring layer's job.
///
/// Matching is faithful to OCPP 2.0.1 (Part 2, `ClearChargingProfile`):
///
/// - **`chargingProfileId` present** — an exclusive selector (spec J01 note): the
///   `chargingProfileCriteria` are ignored and a slot matches iff its stored
///   `profile.id` equals it. A profile id that names nothing installed matches
///   nothing → `Unknown`.
/// - **`chargingProfileCriteria` present** (and no id) — a *filter*, each field
///   independently narrowing: `evseId` against the slot's EVSE key,
///   `chargingProfilePurpose` and `stackLevel` against the stored profile. An
///   absent field means "any", so it does not exclude. `evseId == 0` targets the
///   whole-station entries — over the combined set the default and ceiling stores
///   *do* hold `evseId = 0` profiles, so a `ClearChargingProfile(evseId = 0)` now
///   retracts exactly those and never a specific EVSE's `TxProfile`.
/// - **Neither present** (an empty `{}` request) — matches *every* installed
///   profile across all three stores (the "clear all" wildcard the message
///   documents).
///
/// Trust boundary: every selector field is compared by exact value against owned
/// snapshot data — a `0`, negative, or huge `evseId`/`stackLevel`/`profileId`
/// from the CSMS simply selects (or misses) by key and never panics.
///
/// Pure over its inputs (the selector plus an owned snapshot), so it is
/// unit-testable without a runtime or the store locks; taking the snapshot and
/// performing the removals is the wiring layer's job.
#[must_use]
pub fn v201_clear_charging_profile_matches(
    charging_profile_id: Option<i32>,
    criteria: Option<&ClearChargingProfileType>,
    installed: &[(i32, ChargingProfileType)],
) -> Vec<(i32, ChargingProfileType)> {
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
        .cloned()
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

/// Map a stored charging profile's
/// [`purpose`](ChargingProfilePurposeEnumType) to the
/// [`ChargingLimitSourceEnumType`] the station reports it under in
/// `ReportChargingProfiles` (and filters it by in a `GetChargingProfiles`
/// `chargingLimitSource` criterion).
///
/// OCPP 2.0.1 (Part 2) reports a charging limit's *origin*, not merely the API
/// that installed it. Every profile the simulator holds was installed by the
/// CSMS over `SetChargingProfile`, but the purpose records **whose constraint**
/// it represents:
///
/// - **`TxProfile` / `TxDefaultProfile` / `ChargingStationMaxProfile`** →
///   [`Cso`](ChargingLimitSourceEnumType::Cso). These are Charging Station
///   Operator configuration — the transaction schedule, the per-EVSE default,
///   and the operator's station ceiling — all authored by the CSO / back office.
/// - **`ChargingStationExternalConstraints`** →
///   [`So`](ChargingLimitSourceEnumType::So). This purpose models a ceiling
///   imposed by an actor *external* to the CSO — a distribution/grid System
///   Operator or an energy-management signal relayed through the CSMS. Of the
///   non-CSO sources (`EMS` / `Other` / `SO`), `SO` is the canonical mapping for
///   the grid/DSO external-constraint signal OCPP 2.0.1 Part 2 describes, so a
///   CSMS filtering `GetChargingProfiles` by source can tell an external ceiling
///   apart from operator configuration. (`EMS` would assert an
///   energy-management origin the simulator cannot substantiate; `SO` is the
///   faithful, least-surprising choice for the external-constraints purpose.)
///
/// Pure and total over the four-variant purpose enum, so it is the single source
/// of truth both the reporting pager and the query filter derive the wire source
/// from.
#[must_use]
pub fn v201_charging_limit_source(
    purpose: ChargingProfilePurposeEnumType,
) -> ChargingLimitSourceEnumType {
    match purpose {
        // An externally-imposed ceiling (grid / System Operator signal), distinct
        // from the operator's own configuration.
        ChargingProfilePurposeEnumType::ChargingStationExternalConstraints => {
            ChargingLimitSourceEnumType::So
        }
        // CSO (Charging Station Operator) configuration: the transaction schedule,
        // the per-EVSE default, and the operator's own station ceiling.
        ChargingProfilePurposeEnumType::TxProfile
        | ChargingProfilePurposeEnumType::TxDefaultProfile
        | ChargingProfilePurposeEnumType::ChargingStationMaxProfile => {
            ChargingLimitSourceEnumType::Cso
        }
    }
}

/// Select the installed `TxProfile` slots an inbound `GetChargingProfiles.req`
/// asks the station to report, returning the matching `(evse_id, profile)` pairs.
///
/// The query counterpart to [`v201_clear_charging_profile_matches`]: given the
/// request's optional top-level `evse_id` and its
/// [`ChargingProfileCriterionType`], plus a `(evse_id, profile)` snapshot of the
/// installed v201 charging profiles, it returns every installed slot that matches
/// — the wiring layer then streams them as `ReportChargingProfiles` and answers
/// [`Accepted`](GetChargingProfileStatusEnumType::Accepted) when the returned
/// slice is non-empty, [`NoProfiles`](GetChargingProfileStatusEnumType::NoProfiles)
/// otherwise.
///
/// Since #519 the wiring layer feeds this the **combined** snapshot of all three
/// v201 charging-profile stores — the per-EVSE
/// [`V201TxProfileStore`](crate::v201_charging_profiles::V201TxProfileStore), the
/// [`V201TxDefaultProfileStore`](crate::v201_tx_default_profile::V201TxDefaultProfileStore),
/// and the
/// [`V201StationCeilingStore`](crate::v201_station_ceiling::V201StationCeilingStore)
/// — so a `GetChargingProfiles` reports the station's full installed configuration
/// (transaction profiles, defaults, and ceilings), not just its `TxProfile`s. This
/// function stays pure over whatever snapshot it is given; combining the stores is
/// the wiring layer's job.
///
/// Matching is faithful to OCPP 2.0.1 (Part 2, `GetChargingProfiles`):
///
/// - **top-level `evseId`** — restricts the report to that EVSE key; absent means
///   "every EVSE". `evseId == 0` targets the station-wide entries — the default
///   and ceiling stores can hold an `evseId = 0` whole-station profile, so a
///   `GetChargingProfiles(evseId = 0)` reports exactly those, and a specific-EVSE
///   request (`>= 1`) never mis-scopes a whole-station `0` entry to itself. An
///   `evseId` absent from every store's snapshot is simply not reported. This is a
///   trust boundary on CSMS-supplied input: a `0`, negative, or huge `evseId`
///   never panics, it just selects (or misses) by exact key.
/// - **`chargingProfilePurpose`** — absent = any; present must equal the stored
///   profile's purpose.
/// - **`stackLevel`** — absent = any; present must equal the stored profile's
///   stack level.
/// - **`chargingProfileId`** — absent = any; present = the stored profile's `id`
///   must be one of the listed ids (an id list naming nothing installed matches
///   nothing).
/// - **`chargingLimitSource`** — absent = any; present must contain the profile's
///   own source, derived from its purpose by
///   [`v201_charging_limit_source`]: `TxProfile` / `TxDefaultProfile` /
///   `ChargingStationMaxProfile` report as
///   [`Cso`](ChargingLimitSourceEnumType::Cso) (operator configuration) while a
///   `ChargingStationExternalConstraints` ceiling reports as
///   [`So`](ChargingLimitSourceEnumType::So) (an external grid/DSO signal). So a
///   CSMS asking for `[Cso]` gets the operator profiles and not the external
///   ceiling, and `[SO]` gets only the external ceiling (#551) — the filter is no
///   longer a no-op across the combined store.
///
/// An empty criterion `{}` with no `evseId` matches every installed profile. Each
/// criterion field is an independent, conjunctive filter and an absent field is a
/// wildcard that never excludes — the same shape as
/// [`v201_clear_charging_profile_matches`].
///
/// Pure over its inputs (the request selector plus an owned snapshot), so it is
/// unit-testable without a runtime or the store lock; taking the snapshot and
/// streaming the report is the wiring layer's job.
#[must_use]
pub fn v201_get_charging_profiles_matches(
    evse_id: Option<i32>,
    criterion: &ChargingProfileCriterionType,
    installed: &[(i32, ChargingProfileType)],
) -> Vec<(i32, ChargingProfileType)> {
    installed
        .iter()
        .filter(|(slot_evse, profile)| {
            evse_id.is_none_or(|e| e == *slot_evse)
                && criterion
                    .charging_profile_purpose
                    .is_none_or(|p| p == profile.charging_profile_purpose)
                && criterion
                    .stack_level
                    .is_none_or(|s| s == profile.stack_level)
                && criterion
                    .charging_profile_id
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&profile.id))
                && criterion.charging_limit_source.as_ref().is_none_or(|srcs| {
                    srcs.contains(&v201_charging_limit_source(
                        profile.charging_profile_purpose,
                    ))
                })
        })
        .cloned()
        .collect()
}

/// Build a schema-valid `GetChargingProfiles.conf`
/// ([`GetChargingProfilesResponse`]).
///
/// Pure constructor mirroring [`v201_clear_charging_profile_response`]: the
/// station reports [`Accepted`](GetChargingProfileStatusEnumType::Accepted) when
/// at least one installed profile matched the query (`matched == true`) — those
/// profiles then stream asynchronously as `ReportChargingProfiles` — or
/// [`NoProfiles`](GetChargingProfileStatusEnumType::NoProfiles) when the selector
/// matched nothing installed, exactly the two-value contract
/// `ocpp.v201.enums.GetChargingProfileStatusEnumType` defines. No profile data
/// rides on this synchronous response.
#[must_use]
pub fn v201_get_charging_profiles_response(matched: bool) -> GetChargingProfilesResponse {
    GetChargingProfilesResponse {
        status: if matched {
            GetChargingProfileStatusEnumType::Accepted
        } else {
            GetChargingProfileStatusEnumType::NoProfiles
        },
        status_info: None,
        custom_data: None,
    }
}

/// Page the profiles a `GetChargingProfiles` query matched into the
/// `ReportChargingProfiles` CALL(s) the station streams back.
///
/// The asynchronous data half of the report flow:
/// [`v201_get_charging_profiles_matches`] resolves which installed slots to
/// report, and this builds one [`ReportChargingProfilesRequest`] per
/// **`(evseId, chargingLimitSource)`** — each echoing the triggering
/// `request_id`, tagged with that group's source (derived per profile by
/// [`v201_charging_limit_source`]), and carrying every matched profile of that
/// source on that EVSE. Pages are ordered by ascending `evse_id` then source,
/// and the profiles within a page by ascending `id` (the store snapshots are
/// unordered `HashMap` walks, so all sorts make the stream deterministic), and
/// every page but the last is flagged `tbc` ("to be continued"); the final page
/// leaves `tbc` absent (= `false`). An empty match set builds no pages — there
/// is nothing to stream.
///
/// Since #519 the match set is drawn from **three** stores — the per-EVSE
/// `TxProfile` store, the `TxDefaultProfile` store, and the station-ceiling
/// store — so one EVSE can carry several profiles (a `TxProfile`, a
/// `TxDefaultProfile`, and both `ChargingStationMaxProfile` /
/// `ChargingStationExternalConstraints` ceilings). OCPP 2.0.1 reports profiles
/// per `(chargingLimitSource, evseId)`, and a `ChargingStationExternalConstraints`
/// ceiling reports under the external [`So`](ChargingLimitSourceEnumType::So)
/// source while the operator profiles report under
/// [`Cso`](ChargingLimitSourceEnumType::Cso) (#551) — so an EVSE holding both an
/// operator profile and an external ceiling **splits into two pages with
/// distinct sources**. Grouping keeps each page's `chargingProfile` non-empty,
/// so every built `ReportChargingProfiles` satisfies the schema's `minItems: 1`.
///
/// Pure over its inputs, so it is unit-testable without a runtime; sending the
/// pages over the wire is the wiring layer's job.
#[must_use]
pub fn v201_report_charging_profiles_pages(
    request_id: i32,
    matched: &[(i32, ChargingProfileType)],
) -> Vec<ReportChargingProfilesRequest> {
    // Group the matched profiles per `(evse_id, chargingLimitSource)` — the unit
    // OCPP 2.0.1 reports a profile page under. A `BTreeMap` gives a deterministic
    // ascending-`(evse_id, source)` page order despite the unordered store
    // snapshots; the profiles within each page are sorted by `id` for the same
    // determinism.
    let mut by_key: BTreeMap<(i32, ChargingLimitSourceEnumType), Vec<ChargingProfileType>> =
        BTreeMap::new();
    for (evse_id, profile) in matched {
        let source = v201_charging_limit_source(profile.charging_profile_purpose);
        by_key
            .entry((*evse_id, source))
            .or_default()
            .push(profile.clone());
    }
    for profiles in by_key.values_mut() {
        profiles.sort_by_key(|profile| profile.id);
    }

    let last = by_key.len().saturating_sub(1);
    by_key
        .into_iter()
        .enumerate()
        .map(
            |(i, ((evse_id, charging_limit_source), charging_profile))| {
                ReportChargingProfilesRequest {
                    request_id,
                    charging_limit_source,
                    charging_profile,
                    evse_id,
                    // Every page but the last announces that more follow.
                    tbc: (i < last).then_some(true),
                    custom_data: None,
                }
            },
        )
        .collect()
}

/// The reservability of a `ReserveNow` target EVSE, distilled by the wiring
/// layer from the targeted connector's live `ChargePointStatus` (or its
/// absence).
///
/// Keeping this a small closed enum — rather than threading a
/// `ChargePointStatus` or a runtime
/// handle into [`v201_reserve_now_status`] — is what lets the reservation
/// decision stay a *pure* function of plain values, unit-testable without a
/// connector map. The wiring layer performs the one classification (idle → free,
/// anything else → the matching refusal) and the decision maps it onto the wire
/// status. Mirrors the shared [`UnlockConnectorOutcome`] seam
/// [`v201_unlock_status`] uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorReservState {
    /// The connector exists and is `Available` — reservable.
    Free,
    /// The connector exists but is in use (a transaction is under way, it is
    /// already `Reserved`, or it is otherwise mid-cycle:
    /// `Preparing`/`Charging`/`SuspendedEV`/`SuspendedEVSE`/`Finishing`).
    Busy,
    /// The connector exists but is `Faulted`.
    Faulted,
    /// The connector exists but is `Unavailable` (administratively inoperative).
    Unavailable,
    /// No connector answers the requested `evseId` — an id the station exposes
    /// no EVSE for (out of range, or `0` in the flat simulator topology).
    Missing,
}

/// Decide the [`ReserveNowStatusEnumType`] a `V201` station reports for an
/// inbound `ReserveNow.req`, given whether the reservation has already expired,
/// the requested `evse_id`, and the targeted EVSE's distilled live
/// [`state`](ConnectorReservState) — returning the decided
/// status together with the optional `statusInfo` explaining a refusal.
///
/// Faithful to OCPP 2.0.1 (Part 2, `ReserveNow`) and the direct 2.0.1 successor
/// to the 1.6J `ReserveNow` handler in [`crate`]'s `ChargePoint` (`lib.rs`),
/// keyed off the same connector-status matrix:
///
/// - **Already expired** — a reservation whose `expiryDateTime` is at or before
///   now would auto-free instantly, so it is
///   [`Rejected`](ReserveNowStatusEnumType::Rejected) outright (mirrors the 1.6J
///   handler's past-expiry guard, Issue #85), checked *first* so the reason
///   never depends on the target's live state.
/// - **Station-level (`evseId` omitted)** — 2.0.1 allows reserving the whole
///   station; the flat simulator topology holds no whole-station reservation, so
///   it is [`Rejected`](ReserveNowStatusEnumType::Rejected) with a
///   `StationLevel` `statusInfo` rather than silently reserving a
///   connector the CSMS did not name.
/// - **Structural (`evseId` &lt; 1)** — `0` / negative addresses no EVSE (2.0.1
///   evse ids are 1-based), so
///   [`Rejected`](ReserveNowStatusEnumType::Rejected) with an `UnknownEvse`
///   `statusInfo`, before the state read (mirrors the 1.6J `ConnectorId::new`
///   `Err` arm and [`v201_unlock_status`]'s structural guard).
/// - **Live state** — for a structurally-valid targeted EVSE the distilled
///   [`ConnectorReservState`] decides:
///   [`Free`](ConnectorReservState::Free) →
///   [`Accepted`](ReserveNowStatusEnumType::Accepted);
///   [`Busy`](ConnectorReservState::Busy) →
///   [`Occupied`](ReserveNowStatusEnumType::Occupied);
///   [`Faulted`](ConnectorReservState::Faulted) →
///   [`Faulted`](ReserveNowStatusEnumType::Faulted);
///   [`Unavailable`](ConnectorReservState::Unavailable) →
///   [`Unavailable`](ReserveNowStatusEnumType::Unavailable); and
///   [`Missing`](ConnectorReservState::Missing) (a structurally-valid id the
///   station has no EVSE for) →
///   [`Rejected`](ReserveNowStatusEnumType::Rejected) with an `UnknownEvse`
///   `statusInfo` — the same fold of "unknown target → Rejected" the 1.6J
///   handler applies.
///
/// This is the *pure* decision, depending only on the request target and a
/// distilled [`ConnectorReservState`] — no runtime handles, so it is
/// unit-testable in isolation. Resolving `evse_id` against the live connector
/// topology, recording the reservation, arming the auto-expiry timer, and
/// queueing the `Reserved` `StatusNotification` off the CALL path is the wiring
/// layer's job.
#[must_use]
pub fn v201_reserve_now_status(
    already_expired: bool,
    evse_id: Option<i32>,
    state: ConnectorReservState,
) -> (ReserveNowStatusEnumType, Option<StatusInfoType>) {
    // A reservation whose expiry has already passed would auto-free instantly —
    // reject before any state read (mirrors the 1.6J past-expiry guard, #85).
    if already_expired {
        return (
            ReserveNowStatusEnumType::Rejected,
            reserve_now_status_info(
                "ExpiredReservation",
                "ReserveNow.expiryDateTime is at or before now; the reservation would \
                 auto-free instantly",
            ),
        );
    }
    // Station-level reservation (evseId omitted): the flat simulator holds no
    // whole-station reservation to make.
    let Some(evse_id) = evse_id else {
        return (
            ReserveNowStatusEnumType::Rejected,
            reserve_now_status_info(
                "StationLevel",
                "ReserveNow without an evseId reserves the whole Charging Station; the \
                 simulator only reserves a specific EVSE",
            ),
        );
    };
    // Structural: 2.0.1 evse ids are 1-based; a value below 1 addresses no EVSE.
    if evse_id < 1 {
        return (
            ReserveNowStatusEnumType::Rejected,
            reserve_now_status_info(
                "UnknownEvse",
                "ReserveNow.evseId must name an EVSE the station exposes (2.0.1 evse ids \
                 are 1-based)",
            ),
        );
    }
    match state {
        ConnectorReservState::Free => (ReserveNowStatusEnumType::Accepted, None),
        ConnectorReservState::Busy => (ReserveNowStatusEnumType::Occupied, None),
        ConnectorReservState::Faulted => (ReserveNowStatusEnumType::Faulted, None),
        ConnectorReservState::Unavailable => (ReserveNowStatusEnumType::Unavailable, None),
        ConnectorReservState::Missing => (
            ReserveNowStatusEnumType::Rejected,
            reserve_now_status_info(
                "UnknownEvse",
                "ReserveNow.evseId is structurally valid but names no EVSE on this station",
            ),
        ),
    }
}

/// Build a `Some(StatusInfoType)` carrying a `ReserveNow` refusal reason.
///
/// A tiny helper so [`v201_reserve_now_status`]'s refusal arms read as one line
/// each; mirrors the inline `StatusInfoType { .. }` the
/// [`v201_set_charging_profile_status`] rejections build.
fn reserve_now_status_info(reason_code: &str, detail: &str) -> Option<StatusInfoType> {
    Some(StatusInfoType {
        reason_code: reason_code.to_string(),
        additional_info: Some(detail.to_string()),
        custom_data: None,
    })
}

/// Build a schema-valid `ReserveNow.conf` ([`ReserveNowResponse`]).
///
/// Pure constructor mirroring [`v201_unlock_response`]: carries the decided
/// [`status`](ReserveNowStatusEnumType) plus the optional 2.0.1 `statusInfo`
/// (a vendor-agnostic `reasonCode` and human-readable detail explaining a
/// refusal).
#[must_use]
pub fn v201_reserve_now_response(
    status: ReserveNowStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> ReserveNowResponse {
    ReserveNowResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Decide the [`CancelReservationStatusEnumType`] a `V201` station reports for an
/// inbound `CancelReservation.req`, given the requested `reservationId` and the
/// ids of every reservation the station currently holds.
///
/// Faithful to OCPP 2.0.1 (Part 2, `CancelReservation`) and a direct port of the
/// 1.6J `CancelReservation` handler's decision in [`crate`]'s `ChargePoint`,
/// re-expressed against the 2.0.1 [`CancelReservationStatusEnumType`]:
///
/// - [`Accepted`](CancelReservationStatusEnumType::Accepted) iff `requested`
///   equals one of `held_reservation_ids` — the CSMS named a reservation this
///   station is actually holding, so it can honor the cancel. This mirrors the
///   1.6J handler's `reservations … remove(&reservationId)` returning `Some`.
/// - [`Rejected`](CancelReservationStatusEnumType::Rejected) otherwise — an
///   unknown id, *or* a station holding no reservations at all
///   (`held_reservation_ids` empty), both fold to "no such reservation to
///   cancel", exactly as the 1.6J handler folds them into one `None => Rejected`
///   arm.
///
/// The comparison is a plain membership test over `i32` ids, so a negative,
/// zero, or `i32::MIN` `reservationId` simply fails to match and is `Rejected`,
/// never indexing or casting and never panicking — a trust boundary on the
/// CSMS-supplied id. This is the reserve/cancel analogue of
/// [`v201_request_stop_status`]'s member-of-the-live-set decision;
/// `CancelReservationStatusEnumType` has exactly `Accepted` / `Rejected` (no
/// deferred outcome), so — like it — this is a clean two-way split.
///
/// This is the *pure* decision, depending only on the requested id and the
/// station's held-id read — no runtime handles, so it is unit-testable in
/// isolation. Removing the reservation from the store (atomically, under the same
/// write-lock the status is read on, so a racing auto-expiry timer cannot make
/// the verdict and the removal disagree), freeing the connector, disarming the
/// expiry timer, and announcing `Available` off the CALL path is the wiring
/// layer's job.
#[must_use]
pub fn v201_cancel_reservation_status(
    requested: i32,
    held_reservation_ids: &[i32],
) -> CancelReservationStatusEnumType {
    if held_reservation_ids.contains(&requested) {
        CancelReservationStatusEnumType::Accepted
    } else {
        // Unknown id or a station holding no reservations — nothing to cancel.
        CancelReservationStatusEnumType::Rejected
    }
}

/// Build a schema-valid `CancelReservation.conf` ([`CancelReservationResponse`]).
///
/// Pure constructor mirroring [`v201_request_stop_response`]: carries the decided
/// [`status`](CancelReservationStatusEnumType) plus the optional 2.0.1
/// `statusInfo` (a vendor-agnostic `reasonCode` and human-readable detail —
/// useful, for example, to explain why a cancel was `Rejected`).
#[must_use]
pub fn v201_cancel_reservation_response(
    status: CancelReservationStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> CancelReservationResponse {
    CancelReservationResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Build a `ReservationStatusUpdate.req` ([`ReservationStatusUpdateRequest`])
/// reporting that reservation `reservation_id` is no longer valid because it
/// [`Expired`](ReservationUpdateStatusEnumType::Expired) (its `expiryDateTime`
/// passed) or was [`Removed`](ReservationUpdateStatusEnumType::Removed) (a
/// CSMS-initiated `CancelReservation` tore down a still-held reservation).
///
/// The CP→CSMS half that closes the reservation loop opened by
/// [`v201_reserve_now_response`] / [`v201_cancel_reservation_response`]: those
/// answer an inbound CALL, this *originates* a CALL when the station itself
/// frees a slot. Ports the request half of
/// [`ocpp.v201.call.ReservationStatusUpdate`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py)
/// (the response is empty, so there is no `.conf` builder — the sender only
/// awaits the ack).
///
/// `reservation_id` is copied into the message and never parsed or indexed, so
/// an extreme value (`i32::MIN`/`MAX`) cannot panic; `status` is
/// simulator-decided from the reservation lifecycle, never attacker input.
#[must_use]
pub fn v201_reservation_status_update(
    reservation_id: i32,
    status: ReservationUpdateStatusEnumType,
) -> ReservationStatusUpdateRequest {
    ReservationStatusUpdateRequest {
        reservation_id,
        reservation_update_status: status,
        custom_data: None,
    }
}

/// Decide the [`ClearCacheStatusEnumType`] a `V201` station reports for an
/// inbound `ClearCache.req`, given whether the station implements an
/// Authorization Cache at all.
///
/// - [`Accepted`](ClearCacheStatusEnumType::Accepted) when `supports_caching` —
///   the station has a cache and will empty it. This is the simulator's live
///   answer: it owns an [`AuthCache`](crate::auth_cache::AuthCache), so the clear
///   always executes.
/// - [`Rejected`](ClearCacheStatusEnumType::Rejected) otherwise — a station that
///   does not support caching has nothing to clear, so per OCPP 2.0.1 (Part 2,
///   D03) it refuses rather than silently no-op with an `Accepted`.
///
/// `ClearCache.req` carries no fields beyond the optional `customData`, so there
/// is no CSMS-supplied value to parse and no malformed-input branch — the
/// decision turns solely on this station capability. Modeling the `Rejected`
/// arm keeps the seam for a future opt-in "no caching" knob without reshaping the
/// decision. This is the *pure* decision; emptying the shared cache off the CALL
/// path is the wiring layer's job.
#[must_use]
pub fn v201_clear_cache_status(supports_caching: bool) -> ClearCacheStatusEnumType {
    if supports_caching {
        ClearCacheStatusEnumType::Accepted
    } else {
        ClearCacheStatusEnumType::Rejected
    }
}

/// Build a schema-valid `ClearCache.conf` ([`ClearCacheResponse`]).
///
/// Pure constructor mirroring [`v201_cancel_reservation_response`]: carries the
/// decided [`status`](ClearCacheStatusEnumType) plus the optional 2.0.1
/// `statusInfo` (a vendor-agnostic `reasonCode` and human-readable detail — for
/// example, to explain a `Rejected` on a station without a cache).
#[must_use]
pub fn v201_clear_cache_response(
    status: ClearCacheStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> ClearCacheResponse {
    ClearCacheResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Decide the [`GenericDeviceModelStatusEnumType`] a `V201` station reports for
/// an inbound `GetMonitoringReport.req`, from whether any installed monitor
/// matched the request's filters and whether the asynchronous
/// `NotifyMonitoringReport` could be queued.
///
/// Ports `ocpp.v201.call.GetMonitoringReport` → `NotifyMonitoringReport`. It is
/// the pure twin of the `GetReport` status mapping (#487): the station answers
/// synchronously here, then streams the matched monitors asynchronously as a
/// `NotifyMonitoringReport` CALL correlated by `requestId`. Three outcomes,
/// mirroring `GetReport`:
///
/// - `has_monitors == false` — the request was understood but nothing matched
///   (on today's simulator, always: no monitors are installed yet, issue #493
///   option b) → [`EmptyResultSet`](GenericDeviceModelStatusEnumType::EmptyResultSet),
///   and nothing is queued.
/// - `has_monitors && queued` — a non-empty snapshot was handed to the command
///   channel → [`Accepted`](GenericDeviceModelStatusEnumType::Accepted); the
///   `NotifyMonitoringReport` follows once this CALLRESULT is flushed.
/// - `has_monitors && !queued` — the command consumer has gone away (CP shutting
///   down), so the station cannot stream the report it would otherwise promise →
///   [`Rejected`](GenericDeviceModelStatusEnumType::Rejected).
#[must_use]
pub fn v201_get_monitoring_report_status(
    has_monitors: bool,
    queued: bool,
) -> GenericDeviceModelStatusEnumType {
    if !has_monitors {
        GenericDeviceModelStatusEnumType::EmptyResultSet
    } else if queued {
        GenericDeviceModelStatusEnumType::Accepted
    } else {
        GenericDeviceModelStatusEnumType::Rejected
    }
}

/// Build a schema-valid `GetMonitoringReport.conf`
/// ([`GetMonitoringReportResponse`]).
///
/// Pure constructor mirroring [`v201_cancel_reservation_response`]: carries the
/// decided [`status`](GenericDeviceModelStatusEnumType) plus the optional 2.0.1
/// `statusInfo` (a vendor-agnostic `reasonCode` and human-readable detail).
#[must_use]
pub fn v201_get_monitoring_report_response(
    status: GenericDeviceModelStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> GetMonitoringReportResponse {
    GetMonitoringReportResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Build a schema-valid `SetMonitoringLevel.conf` ([`SetMonitoringLevelResponse`]).
///
/// Pure constructor mirroring [`v201_get_monitoring_report_response`]: carries
/// the station's accept/reject decision on the requested reporting level plus
/// the optional 2.0.1 `statusInfo`. `status_info` is `None` on
/// [`Accepted`](GenericStatusEnumType::Accepted) and carries the machine-readable
/// reason (an out-of-range `severity`) on [`Rejected`](GenericStatusEnumType::Rejected).
///
/// Ports `ocpp.v201.call_result.SetMonitoringLevel`.
#[must_use]
pub fn v201_set_monitoring_level_response(
    status: GenericStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> SetMonitoringLevelResponse {
    SetMonitoringLevelResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Build a schema-valid `SetMonitoringBase.conf` ([`SetMonitoringBaseResponse`]).
///
/// Pure constructor mirroring [`v201_set_monitoring_level_response`]: carries the
/// station's decision on the requested monitoring base as a
/// [`GenericDeviceModelStatusEnumType`] plus the optional 2.0.1 `statusInfo`.
/// `status_info` is `None` on [`Accepted`](GenericDeviceModelStatusEnumType::Accepted)
/// and carries the machine-readable reason (an unmodeled `HardWiredOnly` base) on
/// [`NotSupported`](GenericDeviceModelStatusEnumType::NotSupported).
///
/// Ports `ocpp.v201.call_result.SetMonitoringBase`.
#[must_use]
pub fn v201_set_monitoring_base_response(
    status: GenericDeviceModelStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> SetMonitoringBaseResponse {
    SetMonitoringBaseResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Decide the `(messagesInQueue, ongoingIndicator)` pair a `V201` station reports
/// for an inbound `GetTransactionStatus.req`, from the request's optional
/// `transactionId`, the ids of every transaction the station currently has live,
/// and whether the station still has messages queued for delivery.
///
/// Ports `ocpp.v201.call.GetTransactionStatus` /
/// `ocpp.v201.call_result.GetTransactionStatus` (OCPP 2.0.1 Part 2, E13). There
/// is no 1.6J equivalent — this is a 2.0.1-only message the CSMS uses (e.g. after
/// a reconnect) to learn whether a transaction is still ongoing and/or whether
/// the station still has undelivered messages for it before deciding to clean up.
///
/// The `ongoingIndicator` mirrors [`v201_request_stop_status`]'s
/// member-of-the-live-set test, but reported rather than acted on:
///
/// - **`transactionId` present** — `ongoingIndicator = Some(true)` iff `requested`
///   equals one of `live_transaction_ids` (the station is actually running it);
///   otherwise `Some(false)` — an unknown id, an id whose transaction has already
///   ended, or an idle station all fold to the modeled "not ongoing" answer, the
///   same fold the stop decision applies. Matching is exact string equality over
///   the station-minted decimal id, never a numeric parse: a malformed or
///   non-canonical id (`"07"`, whitespace, a huge value) simply fails to match and
///   reports `Some(false)`, never panicking — a trust boundary on the
///   CSMS-supplied id.
/// - **`transactionId` omitted (station-wide query)** — `ongoingIndicator = None`:
///   the query concerns only the station's queued-message state, so the response
///   field is left absent (it is optional for exactly this case) rather than
///   fabricating a per-transaction verdict.
///
/// `messages_in_queue` is reported through unchanged. The simulator does not yet
/// buffer offline messages, so the wiring layer passes `false` today (a modeled
/// answer, documented at the call site); accepting it as an input keeps this pure
/// and lets a future outbound queue flip it without reshaping the decision.
///
/// This is the *pure* decision, depending only on the request and two plain reads
/// — no runtime handles, so it is unit-testable in isolation. Resolving the live
/// ids from the transaction table off the CALL path is the wiring layer's job.
#[must_use]
pub fn v201_get_transaction_status(
    requested: Option<&str>,
    live_transaction_ids: &[&str],
    messages_in_queue: bool,
) -> (bool, Option<bool>) {
    // A present id maps to a membership verdict; an omitted id (station-wide
    // query) leaves the optional indicator absent.
    let ongoing_indicator = requested.map(|id| live_transaction_ids.contains(&id));
    (messages_in_queue, ongoing_indicator)
}

/// Build a schema-valid `GetTransactionStatus.conf`
/// ([`GetTransactionStatusResponse`]).
///
/// Pure constructor mirroring [`v201_request_stop_response`]: carries the required
/// [`messages_in_queue`](GetTransactionStatusResponse::messages_in_queue) flag and
/// the optional [`ongoing_indicator`](GetTransactionStatusResponse::ongoing_indicator)
/// — both decided by [`v201_get_transaction_status`]. Unlike the command builders
/// this response carries no `statusInfo` (the message defines none), only the two
/// status fields plus the vendor `customData` extension.
#[must_use]
pub fn v201_get_transaction_status_response(
    messages_in_queue: bool,
    ongoing_indicator: Option<bool>,
) -> GetTransactionStatusResponse {
    GetTransactionStatusResponse {
        messages_in_queue,
        ongoing_indicator,
        custom_data: None,
    }
}

/// Decide the [`DisplayMessageStatusEnumType`] a `V201` station reports for an
/// inbound `SetDisplayMessage.req`, given the [`message`](MessageInfoType) to
/// install and the ids of every transaction the station currently has live —
/// returning the decided status together with the optional `statusInfo`
/// explaining a refusal.
///
/// Faithful to OCPP 2.0.1 (Part 2, E05–E08, `SetDisplayMessage`):
///
/// - **Transaction-bound message referencing an unknown transaction** — when the
///   message carries a `transactionId` (`MessageInfoType.transaction_id`) that is
///   **not** one of `live_transaction_ids`, the station cannot scope the message
///   to a session it is not running, so it is
///   [`UnknownTransaction`](DisplayMessageStatusEnumType::UnknownTransaction) with
///   a `NoTransaction` `statusInfo`, checked *first* — the message is **not**
///   installed. Matching is exact string equality over the station-minted decimal
///   id, the same member-of-the-live-set test
///   [`v201_request_stop_status`] / [`v201_get_transaction_status`] use, so a
///   non-canonical spelling, whitespace, or a huge value simply misses and is
///   `UnknownTransaction`, never parsed and never panicking — a trust boundary on
///   the CSMS-supplied id.
/// - **Otherwise** — a schema-valid message the simulator can model (a
///   station-wide message, or one bound to a live transaction) →
///   [`Accepted`](DisplayMessageStatusEnumType::Accepted), no `statusInfo`. The
///   wiring layer then upserts it into the display-message store by
///   `MessageInfoType.id`.
///
/// The remaining `DisplayMessageStatusEnumType` variants
/// (`NotSupportedMessageFormat`, `NotSupportedPriority`, `NotSupportedState`,
/// `Rejected`) are **documented modeled seams** the simulator does not produce: a
/// simulated display can render any schema-valid `MessageFormatEnumType`,
/// `MessagePriorityEnumType`, and `MessageStateEnumType`, so it accepts every
/// well-formed `MessageInfoType`. They stay in the ported status enum for the wire
/// — a real station with a fixed-format display, or a future opt-in capability
/// knob, maps to them — mirroring how the monitor store (#494) documents its
/// unproduced statuses. The three enum fields are typed, so an unknown wire value
/// fails deserialization (rejected as a CALLERROR) before reaching this decision.
///
/// This is the *pure* decision, depending only on the message and the station's
/// live-id read — no runtime handles, so it is unit-testable in isolation.
/// Resolving the live ids from the transaction table and performing the upsert off
/// the CALL path is the wiring layer's job.
#[must_use]
pub fn v201_set_display_message_status(
    message: &MessageInfoType,
    live_transaction_ids: &[&str],
) -> (DisplayMessageStatusEnumType, Option<StatusInfoType>) {
    // A message may bind itself to a transaction; if it names one the station is
    // not running, there is nothing to scope it to — refuse before installing.
    if let Some(transaction_id) = message.transaction_id.as_deref() {
        if !live_transaction_ids.contains(&transaction_id) {
            return (
                DisplayMessageStatusEnumType::UnknownTransaction,
                Some(StatusInfoType {
                    reason_code: "NoTransaction".to_string(),
                    additional_info: Some(
                        "SetDisplayMessage.message.transactionId does not name a \
                         transaction the station is currently running"
                            .to_string(),
                    ),
                    custom_data: None,
                }),
            );
        }
    }
    (DisplayMessageStatusEnumType::Accepted, None)
}

/// Build a schema-valid `SetDisplayMessage.conf` ([`SetDisplayMessageResponse`]).
///
/// Pure constructor mirroring [`v201_set_charging_profile_response`]: carries the
/// decided [`status`](DisplayMessageStatusEnumType) plus the optional 2.0.1
/// `statusInfo` (a vendor-agnostic `reasonCode` and human-readable detail —
/// useful, for example, to explain why an install was `UnknownTransaction`).
///
/// Ports `ocpp.v201.call_result.SetDisplayMessage`.
#[must_use]
pub fn v201_set_display_message_response(
    status: DisplayMessageStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> SetDisplayMessageResponse {
    SetDisplayMessageResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Resolve which installed display messages a `GetDisplayMessages` query matches,
/// given its optional `id` / `priority` / `state` selectors and an owned
/// `snapshot()` of the [`V201DisplayMessageStore`](crate::v201_display_message::V201DisplayMessageStore).
///
/// The query half of the display-message family (ports
/// [`ocpp.v201.call.GetDisplayMessages`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py)),
/// mirroring [`v201_get_charging_profiles_matches`]: each selector is an
/// independent, **conjunctive** filter and an absent (`None`) selector is a
/// wildcard that never excludes. An empty query `{}` (all selectors absent) thus
/// matches every installed message.
///
/// - **`id`** — absent = any; present = the message's [`id`](MessageInfoType::id)
///   must be one of the listed ids (an id list naming nothing installed matches
///   nothing). The schema guarantees a present list is non-empty (`minItems` 1).
/// - **`priority`** — absent = any; present must equal the message's
///   (required) [`priority`](MessageInfoType::priority).
/// - **`state`** — absent = any; present matches a message whose
///   [`state`](MessageInfoType::state) equals it **or** whose state is `None`. A
///   stateless message is displayed in *every* station state (OCPP 2.0.1
///   `MessageInfoType.state`: "When omitted this message should be shown in any
///   state"), so a "which messages show in state X" query must include it —
///   faithful to the spec rather than a literal field-equality.
///
/// Pure over its inputs (the request selectors plus an owned snapshot), so it is
/// unit-testable without a runtime or the store lock; taking the snapshot and
/// streaming the report is the wiring layer's job.
#[must_use]
pub fn v201_get_display_messages_matches(
    id: Option<&[i32]>,
    priority: Option<MessagePriorityEnumType>,
    state: Option<MessageStateEnumType>,
    installed: &[MessageInfoType],
) -> Vec<MessageInfoType> {
    installed
        .iter()
        .filter(|message| {
            id.is_none_or(|ids| ids.contains(&message.id))
                && priority.is_none_or(|p| p == message.priority)
                // A stateless message shows in any state, so a state query includes it.
                && state.is_none_or(|s| message.state.is_none_or(|ms| ms == s))
        })
        .cloned()
        .collect()
}

/// Build a schema-valid `GetDisplayMessages.conf` ([`GetDisplayMessagesResponse`]).
///
/// Pure constructor mirroring [`v201_get_charging_profiles_response`]: the station
/// reports [`Accepted`](GetDisplayMessagesStatusEnumType::Accepted) when at least
/// one installed message matched the query (`matched == true`) — those messages
/// then stream asynchronously as `NotifyDisplayMessages` — or
/// [`Unknown`](GetDisplayMessagesStatusEnumType::Unknown) when the selector
/// matched nothing installed, exactly the two-value contract
/// `ocpp.v201.enums.GetDisplayMessagesStatusEnumType` defines. No message data
/// rides on this synchronous response.
#[must_use]
pub fn v201_get_display_messages_response(matched: bool) -> GetDisplayMessagesResponse {
    GetDisplayMessagesResponse {
        status: if matched {
            GetDisplayMessagesStatusEnumType::Accepted
        } else {
            GetDisplayMessagesStatusEnumType::Unknown
        },
        status_info: None,
        custom_data: None,
    }
}

/// Page the messages a `GetDisplayMessages` query matched into the
/// `NotifyDisplayMessages` CALL(s) the station streams back.
///
/// The asynchronous data half of the query flow (ports
/// [`ocpp.v201.call.NotifyDisplayMessages`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py)):
/// [`v201_get_display_messages_matches`] resolves which installed messages to
/// report, and this builds one [`NotifyDisplayMessagesRequest`] per message —
/// each echoing the triggering `request_id`. Pages are ordered by ascending
/// [`id`](MessageInfoType::id) (the store snapshot is an unordered `HashMap` walk,
/// so sorting makes the paging deterministic), and every page but the last is
/// flagged `tbc` ("to be continued"); the final page leaves `tbc` absent
/// (= `false`). An empty match set builds no pages — there is nothing to stream.
///
/// One message per page (mirroring [`v201_report_charging_profiles_pages`]'s
/// one-profile-per-page shape) keeps each page's `messageInfo` at exactly one
/// item — always satisfying the schema's `minItems: 1` — and makes the `tbc`
/// paging observable. Multi-message-per-page batching is a later refinement if a
/// station's installed set ever outgrows a single frame; the CSMS reassembles by
/// `request_id` regardless of how the messages are split across pages.
///
/// Pure over its inputs, so it is unit-testable without a runtime; sending the
/// pages over the wire is the wiring layer's job.
#[must_use]
pub fn v201_notify_display_messages_pages(
    request_id: i32,
    matched: &[MessageInfoType],
) -> Vec<NotifyDisplayMessagesRequest> {
    // Deterministic paging order: the store snapshot is a HashMap walk.
    let mut ordered: Vec<&MessageInfoType> = matched.iter().collect();
    ordered.sort_by_key(|message| message.id);

    let last = ordered.len().saturating_sub(1);
    ordered
        .into_iter()
        .enumerate()
        .map(|(i, message)| NotifyDisplayMessagesRequest {
            request_id,
            message_info: Some(vec![message.clone()]),
            // Every page but the last announces that more follow.
            tbc: (i < last).then_some(true),
            custom_data: None,
        })
        .collect()
}

/// The deterministic, simulator-generated customer-data pages a
/// `CustomerInformation(report: true)` report streams back.
///
/// The Charging Station *simulator* holds no real customer store — there is no
/// PII to enumerate — so an accepted reporting request replies with a small,
/// fixed set of human-readable lines that identify the report as simulated and
/// stand in for the stored-data body a production station would emit. Split into
/// more than one page so the `NotifyCustomerInformation` paging (`seqNo` / `tbc`)
/// the flow exists to exercise is observable end-to-end. Each line is a short
/// constant well under the schema's `maxLength: 512`, so no page can overflow.
///
/// Kept as a standalone `const` (rather than inlined into the page builder) so a
/// test can assert the page bounds directly, and so the simulated body is a
/// single documented seam a future "real customer store" slice would replace.
pub const V201_SIMULATED_CUSTOMER_INFORMATION_PAGES: &[&str] = &[
    "ocpp-rs charge point simulator: simulated CustomerInformation report. This \
     station holds no real customer data store (no PII).",
    "customerData: <none on record>. Emitted deterministically to exercise the \
     NotifyCustomerInformation report/clear flow.",
];

/// Build the `NotifyCustomerInformation.req` pages a
/// `CustomerInformation(report: true)` report streams back — the flat-text twin
/// of [`v201_notify_display_messages_pages`] / [`v201_report_charging_profiles_pages`].
///
/// Ports the paged carrier of
/// [`ocpp.v201.call.NotifyCustomerInformation`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py):
/// one [`NotifyCustomerInformationRequest`] per `data` page, every page echoing
/// the triggering `request_id` so the CSMS can correlate the stream. Pages are
/// numbered by [`seq_no`](NotifyCustomerInformationRequest::seq_no) from 0 in
/// input order, and every page but the last is flagged
/// [`tbc`](NotifyCustomerInformationRequest::tbc) ("to be continued"); the final
/// page leaves `tbc` absent (= `false`). The `generated_at` timestamp is taken
/// as an input string (the wiring layer stamps it with the current time) so this
/// builder stays pure over its inputs and unit-testable without a clock — the
/// same split the `NotifyReport` emitter uses.
///
/// An empty `data_pages` slice builds no pages (nothing to stream); the caller
/// only invokes this for an accepted reporting request, which always supplies at
/// least one simulated page.
///
/// `request_id` is copied into every page and never parsed or indexed, so an
/// extreme value (`i32::MIN`/`MAX`) cannot panic; each `data` page is
/// caller-supplied simulated text, never attacker input.
#[must_use]
pub fn v201_notify_customer_information_pages(
    request_id: i32,
    generated_at: &str,
    data_pages: &[&str],
) -> Vec<NotifyCustomerInformationRequest> {
    let last = data_pages.len().saturating_sub(1);
    data_pages
        .iter()
        .enumerate()
        .map(|(i, data)| NotifyCustomerInformationRequest {
            data: (*data).to_string(),
            // Every page but the last announces that more follow.
            tbc: (i < last).then_some(true),
            seq_no: i32::try_from(i).unwrap_or(i32::MAX),
            generated_at: generated_at.to_string(),
            request_id,
            custom_data: None,
        })
        .collect()
}

/// Build a schema-valid `ClearDisplayMessage.conf` ([`ClearDisplayMessageResponse`]).
///
/// Pure constructor mirroring [`v201_clear_charging_profile_response`]: the
/// teardown half of the display-message family (ports
/// [`ocpp.v201.call_result.ClearDisplayMessage`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call_result.py)).
/// The station reports [`Accepted`](ClearMessageStatusEnumType::Accepted) when a
/// message with the requested id existed and was removed (`removed == true`), or
/// [`Unknown`](ClearMessageStatusEnumType::Unknown) when the id named nothing
/// installed — exactly the two-value contract
/// `ocpp.v201.enums.ClearMessageStatusEnumType` defines. No detail rides on
/// either arm; `remove` is a single side effect the wiring layer applies before
/// calling this.
#[must_use]
pub fn v201_clear_display_message_response(removed: bool) -> ClearDisplayMessageResponse {
    ClearDisplayMessageResponse {
        status: if removed {
            ClearMessageStatusEnumType::Accepted
        } else {
            ClearMessageStatusEnumType::Unknown
        },
        status_info: None,
        custom_data: None,
    }
}

/// Build the (empty) `CostUpdated` acknowledgement.
///
/// Ports [`ocpp.v201.call_result.CostUpdated`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call_result.py):
/// OCPP 2.0.1 Part 2, K (Tariff & Cost) defines no fields and no rejection
/// status for the response, so the station always acknowledges with an empty
/// body (`{}` on the wire, only the optional vendor extension). A builder is
/// kept — rather than inlining the struct literal at the call site — for
/// symmetry with the family's other response builders and to give the
/// schema-validity test a single named target.
#[must_use]
pub fn v201_cost_updated_response() -> CostUpdatedResponse {
    CostUpdatedResponse { custom_data: None }
}

/// Decide whether the station accepts, refuses, or fails to install the root
/// certificate an `InstallCertificate` delivered (OCPP 2.0.1 Part 2, A02 /
/// M03–M05).
///
/// Ports the accept/reject/fail decision behind
/// [`ocpp.v201.call_result.InstallCertificate`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call_result.py)'s
/// [`InstallCertificateStatusEnumType`]. The simulator models the *protocol*
/// decision, not a PKI, so this is a deliberately lightweight predicate on the
/// PEM string — **no X.509 parse, no chain/signature verification** (a documented
/// boundary; a real validation seam is a natural follow-up). `certificate` is
/// untrusted CSMS input, so it is treated as an opaque, bounded string and is
/// only ever inspected, never parsed or unwrapped — no wire value can panic.
///
/// The three wire statuses are distinguished by the certificate's *shape*, so all
/// of `Accepted` / `Rejected` / `Failed` are reachable and unit-testable:
///
/// - **`Rejected`** — the station refuses up front: the certificate is empty /
///   whitespace-only, or is not PEM-armored at all (missing the
///   `-----BEGIN … -----` / `-----END … -----` markers). There is nothing that
///   looks like a certificate to install.
/// - **`Failed`** — the station recognized a PEM certificate and *attempted* the
///   install, but it could not complete: the certificate is PEM-armored yet
///   carries no key material between the markers (an empty body). This is the
///   "attempted but did not complete" arm the spec distinguishes from an up-front
///   refusal.
/// - **`Accepted`** — a PEM-armored certificate with a non-empty body; the
///   station installs it.
#[must_use]
pub fn v201_install_certificate_status(certificate: &str) -> InstallCertificateStatusEnumType {
    let trimmed = certificate.trim();

    // Nothing to install, or not a PEM certificate at all → refuse up front.
    if trimmed.is_empty() || !trimmed.contains("-----BEGIN") || !trimmed.contains("-----END") {
        return InstallCertificateStatusEnumType::Rejected;
    }

    // PEM-armored, but is there any key material between the markers? The armor
    // lines (`-----BEGIN … -----`, `-----END … -----`) are stripped; whatever
    // remains, minus whitespace, is the base64 body.
    let has_body = trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("-----"))
        .any(|line| !line.is_empty());

    if has_body {
        InstallCertificateStatusEnumType::Accepted
    } else {
        // Recognized as a certificate but unusable — attempted, could not complete.
        InstallCertificateStatusEnumType::Failed
    }
}

/// Build a schema-valid `InstallCertificate.conf` ([`InstallCertificateResponse`]).
///
/// Pure constructor mirroring [`v201_set_display_message_response`]: carries the
/// decided [`status`](InstallCertificateStatusEnumType) plus the optional 2.0.1
/// `statusInfo` — a vendor-agnostic `reasonCode` and human-readable detail the
/// handler attaches to a non-`Accepted` outcome (why the install was `Rejected`
/// or `Failed`).
///
/// Ports `ocpp.v201.call_result.InstallCertificate`.
#[must_use]
pub fn v201_install_certificate_response(
    status: InstallCertificateStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> InstallCertificateResponse {
    InstallCertificateResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Decide the [`SetNetworkProfileStatusEnumType`] a `SetNetworkProfile.req`
/// answers, given the requested profile.
///
/// Ports the accept/refuse decision of `ocpp.v201.call.SetNetworkProfile`
/// (OCPP 2.0.1 Part 2, provisioning, B09/B10). A lightweight, pure predicate on
/// the [`NetworkConnectionProfileType`](ocpp_types::v201::NetworkConnectionProfileType) —
/// **no real network dial** (the documented simulator boundary: the station
/// never actually connects with the profile), mirroring how
/// [`v201_install_certificate_status`] inspects a PEM without an X.509 parse.
///
/// - **`Rejected`** — the profile is unusable up front: its `ocppCsmsUrl` is
///   empty or whitespace-only, so it names no CSMS the station could ever reach.
///   The schema permits an empty `ocppCsmsUrl` (`type: string, maxLength: 512`,
///   no `minLength`), so this refusal is genuinely reachable from a
///   schema-valid wire frame rather than pre-filtered to a CALLERROR — a real
///   station refuses a provisioning it cannot act on.
/// - **`Accepted`** — a profile naming a non-blank CSMS URL; the station stores
///   it in the requested `configurationSlot`.
/// - **`Failed`** is *not* produced here. It is the spec's "accepted the profile
///   but could not apply it" runtime arm — a station that stored the slot yet
///   could not bring the interface up. The simulator's store never fails to
///   accept a well-formed profile, so no schema-valid input forces `Failed`
///   deterministically; it is kept as a documented seam, exercised at the
///   response-builder + schema level (all three statuses) rather than through
///   `handle_message`. This mirrors the unproduced-status convention `GetLog`
///   (`Rejected`) and `DeleteCertificate` (`Failed`, a race arm) follow.
#[must_use]
pub fn v201_set_network_profile_decision(
    request: &SetNetworkProfileRequest,
) -> SetNetworkProfileStatusEnumType {
    if request.connection_data.ocpp_csms_url.trim().is_empty() {
        // Nothing to connect to — the profile names no reachable CSMS.
        SetNetworkProfileStatusEnumType::Rejected
    } else {
        SetNetworkProfileStatusEnumType::Accepted
    }
}

/// Build a schema-valid `SetNetworkProfile.conf` ([`SetNetworkProfileResponse`]).
///
/// Pure constructor mirroring [`v201_install_certificate_response`]: carries the
/// decided [`status`](SetNetworkProfileStatusEnumType) plus the optional 2.0.1
/// `statusInfo` — a vendor-agnostic `reasonCode` and human-readable detail the
/// handler attaches to a non-`Accepted` outcome (why the profile was `Rejected`).
///
/// Ports `ocpp.v201.call_result.SetNetworkProfile`.
#[must_use]
pub fn v201_set_network_profile_response(
    status: SetNetworkProfileStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> SetNetworkProfileResponse {
    SetNetworkProfileResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Synthesize the deterministic `filename` a `GetLog` upload will produce for
/// `(log_type, request_id)`.
///
/// The simulator has no real log file to name, so it mints a stable one: the log
/// kind (`diagnostics` / `security`) joined to the request's `requestId`, e.g.
/// `diagnostics_42.log`. Deterministic — the same inputs always yield the same
/// name, so a retry of the same `GetLog` reports the same `filename` — and always
/// non-empty. The result is far under the schema's `filename` `maxLength: 255`:
/// the longest possible spelling, `diagnostics_-2147483648.log` (an `i32::MIN`
/// request id), is 27 characters, so no wire `requestId` can overflow the bound.
#[must_use]
pub fn v201_log_filename(log_type: LogEnumType, request_id: i32) -> String {
    let kind = match log_type {
        LogEnumType::DiagnosticsLog => "diagnostics",
        LogEnumType::SecurityLog => "security",
    };
    format!("{kind}_{request_id}.log")
}

/// Decide how a `for_version(V201)` station answers a `GetLog.req`, given the
/// request and the `requestId` of any upload already in flight.
///
/// A station uploads one log at a time, so the answer turns on whether — and
/// which — request is currently in flight (the caller reads that from the
/// [`V201LogUploadStore`](crate::v201_log_upload::V201LogUploadStore); `None` is
/// idle):
///
/// - **idle** (`in_flight` is `None`) → [`Accepted`](LogStatusEnumType::Accepted):
///   a fresh upload starts;
/// - **retry** (`in_flight` names this same `requestId`) →
///   [`Accepted`](LogStatusEnumType::Accepted): idempotently the same answer, the
///   same `filename`, no second upload and no cancellation — a `GetLog` retried
///   under its original `requestId` must not report a spurious cancel;
/// - **supersede** (`in_flight` names a *different* `requestId`) →
///   [`AcceptedCanceled`](LogStatusEnumType::AcceptedCanceled): the new request is
///   accepted and the in-progress upload is canceled to serve it, so a CSMS can
///   always kick off a fresh log collection.
///
/// The returned `filename` is [`v201_log_filename`]'s deterministic name and is
/// present on every arm (all three are accepts that will upload).
/// [`Rejected`](LogStatusEnumType::Rejected) is a **documented modeled seam** this
/// simulator does not produce: a real station that refuses concurrent uploads
/// outright (rather than superseding) would answer it, and it stays in the ported
/// status enum for the wire and the response builder's schema coverage —
/// mirroring how [`v201_set_display_message_status`] documents its unproduced
/// `NotSupported*` / `Rejected` statuses.
///
/// This is the *pure* decision, depending only on the request and the in-flight
/// snapshot — no runtime handles, no store lock — so it is unit-testable in
/// isolation. Recording the accepted request as the new in-flight upload (the
/// side effect) is the wiring layer's job. Trust boundary: `request.log`
/// (carrying the untrusted `remoteLocation`) is never read here, and `request_id`
/// is only compared and formatted, never parsed or indexed, so no wire value
/// (including `i32::MIN`/`MAX`) can panic. Ports `ocpp.v201.call.GetLog` →
/// `ocpp.v201.call_result.GetLog`.
#[must_use]
pub fn v201_get_log_decision(
    request: &GetLogRequest,
    in_flight: Option<i32>,
) -> (LogStatusEnumType, Option<String>) {
    let filename = v201_log_filename(request.log_type, request.request_id);
    let status = match in_flight {
        None => LogStatusEnumType::Accepted,
        Some(current) if current == request.request_id => LogStatusEnumType::Accepted,
        Some(_) => LogStatusEnumType::AcceptedCanceled,
    };
    (status, Some(filename))
}

/// Build a schema-valid `GetLog.conf` ([`GetLogResponse`]).
///
/// Pure constructor mirroring [`v201_install_certificate_response`]: carries the
/// decided [`status`](LogStatusEnumType), the optional 2.0.1 `statusInfo`, and the
/// `filename` the station will produce (present on an accept, absent on a refusal
/// — the `maxLength: 255` bound is enforced at the schema layer).
///
/// Ports `ocpp.v201.call_result.GetLog`.
#[must_use]
pub fn v201_get_log_response(
    status: LogStatusEnumType,
    status_info: Option<StatusInfoType>,
    filename: Option<String>,
) -> GetLogResponse {
    GetLogResponse {
        status,
        status_info,
        filename,
        custom_data: None,
    }
}

/// Decide how a `for_version(V201)` station answers an `UpdateFirmware.req`, given
/// the request and the `requestId` of any firmware update already in flight.
///
/// `UpdateFirmware` asks the station to fetch and install a firmware image named
/// by a `FirmwareType` (download `location`, retrieve/install timestamps, and an
/// optional signing certificate + signature). The decision has two independent
/// axes, checked in this precedence:
///
/// 1. **Signing certificate.** If `firmware.signing_certificate` is *present* but
///    not a usable PEM certificate, the image cannot be trusted, so the request is
///    refused outright with
///    [`InvalidCertificate`](UpdateFirmwareStatusEnumType::InvalidCertificate) —
///    regardless of the in-flight state, and nothing is recorded. "Usable" reuses
///    the same **no-X.509-parse** boundary as
///    [`v201_install_certificate_status`]: a present certificate passes only when
///    it is PEM-armored with a non-empty body; empty / whitespace-only / non-PEM /
///    empty-bodied all take the `InvalidCertificate` arm. An *absent* certificate
///    (the common case — the image is unsigned or the signature rides elsewhere)
///    skips this axis entirely.
/// 2. **In-flight state** (a station runs one update at a time; the caller reads
///    the in-flight `requestId` from the
///    [`V201FirmwareUpdateStore`](crate::v201_firmware_update::V201FirmwareUpdateStore),
///    `None` is idle):
///    - **idle** (`in_flight` is `None`) →
///      [`Accepted`](UpdateFirmwareStatusEnumType::Accepted): a fresh update
///      starts;
///    - **retry** (`in_flight` names this same `requestId`) →
///      [`Accepted`](UpdateFirmwareStatusEnumType::Accepted): idempotently the same
///      answer, no second update and no cancellation — an `UpdateFirmware` retried
///      under its original `requestId` must not report a spurious cancel;
///    - **supersede** (`in_flight` names a *different* `requestId`) →
///      [`AcceptedCanceled`](UpdateFirmwareStatusEnumType::AcceptedCanceled): the
///      new request is accepted and the in-progress update is canceled to serve it.
///
/// [`Rejected`](UpdateFirmwareStatusEnumType::Rejected) (a policy refusal the
/// simulator does not model) and
/// [`RevokedCertificate`](UpdateFirmwareStatusEnumType::RevokedCertificate) (the
/// simulator holds no revocation list) are **documented unproduced seams** kept in
/// the ported status enum for the wire and the response builder's schema coverage,
/// mirroring how [`v201_get_log_decision`] documents its unproduced `Rejected` and
/// [`v201_delete_certificate_target`] its `Failed` race arm.
///
/// This is the *pure* decision, depending only on the request and the in-flight
/// snapshot — no runtime handles, no store lock — so it is unit-testable in
/// isolation. Recording the accepted request as the new in-flight update (the side
/// effect) is the wiring layer's job. Trust boundary: `firmware.location` and
/// `firmware.signature` are never read here, `firmware.signing_certificate` is only
/// inspected for PEM shape (never parsed or unwrapped), and `request_id` is only
/// compared, never parsed or indexed, so no wire value (including `i32::MIN`/`MAX`)
/// can panic. Ports `ocpp.v201.call.UpdateFirmware` →
/// `ocpp.v201.call_result.UpdateFirmware`.
#[must_use]
pub fn v201_update_firmware_decision(
    request: &UpdateFirmwareRequest,
    in_flight: Option<i32>,
) -> UpdateFirmwareStatusEnumType {
    // Axis 1: a present-but-unusable signing certificate refuses the whole
    // request up front, before any in-flight bookkeeping. A well-formed PEM
    // certificate reuses the `InstallCertificate` "no X.509 parse" boundary — it
    // is usable iff `v201_install_certificate_status` accepts it; an absent
    // certificate skips this axis.
    if let Some(certificate) = &request.firmware.signing_certificate {
        if v201_install_certificate_status(certificate)
            != InstallCertificateStatusEnumType::Accepted
        {
            return UpdateFirmwareStatusEnumType::InvalidCertificate;
        }
    }

    // Axis 2: single-in-flight supersede model, identical to `v201_get_log_decision`.
    match in_flight {
        None => UpdateFirmwareStatusEnumType::Accepted,
        Some(current) if current == request.request_id => UpdateFirmwareStatusEnumType::Accepted,
        Some(_) => UpdateFirmwareStatusEnumType::AcceptedCanceled,
    }
}

/// Build a schema-valid `UpdateFirmware.conf` ([`UpdateFirmwareResponse`]).
///
/// Pure constructor mirroring [`v201_get_log_response`]: carries the decided
/// [`status`](UpdateFirmwareStatusEnumType) plus the optional 2.0.1 `statusInfo` —
/// a vendor-agnostic `reasonCode` and human-readable detail the handler attaches to
/// a non-accept outcome (e.g. why the update was `InvalidCertificate`).
///
/// Ports `ocpp.v201.call_result.UpdateFirmware`.
#[must_use]
pub fn v201_update_firmware_response(
    status: UpdateFirmwareStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> UpdateFirmwareResponse {
    UpdateFirmwareResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// The first async status a station reports once it *begins* uploading the log
/// an `Accepted` `GetLog` asked for — [`Uploading`](UploadLogStatusEnumType::Uploading).
///
/// The `GetLog` CALLRESULT only acks; the station then streams upload progress
/// as `LogStatusNotification.req`, opening with this status and closing with a
/// terminal one from [`v201_log_upload_terminal_status`]. Named so the flow
/// reads as `IN_PROGRESS → terminal` at both the handler and the tests.
pub const V201_LOG_UPLOAD_IN_PROGRESS: UploadLogStatusEnumType = UploadLogStatusEnumType::Uploading;

/// Decide the terminal [`UploadLogStatusEnumType`] a simulated `GetLog` upload
/// settles on, closing the async `LogStatusNotification.req` stream it opened
/// with [`V201_LOG_UPLOAD_IN_PROGRESS`].
///
/// The simulator has no real archive to upload, so it models the transfer on a
/// short timer and reports one of three terminal outcomes, in precedence order:
///
/// - **`superseded`** (a newer `GetLog` took the station's single upload slot
///   while this one was in flight) → [`AcceptedCanceled`](UploadLogStatusEnumType::AcceptedCanceled):
///   the transfer was canceled to serve the newer request, so it reports the
///   cancel rather than a completion — and a canceled upload never reports a
///   `UploadFailure`, so this arm wins even under fault injection;
/// - **`should_fail`** (opt-in fault injection,
///   [`ChargePointConfig::log_upload_should_fail`](crate::ChargePointConfig::log_upload_should_fail))
///   → [`UploadFailure`](UploadLogStatusEnumType::UploadFailure): the transfer
///   ran as the owner but failed, so a CSMS can be exercised against a log
///   upload that fails, not just one that succeeds;
/// - otherwise → [`Uploaded`](UploadLogStatusEnumType::Uploaded): the happy path,
///   the transfer completed as the owner.
///
/// `superseded` is what the compare-and-clear completion seam
/// ([`V201LogUploadStore::complete`](crate::v201_log_upload::V201LogUploadStore::complete))
/// reports as `!still_owner`. The remaining `UploadLogStatusEnumType` values
/// (`Idle`, `BadMessage`, `NotSupportedOperation`, `PermissionDenied`) are
/// documented modeled seams this simulator does not drive from a `GetLog`: `Idle`
/// is the resting state (never a transition this stream reports), and the three
/// rejections belong to a station that refuses the request outright — which this
/// simulator, superseding rather than refusing, never does. They stay in the
/// ported enum for the wire and schema coverage.
#[must_use]
pub fn v201_log_upload_terminal_status(
    superseded: bool,
    should_fail: bool,
) -> UploadLogStatusEnumType {
    if superseded {
        UploadLogStatusEnumType::AcceptedCanceled
    } else if should_fail {
        UploadLogStatusEnumType::UploadFailure
    } else {
        UploadLogStatusEnumType::Uploaded
    }
}

/// Build a schema-valid `LogStatusNotification.req`
/// ([`LogStatusNotificationRequest`]) reporting `status` for the upload started
/// by the `GetLog` carrying `request_id`.
///
/// The `requestId` is always carried here (it correlates the async progress
/// report back to the triggering `GetLogRequest`); it is only absent when a
/// `TriggerMessage` asks for a `LogStatusNotification` with no upload ongoing,
/// which this `GetLog`-driven flow never is. Pure constructor mirroring
/// [`v201_get_log_response`]. Ports `ocpp.v201.call.LogStatusNotification`.
#[must_use]
pub fn v201_log_status_notification(
    status: UploadLogStatusEnumType,
    request_id: i32,
) -> LogStatusNotificationRequest {
    LogStatusNotificationRequest {
        status,
        request_id: Some(request_id),
        custom_data: None,
    }
}

/// Build a schema-valid `FirmwareStatusNotification.req`
/// ([`FirmwareStatusNotificationRequest`]) reporting `status` for the rollout
/// started by the `UpdateFirmware` carrying `request_id` (OCPP 2.0.1 Part 2,
/// firmware management, Issue #534).
///
/// The `requestId` is always carried here (it correlates the async progress
/// report back to the triggering `UpdateFirmwareRequest`); it is only absent
/// when a `TriggerMessage` asks for a `FirmwareStatusNotification` with no update
/// ongoing, which this `UpdateFirmware`-driven flow never is. The firmware twin
/// of [`v201_log_status_notification`]. Ports
/// `ocpp.v201.call.FirmwareStatusNotification`.
#[must_use]
pub fn v201_firmware_status_notification(
    status: FirmwareStatusEnumType,
    request_id: i32,
) -> FirmwareStatusNotificationRequest {
    FirmwareStatusNotificationRequest {
        status,
        request_id: Some(request_id),
        custom_data: None,
    }
}

/// The URIs a simulated firmware *publish* reports the cached image is available
/// from, carried on the terminal `Published`
/// [`PublishFirmwareStatusNotification`](PublishFirmwareStatusNotificationRequest).
///
/// The Charging Station *simulator* caches no real image — there is nothing to
/// actually serve — so an accepted publish reports a small, fixed set of
/// LAN-style download URIs that stand in for the locations a production Local
/// Controller would advertise. Kept as a standalone `const` (rather than inlined
/// into the notification builder) so a test can assert the list directly, and so
/// the simulated locations are a single documented seam a future "real firmware
/// cache" slice would replace. Each entry is well under the schema's per-URI
/// `maxLength: 512`.
pub const V201_SIMULATED_PUBLISH_FIRMWARE_LOCATIONS: &[&str] = &[
    "http://local-controller.lan/firmware/published.bin",
    "ftp://local-controller.lan/firmware/published.bin",
];

/// Build a `PublishFirmwareStatusNotification.req`
/// ([`PublishFirmwareStatusNotificationRequest`]) reporting `status` for the
/// firmware publish identified by `request_id` — the publish-to-local-cache twin
/// of [`v201_firmware_status_notification`].
///
/// Ports the progress carrier of
/// [`ocpp.v201.call.PublishFirmwareStatusNotification`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py):
/// the single [`PublishFirmwareStatusEnumType`], the correlating `requestId`
/// (always present here — a simulator only emits these while driving a publish it
/// accepted, never off a bare `TriggerMessage`), and the optional `location` URI
/// list. Per the spec `location` is required only on the terminal
/// [`Published`](PublishFirmwareStatusEnumType::Published) state and absent on the
/// intermediate lifecycle states; the caller passes `Some(list)` only there, so
/// this builder simply forwards whatever it is given.
///
/// `request_id` is copied into the message and never parsed or indexed, so an
/// extreme value (`i32::MIN`/`MAX`) cannot panic; `location` is simulator-supplied
/// (see [`V201_SIMULATED_PUBLISH_FIRMWARE_LOCATIONS`]), never attacker input.
#[must_use]
pub fn v201_publish_firmware_status_notification(
    status: PublishFirmwareStatusEnumType,
    location: Option<Vec<String>>,
    request_id: i32,
) -> PublishFirmwareStatusNotificationRequest {
    PublishFirmwareStatusNotificationRequest {
        status,
        location,
        request_id: Some(request_id),
        custom_data: None,
    }
}

/// A small stable 64-bit digest of `input`, rendered as 16 lowercase hex chars.
///
/// FNV-1a — deterministic across runs and platforms (unlike the standard
/// library's `DefaultHasher`, whose output is unspecified), which is the whole
/// point: it lets the certificate-hash placeholders round-trip
/// `GetInstalledCertificateIds` → `DeleteCertificate` within a run. It is a
/// content digest for *identification*, **not** a cryptographic hash — the
/// simulator does no X.509 parse (the "no PKI" boundary Issue #518 set), so this
/// stands in for a genuine certificate-hash derivation. 16 hex chars fits every
/// `CertificateHashDataType` field bound (`serialNumber` ≤ 40, the two issuer
/// hashes ≤ 128).
fn stable_hash_hex(input: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for byte in input.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

/// Derive the placeholder [`CertificateHashDataType`] that identifies the anchor
/// installed under `use_` with PEM `pem`.
///
/// The **shared hash seam** of the certificate-management read/remove pair: the
/// hash `GetInstalledCertificateIds` reports for an anchor is exactly the hash a
/// later `DeleteCertificate` names to remove that same anchor, so this one
/// function is the single source of truth both go through (Issue #522 reuses it
/// to resolve a requested hash back to its `use_`). Because the derivation is a
/// pure, deterministic function of `(use_, pem)`, a hash returned by an enumerate
/// round-trips to a delete of the same anchor for as long as that anchor holds
/// that PEM; rotating the anchor (a re-install under the same use with different
/// PEM) changes its hash, exactly as a real certificate hash would.
///
/// The simulator does **no** X.509 parse, so the three hash fields are stable
/// digests of salted `(use_, pem)` inputs rather than genuine issuer-name /
/// issuer-key / serial hashes; [`hash_algorithm`](CertificateHashDataType::hash_algorithm)
/// is reported as `SHA256` as a placeholder. This is a documented boundary — a
/// real derivation is a natural follow-up behind an X.509 feature.
#[must_use]
pub fn v201_certificate_hash_data(
    use_: InstallCertificateUseEnumType,
    pem: &str,
) -> CertificateHashDataType {
    let use_tag = format!("{use_:?}");
    CertificateHashDataType {
        hash_algorithm: HashAlgorithmEnumType::Sha256,
        issuer_name_hash: stable_hash_hex(&format!("issuer-name|{use_tag}|{pem}")),
        issuer_key_hash: stable_hash_hex(&format!("issuer-key|{use_tag}|{pem}")),
        serial_number: stable_hash_hex(&format!("serial|{use_tag}|{pem}")),
        custom_data: None,
    }
}

/// Decide whether the station accepts or refuses the signed certificate chain a
/// `CertificateSigned` delivered (OCPP 2.0.1 Part 2, A02).
///
/// Ports the accept/reject decision behind
/// [`ocpp.v201.call_result.CertificateSigned`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call_result.py)'s
/// [`CertificateSignedStatusEnumType`]. `CertificateSigned` is the delivery
/// terminus of the certificate-*provisioning* flow: after the station submits a
/// CSR via `SignCertificate`, the CSMS pushes the CA-signed chain here and the
/// station installs it and answers `Accepted`, or refuses a malformed / unusable
/// chain with `Rejected`.
///
/// The simulator models the *protocol* decision, not a PKI, so this is a
/// deliberately lightweight predicate on the PEM string — **no X.509 parse, no
/// signature/chain verification** (the same documented "no PKI" boundary
/// [`v201_install_certificate_status`] sets; a real validation seam is a natural
/// follow-up). `certificate_chain` is untrusted CSMS input, so it is treated as
/// an opaque, bounded string and is only ever inspected, never parsed or
/// unwrapped — no wire value (empty, garbage, very long, control chars) can
/// panic.
///
/// Unlike `InstallCertificate`'s three-value enum, `CertificateSignedStatusEnumType`
/// is binary (`Accepted` / `Rejected`), so the "recognized but unusable" arm
/// `InstallCertificate` reports as `Failed` collapses into `Rejected` here:
///
/// - **`Rejected`** — the chain is empty / whitespace-only, is not PEM-armored at
///   all (missing the `-----BEGIN … -----` / `-----END … -----` markers), or is
///   PEM-armored yet carries no key material between the markers. There is
///   nothing usable to install.
/// - **`Accepted`** — a PEM-armored chain with a non-empty body; the station
///   installs it.
#[must_use]
pub fn v201_certificate_signed_status(certificate_chain: &str) -> CertificateSignedStatusEnumType {
    let trimmed = certificate_chain.trim();

    // Nothing to install, or not a PEM chain at all → refuse.
    if trimmed.is_empty() || !trimmed.contains("-----BEGIN") || !trimmed.contains("-----END") {
        return CertificateSignedStatusEnumType::Rejected;
    }

    // PEM-armored, but is there any body between the markers? The armor lines
    // (`-----BEGIN … -----`, `-----END … -----`) are stripped; whatever remains,
    // minus whitespace, is the base64 body.
    let has_body = trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("-----"))
        .any(|line| !line.is_empty());

    if has_body {
        CertificateSignedStatusEnumType::Accepted
    } else {
        CertificateSignedStatusEnumType::Rejected
    }
}

/// Build a schema-valid `CertificateSigned.conf` ([`CertificateSignedResponse`]).
///
/// Pure constructor mirroring [`v201_install_certificate_response`]: carries the
/// decided [`status`](CertificateSignedStatusEnumType) plus the optional 2.0.1
/// `statusInfo` — a vendor-agnostic `reasonCode` and human-readable detail the
/// handler attaches to a `Rejected` outcome (why the chain was refused). An
/// `Accepted` carries no `statusInfo`.
///
/// Ports `ocpp.v201.call_result.CertificateSigned`.
#[must_use]
pub fn v201_certificate_signed_response(
    status: CertificateSignedStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> CertificateSignedResponse {
    CertificateSignedResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// A well-shaped, **opaque** placeholder Certificate Signing Request the
/// simulator submits in a `SignCertificate.req`.
///
/// The simulator does no crypto (the same "no PKI" boundary
/// [`v201_certificate_signed_status`] / [`v201_install_certificate_status`] set):
/// there is no key pair and no real RFC 2986 CSR to encode. This returns a
/// PEM-armored, base64-body blob shaped like a `CERTIFICATE REQUEST` so it is
/// well-formed on the wire and satisfies the `SignCertificate` schema (a plain
/// `csr: string`, `maxLength` 5500) — it is **not** a cryptographically valid
/// CSR and a real CSMS CA would reject it. It exists purely for wire/schema
/// coverage of the CP-initiated provisioning entry point; substituting real key
/// material is a follow-up gated on the simulator growing a crypto provider.
///
/// The body stays comfortably under the schema's 5500-char cap.
#[must_use]
pub fn v201_placeholder_csr() -> String {
    // A fixed, deterministic base64-ish body between PEM `CERTIFICATE REQUEST`
    // armor. Deterministic so tests can pin it and so successive requests are
    // byte-identical (the simulator has no per-request key material to vary).
    concat!(
        "-----BEGIN CERTIFICATE REQUEST-----\n",
        "MIIBVDCB+wIBADAVMRMwEQYDVQQDDApvY3BwLXJzLXNpbTBZMBMGByqGSM49AgEG\n",
        "CCqGSM49AwEHA0IABE9vY3BwLXJzIHNpbXVsYXRvciBwbGFjZWhvbGRlciBDU1Ig\n",
        "bm90IHJlYWwga2V5IG1hdGVyaWFsIG9wYXF1ZSBibG9ioAAwCgYIKoZIzj0EAwID\n",
        "SQAwRgIhAJvc3BwLXJzLXNpbXVsYXRvci1wbGFjZWhvbGRlci1jc3ItYWIxYWIx\n",
        "YWIxYWIxYWIxYWIxYWIxYWIxYWIxYWIxYWIx\n",
        "-----END CERTIFICATE REQUEST-----"
    )
    .to_string()
}

/// Build a schema-valid `SignCertificate.req` ([`SignCertificateRequest`]) — the
/// **CP-initiated entry point** of the OCPP 2.0.1 certificate-provisioning flow.
///
/// Ports [`ocpp.v201.call.SignCertificate`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py).
/// In 2.0.1 the station originates provisioning: it sends this request carrying a
/// PEM-encoded CSR, the CSMS acknowledges synchronously with a
/// [`GenericStatusEnumType`]
/// (`Accepted` / `Rejected`), and the operator's CA later returns the signed
/// chain out-of-band via the paired `CertificateSigned` CALL (the delivery
/// terminus this module already answers, [`v201_certificate_signed_status`]).
///
/// `certificate_type` selects which certificate the CSR is for
/// ([`ChargingStationCertificate`](CertificateSigningUseEnumType::ChargingStationCertificate)
/// or [`V2GCertificate`](CertificateSigningUseEnumType::V2GCertificate)); when
/// `None` the request omits the field, which the spec reads as *both* the ISO
/// 15118 connection and the Charging-Station-to-CSMS connection. The `csr` is the
/// opaque simulator placeholder ([`v201_placeholder_csr`]) — no real key
/// material; this slice provisions the wire/schema path, not a PKI.
///
/// Pure over its input, so it is unit- and schema-testable without a runtime or a
/// socket; emitting it as a CALL and surfacing the response is the wiring layer's
/// job ([`ChargePoint::request_sign_certificate`](crate::ChargePoint::request_sign_certificate)).
#[must_use]
pub fn v201_sign_certificate_request(
    certificate_type: Option<CertificateSigningUseEnumType>,
) -> SignCertificateRequest {
    SignCertificateRequest {
        csr: v201_placeholder_csr(),
        certificate_type,
        custom_data: None,
    }
}

/// Build a schema-valid `Get15118EVCertificate.req`
/// ([`Get15118EVCertificateRequest`]) — the **CP-initiated** relay leg of the ISO
/// 15118 Plug-and-Charge certificate exchange (Part 2, A01/A02, Issue #558).
///
/// Ports [`ocpp.v201.call.Get15118EVCertificate`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py).
/// During a 15118 session the EV emits a raw EXI `CertificateInstallationReq`
/// that the Charging Station cannot interpret; it forwards the opaque blob up to
/// the CSMS carrying the `iso15118SchemaVersion` the session negotiated and a
/// [`CertificateActionEnumType`] (`Install` / `Update`). The CSMS relays it to the
/// contract-certificate backend and answers with an
/// [`Iso15118EVCertificateStatusEnumType`](ocpp_types::v201::Iso15118EVCertificateStatusEnumType)
/// plus the EXI `CertificateInstallationRes` the station passes back to the EV.
///
/// All three request fields are required and threaded through **verbatim** — the
/// builder adds no policy. `exi_request` is the EV's base64-EXI request, relayed
/// opaquely: the station is a transparent relay and never decodes it (the same
/// "no PKI / no EXI codec" boundary the certificate decision predicates set). The
/// schema caps `iso15118SchemaVersion` at 50 chars and `exiRequest` at 5600; the
/// caller supplies values within those bounds (the simulator uses a small, fixed
/// placeholder EXI blob in its tests).
///
/// Pure over its input, so it is unit- and schema-testable without a runtime or a
/// socket; emitting it as a CALL and surfacing the full response is the wiring
/// layer's job
/// ([`ChargePoint::request_get_15118_ev_certificate`](crate::ChargePoint::request_get_15118_ev_certificate)).
#[must_use]
pub fn v201_get_15118_ev_certificate_request(
    iso15118_schema_version: &str,
    action: CertificateActionEnumType,
    exi_request: &str,
) -> Get15118EVCertificateRequest {
    Get15118EVCertificateRequest {
        iso15118_schema_version: iso15118_schema_version.to_string(),
        action,
        exi_request: exi_request.to_string(),
        custom_data: None,
    }
}

/// Map an [`InstallCertificateUseEnumType`] (what the store keys by) to the
/// [`GetCertificateIdUseEnumType`] (`GetInstalledCertificateIds` reports and
/// filters by).
///
/// `GetCertificateIdUseEnumType` is a superset of `InstallCertificateUseEnumType`
/// — it adds `V2GCertificateChain`, which names the station's installed V2G *leaf*
/// chain rather than a trust-anchor root. The store only ever holds the four
/// roots, so this mapping is total and never produces `V2GCertificateChain`; a
/// filter that names it simply matches nothing installed.
fn get_use_of_install_use(use_: InstallCertificateUseEnumType) -> GetCertificateIdUseEnumType {
    match use_ {
        InstallCertificateUseEnumType::V2GRootCertificate => {
            GetCertificateIdUseEnumType::V2GRootCertificate
        }
        InstallCertificateUseEnumType::MORootCertificate => {
            GetCertificateIdUseEnumType::MORootCertificate
        }
        InstallCertificateUseEnumType::CSMSRootCertificate => {
            GetCertificateIdUseEnumType::CSMSRootCertificate
        }
        InstallCertificateUseEnumType::ManufacturerRootCertificate => {
            GetCertificateIdUseEnumType::ManufacturerRootCertificate
        }
    }
}

/// A stable ordering index over [`GetCertificateIdUseEnumType`], used only to make
/// the reported chain deterministic (the store snapshot is an unordered `HashMap`
/// walk). The exact order is arbitrary but fixed.
fn get_use_order(use_: GetCertificateIdUseEnumType) -> u8 {
    match use_ {
        GetCertificateIdUseEnumType::V2GRootCertificate => 0,
        GetCertificateIdUseEnumType::MORootCertificate => 1,
        GetCertificateIdUseEnumType::CSMSRootCertificate => 2,
        GetCertificateIdUseEnumType::V2GCertificateChain => 3,
        GetCertificateIdUseEnumType::ManufacturerRootCertificate => 4,
    }
}

/// Resolve which installed trust anchors a `GetInstalledCertificateIds` query
/// enumerates, given its optional `certificate_type` filter and an owned
/// `snapshot()` of the [`V201CertificateStore`](crate::v201_certificate_store::V201CertificateStore).
///
/// The **read** half of the certificate-management family (ports
/// [`ocpp.v201.call.GetInstalledCertificateIds`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py)),
/// mirroring [`v201_get_display_messages_matches`]. The filter is a set of
/// [`GetCertificateIdUseEnumType`] categories:
///
/// - **absent (`None`)** — a wildcard: every installed anchor matches.
/// - **present** — an anchor matches when its use is one of the listed categories.
///   The schema guarantees a present list is non-empty (`minItems` 1). Duplicate
///   entries are harmless (membership, not iteration), and a category the store
///   cannot hold — notably `V2GCertificateChain`, or any root not installed —
///   simply matches nothing.
///
/// Each matched anchor becomes one [`CertificateHashDataChainType`] carrying its
/// [`certificate_type`](CertificateHashDataChainType::certificate_type) and the
/// placeholder [`v201_certificate_hash_data`] for `(use, PEM)`; no child chain is
/// synthesized (the simulator holds only the root PEM). The result is sorted by a
/// fixed use order so the reported chain is deterministic regardless of the
/// snapshot's `HashMap` walk order.
///
/// Pure over its inputs (the filter plus an owned snapshot), so it is
/// unit-testable without a runtime or the store lock; taking the snapshot and
/// answering is the wiring layer's job.
#[must_use]
pub fn v201_get_installed_certificate_ids_matches(
    filter: Option<&[GetCertificateIdUseEnumType]>,
    snapshot: &[(InstallCertificateUseEnumType, String)],
) -> Vec<CertificateHashDataChainType> {
    let mut chain: Vec<CertificateHashDataChainType> = snapshot
        .iter()
        .filter_map(|(use_, pem)| {
            let get_use = get_use_of_install_use(*use_);
            filter
                .is_none_or(|types| types.contains(&get_use))
                .then(|| CertificateHashDataChainType {
                    certificate_type: get_use,
                    certificate_hash_data: v201_certificate_hash_data(*use_, pem),
                    child_certificate_hash_data: None,
                    custom_data: None,
                })
        })
        .collect();
    chain.sort_by_key(|entry| get_use_order(entry.certificate_type));
    chain
}

/// Build a schema-valid `GetInstalledCertificateIds.conf`
/// ([`GetInstalledCertificateIdsResponse`]) from the matched `chain`.
///
/// Pure constructor: the station reports
/// [`Accepted`](GetInstalledCertificateStatusEnumType::Accepted) with the matched
/// hash chain when at least one installed anchor matched the query, or
/// [`NotFound`](GetInstalledCertificateStatusEnumType::NotFound) with no chain
/// when nothing matched (an empty store, or a filter naming nothing installed) —
/// exactly the two-value contract `ocpp.v201.enums.GetInstalledCertificateStatusEnumType`
/// defines. The `certificate_hash_data_chain` is left absent rather than an empty
/// array on `NotFound`, satisfying the schema's `minItems: 1`-when-present rule.
/// Ports `ocpp.v201.call_result.GetInstalledCertificateIds`.
#[must_use]
pub fn v201_get_installed_certificate_ids_response(
    chain: Vec<CertificateHashDataChainType>,
) -> GetInstalledCertificateIdsResponse {
    if chain.is_empty() {
        GetInstalledCertificateIdsResponse {
            status: GetInstalledCertificateStatusEnumType::NotFound,
            status_info: None,
            certificate_hash_data_chain: None,
            custom_data: None,
        }
    } else {
        GetInstalledCertificateIdsResponse {
            status: GetInstalledCertificateStatusEnumType::Accepted,
            status_info: None,
            certificate_hash_data_chain: Some(chain),
            custom_data: None,
        }
    }
}

/// Whether two [`CertificateHashDataType`] name the **same** certificate.
///
/// Identity is the OCPP hash triple plus the algorithm that produced it —
/// [`hash_algorithm`](CertificateHashDataType::hash_algorithm),
/// [`issuer_name_hash`](CertificateHashDataType::issuer_name_hash),
/// [`issuer_key_hash`](CertificateHashDataType::issuer_key_hash), and
/// [`serial_number`](CertificateHashDataType::serial_number). The `customData`
/// vendor extension is deliberately excluded: it is an optional annotation, not
/// part of what the hash *identifies*, so a `DeleteCertificate` that carries a
/// `customData` still matches the anchor it names (and derived hashes, which
/// carry none, match a request that does). This is a field-wise compare rather
/// than the derived `PartialEq` precisely so that extension cannot change the
/// match.
#[must_use]
pub fn v201_certificate_hash_matches(
    a: &CertificateHashDataType,
    b: &CertificateHashDataType,
) -> bool {
    a.hash_algorithm == b.hash_algorithm
        && a.issuer_name_hash == b.issuer_name_hash
        && a.issuer_key_hash == b.issuer_key_hash
        && a.serial_number == b.serial_number
}

/// Resolve which installed trust anchor (if any) a `DeleteCertificate` request's
/// `certificate_hash_data` names, over an owned `snapshot()` of the
/// [`V201CertificateStore`](crate::v201_certificate_store::V201CertificateStore).
///
/// The **remove** half of the certificate-management family (ports
/// [`ocpp.v201.call.DeleteCertificate`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py)),
/// the counterpart of [`v201_get_installed_certificate_ids_matches`]. It reuses
/// the **same** [`v201_certificate_hash_data`] seam the enumerate side reports
/// through: each stored `(use, PEM)` is hashed to the placeholder
/// [`CertificateHashDataType`] `GetInstalledCertificateIds` would return for it,
/// and the requested hash is matched against those by
/// [`v201_certificate_hash_matches`]. So a hash a CSMS learned from
/// `GetInstalledCertificateIds` round-trips to a delete of exactly that anchor,
/// for as long as it still holds that PEM (a rotation changes the anchor's hash,
/// exactly as a real certificate hash would, and a stale hash then resolves to
/// nothing).
///
/// Returns the [`InstallCertificateUseEnumType`] of the first matching anchor, or
/// `None` when nothing installed matches — the wiring layer maps `None` to
/// `NotFound` and drives the removal itself, since whether a matched anchor can
/// actually be removed (`Accepted` vs `Failed`) depends on the live store, not on
/// this pure snapshot. At most one anchor is held per use and each use derives a
/// distinct hash, so at most one entry can match; `find_map` short-circuits on it.
///
/// Pure over its inputs (the requested hash plus an owned snapshot), so it is
/// unit-testable without a runtime or the store lock. The requested hash is
/// untrusted CSMS input, but every field is only ever string-compared here —
/// never parsed or unwrapped — so no hostile value (arbitrary bytes, over-long
/// strings) can panic.
#[must_use]
pub fn v201_delete_certificate_target(
    requested: &CertificateHashDataType,
    snapshot: &[(InstallCertificateUseEnumType, String)],
) -> Option<InstallCertificateUseEnumType> {
    snapshot.iter().find_map(|(use_, pem)| {
        v201_certificate_hash_matches(requested, &v201_certificate_hash_data(*use_, pem))
            .then_some(*use_)
    })
}

/// Build a schema-valid `DeleteCertificate.conf` ([`DeleteCertificateResponse`]).
///
/// Pure constructor mirroring [`v201_install_certificate_response`]: carries the
/// decided [`status`](DeleteCertificateStatusEnumType) plus the optional 2.0.1
/// `statusInfo` — a vendor-agnostic `reasonCode` and human-readable detail the
/// handler attaches to a non-`Accepted` outcome (why nothing was removed:
/// `NotFound` with no matching anchor, or `Failed` when a matched anchor could
/// not be removed). An `Accepted` carries no `statusInfo`.
///
/// Ports `ocpp.v201.call_result.DeleteCertificate`.
#[must_use]
pub fn v201_delete_certificate_response(
    status: DeleteCertificateStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> DeleteCertificateResponse {
    DeleteCertificateResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Decide how a `for_version(V201)` station answers a `CustomerInformation.req`
/// (OCPP 2.0.1 Part 2, N09/N10 — the privacy / GDPR "report and/or clear stored
/// customer data" command), given the inbound request.
///
/// The request identifies a customer by up to three optional selectors — a hashed
/// customer certificate ([`customer_certificate`](CustomerInformationRequest::customer_certificate)),
/// an authorization token ([`id_token`](CustomerInformationRequest::id_token)), or
/// a free-form [`customer_identifier`](CustomerInformationRequest::customer_identifier))
/// — and asks the station to [`report`](CustomerInformationRequest::report) the
/// stored data, [`clear`](CustomerInformationRequest::clear) it, or both.
///
/// Faithful to the reference contract:
///
/// - **Actionable** — the request names **at least one** selector *and* requests
///   **at least one** action (`report` or `clear`) →
///   [`Accepted`](CustomerInformationStatusEnumType::Accepted). The station
///   acknowledges the command; any requested report data then arrives
///   asynchronously via `NotifyCustomerInformation` (a separate follow-up).
/// - **Malformed** — the request names **no** selector, *or* requests **neither**
///   `report` nor `clear` → [`Invalid`](CustomerInformationStatusEnumType::Invalid):
///   there is no customer to act on / nothing to do. This is the input-reachable
///   status, mirroring how the reference treats a request it cannot act on, and the
///   same "malformed request" arm the [`v201_get_log_decision`] /
///   [`v201_set_display_message_status`] siblings model.
///
/// [`Rejected`](CustomerInformationStatusEnumType::Rejected) is kept as a
/// documented **unproduced** seam for wire + schema coverage: the simulator does
/// not model an authorization-refusal policy (it holds no real customer-data
/// store), so it never refuses an otherwise-actionable request. A real station
/// enforcing an access policy maps to it. This follows the unproduced-status
/// convention `GetLog` (`Rejected`) and `DeleteCertificate` (`Failed`, a race arm)
/// already use.
///
/// This is the *pure* decision — no runtime handles, no lock — so it is
/// unit-testable in isolation. The three selectors are attacker-influenced CSMS
/// input, inspected only for **presence** ([`Option::is_some`]) and never
/// unwrapped, parsed, or indexed; `report` / `clear` are booleans and `request_id`
/// is not read here at all, so no wire value (including `i32::MIN`/`MAX`) can
/// panic. Over-length selector fields are refused at the schema layer (→
/// CALLERROR) before this runs. Ports `ocpp.v201.call.CustomerInformation` →
/// `ocpp.v201.call_result.CustomerInformation`.
#[must_use]
pub fn v201_customer_information_decision(
    request: &CustomerInformationRequest,
) -> CustomerInformationStatusEnumType {
    let names_a_customer = request.customer_certificate.is_some()
        || request.id_token.is_some()
        || request.customer_identifier.is_some();
    let requests_an_action = request.report || request.clear;

    if names_a_customer && requests_an_action {
        CustomerInformationStatusEnumType::Accepted
    } else {
        CustomerInformationStatusEnumType::Invalid
    }
}

/// Build a schema-valid `CustomerInformation.conf`
/// ([`CustomerInformationResponse`]).
///
/// Pure constructor mirroring [`v201_certificate_signed_response`]: carries the
/// decided [`status`](CustomerInformationStatusEnumType) plus the optional 2.0.1
/// `statusInfo` — a vendor-agnostic `reasonCode` and human-readable detail the
/// handler attaches to an `Invalid` outcome (why the request could not be acted
/// on). An `Accepted` carries no `statusInfo`.
///
/// Ports `ocpp.v201.call_result.CustomerInformation`.
#[must_use]
pub fn v201_customer_information_response(
    status: CustomerInformationStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> CustomerInformationResponse {
    CustomerInformationResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Decide how a `for_version(V201)` station answers a `PublishFirmware.req`
/// (OCPP 2.0.1 Part 2, the Local-Controller firmware-cache trigger).
///
/// `PublishFirmware` tells a station acting as a Local Controller to download a
/// firmware image from [`location`](PublishFirmwareRequest::location) once and
/// cache it locally, so the chargers behind it can pull it over the LAN instead
/// of each fetching it from the CSMS over the WAN. The image is identified by a
/// 32-char MD5 [`checksum`](PublishFirmwareRequest::checksum) and correlated by
/// [`request_id`](PublishFirmwareRequest::request_id); the actual download
/// progress is reported asynchronously via `PublishFirmwareStatusNotification`
/// (a separate follow-up). This is the **synchronous accept/reject** decision.
///
/// The simulator models the *protocol* decision, not a firmware downloader, so
/// this is a lightweight shape predicate — **no URL is opened, followed, or
/// parsed**, and the checksum is never used to verify a real image:
///
/// - [`Accepted`](GenericStatusEnumType::Accepted) — the request carries a
///   non-empty `location` **and** a well-shaped `checksum` (exactly 32
///   hexadecimal characters, an MD5 digest). The Local Controller would begin
///   the cached download.
/// - [`Rejected`](GenericStatusEnumType::Rejected) — the `location` is empty /
///   whitespace-only, or the `checksum` is not a 32-char hex string; there is
///   nothing actionable to download or verify against.
///
/// [`retries`](PublishFirmwareRequest::retries) /
/// [`retry_interval`](PublishFirmwareRequest::retry_interval) are advisory
/// download tuning, not policy inputs, and do not affect the decision.
///
/// Trust boundary: `location` and `checksum` are attacker-influenced CSMS input,
/// inspected only for shape and **never** opened, followed, parsed, or indexed,
/// so no wire value (empty, garbage, very long, control chars) can panic.
/// `request_id` is not read here, so extreme values (`i32::MIN`/`MAX`) are safe.
/// Over-length fields (`location` maxLength 512, `checksum` maxLength 32) are
/// refused at the schema layer (→ CALLERROR) before this runs. Ports the
/// accept/reject decision behind
/// [`ocpp.v201.call_result.PublishFirmware`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call_result.py)'s
/// [`GenericStatusEnumType`].
#[must_use]
pub fn v201_publish_firmware_decision(request: &PublishFirmwareRequest) -> GenericStatusEnumType {
    let has_location = !request.location.trim().is_empty();
    // An MD5 digest rendered as hex: exactly 32 ASCII hex characters. The schema
    // caps the length (maxLength 32) but does not enforce the count or hex-ness,
    // so the handler checks the shape it can actually act on.
    let checksum = &request.checksum;
    let well_shaped_checksum =
        checksum.len() == 32 && checksum.bytes().all(|b| b.is_ascii_hexdigit());

    if has_location && well_shaped_checksum {
        GenericStatusEnumType::Accepted
    } else {
        GenericStatusEnumType::Rejected
    }
}

/// Build a schema-valid `PublishFirmware.conf` ([`PublishFirmwareResponse`]).
///
/// Pure constructor mirroring [`v201_customer_information_response`]: carries the
/// decided [`status`](GenericStatusEnumType) plus the optional 2.0.1
/// `statusInfo` — a vendor-agnostic `reasonCode` and human-readable detail the
/// handler attaches to a `Rejected` outcome (why the publish request was
/// refused). An `Accepted` carries no `statusInfo`.
///
/// Ports `ocpp.v201.call_result.PublishFirmware`.
#[must_use]
pub fn v201_publish_firmware_response(
    status: GenericStatusEnumType,
    status_info: Option<StatusInfoType>,
) -> PublishFirmwareResponse {
    PublishFirmwareResponse {
        status,
        status_info,
        custom_data: None,
    }
}

/// Decide how a `for_version(V201)` station answers an `UnpublishFirmware.req`
/// (OCPP 2.0.1 Part 2, the Local-Controller firmware-cache teardown).
///
/// `UnpublishFirmware` is the counterpart to
/// [`v201_publish_firmware_decision`]: where `PublishFirmware` tells a station
/// acting as a Local Controller to *download and cache* a firmware image,
/// `UnpublishFirmware` tells it to *remove* a previously-cached image,
/// identified by the same 32-char MD5 [`checksum`](UnpublishFirmwareRequest::checksum)
/// used when it was published. Unlike `PublishFirmware`, the answer is a single
/// **synchronous** terminal status — there is no asynchronous progress stream —
/// so this is a self-contained decide-and-answer with no queued follow-up.
///
/// The simulator models the *protocol* decision, not a firmware cache, so this
/// is a lightweight shape predicate — **no checksum is opened, followed,
/// parsed, or indexed** to look up a real image:
///
/// - [`Unpublished`](UnpublishFirmwareStatusEnumType::Unpublished) — the
///   `checksum` is a well-shaped MD5 digest (exactly 32 hexadecimal
///   characters). The Local Controller would drop the matching cached image.
/// - [`NoFirmware`](UnpublishFirmwareStatusEnumType::NoFirmware) — the
///   `checksum` is not a 32-char hex string, so it cannot name any image the
///   station could have cached (the stateless simulator holds no publish
///   store, so a mis-shaped checksum can never match).
/// - [`DownloadOngoing`](UnpublishFirmwareStatusEnumType::DownloadOngoing) is a
///   **documented unproduced seam** — it would be reported only when the
///   `checksum` names a publish still in flight (a `PublishFirmware` whose async
///   stream has not reached its terminal `Published`). The stateless simulator
///   models no publish-cache store, so it never emits this arm; it stays covered
///   on the wire and in the schema for when a real store lands (mirroring the
///   `GetLog` / `UpdateFirmware` convention of keeping every enum arm covered
///   even when the simulator never produces it).
///
/// Trust boundary: `checksum` is attacker-influenced CSMS input, inspected only
/// for shape and **never** opened, followed, parsed, or indexed, so no wire
/// value (empty, garbage, control chars, non-ASCII) can panic. Over-length
/// (`checksum` maxLength 32) is refused at the schema layer (→ CALLERROR) before
/// this runs. Ports the decision behind
/// [`ocpp.v201.call_result.UnpublishFirmware`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call_result.py)'s
/// [`UnpublishFirmwareStatusEnumType`].
#[must_use]
pub fn v201_unpublish_firmware_decision(
    request: &UnpublishFirmwareRequest,
) -> UnpublishFirmwareStatusEnumType {
    // An MD5 digest rendered as hex: exactly 32 ASCII hex characters. The schema
    // caps the length (maxLength 32) but enforces neither the exact count nor
    // hex-ness, so the handler checks the shape it can actually act on — the same
    // predicate the `PublishFirmware` decision uses on this field.
    let checksum = &request.checksum;
    let well_shaped_checksum =
        checksum.len() == 32 && checksum.bytes().all(|b| b.is_ascii_hexdigit());

    if well_shaped_checksum {
        UnpublishFirmwareStatusEnumType::Unpublished
    } else {
        UnpublishFirmwareStatusEnumType::NoFirmware
    }
}

/// Build a schema-valid `UnpublishFirmware.conf` ([`UnpublishFirmwareResponse`]).
///
/// Pure constructor mirroring [`v201_publish_firmware_response`]. Unlike its
/// publish sibling, the `UnpublishFirmware` response carries no `statusInfo`
/// field (the 2.0.1 schema defines only `status` + `customData`), so the single
/// terminal [`status`](UnpublishFirmwareStatusEnumType) is the whole answer.
///
/// Ports `ocpp.v201.call_result.UnpublishFirmware`.
#[must_use]
pub fn v201_unpublish_firmware_response(
    status: UnpublishFirmwareStatusEnumType,
) -> UnpublishFirmwareResponse {
    UnpublishFirmwareResponse {
        status,
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
    fn set_charging_profile_accepts_a_txdefaultprofile_regardless_of_transaction() {
        // A TxDefaultProfile is station configuration, not transaction-scoped, so
        // it is Accepted whether or not a session is live on the EVSE (Issue #471).
        for has_tx in [false, true] {
            let (status, info) = v201_set_charging_profile_status(
                ChargingProfilePurposeEnumType::TxDefaultProfile,
                has_tx,
            );
            assert_eq!(
                status,
                ChargingProfileStatusEnumType::Accepted,
                "a TxDefaultProfile is accepted (has_tx={has_tx})"
            );
            assert!(
                info.is_none(),
                "an accepted default carries no rejection detail (has_tx={has_tx})"
            );
        }
    }

    #[test]
    fn set_charging_profile_accepts_the_station_ceiling_purposes() {
        // The station-wide ceilings are station configuration, not
        // transaction-scoped, so they are Accepted regardless of whether a session
        // is live; the wiring layer installs them into the ceiling store and the
        // metering resolver caps by them (Issue #511).
        for purpose in [
            ChargingProfilePurposeEnumType::ChargingStationMaxProfile,
            ChargingProfilePurposeEnumType::ChargingStationExternalConstraints,
        ] {
            for has_tx in [false, true] {
                let (status, info) = v201_set_charging_profile_status(purpose, has_tx);
                assert_eq!(
                    status,
                    ChargingProfileStatusEnumType::Accepted,
                    "{purpose:?} is honored as a station ceiling (has_tx={has_tx})"
                );
                assert!(
                    info.is_none(),
                    "an accepted ceiling carries no rejection detail (has_tx={has_tx})"
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
            reason_code: "NoTransaction".to_string(),
            additional_info: Some(
                "a TxProfile requires an ongoing transaction on the targeted EVSE".to_string(),
            ),
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
            matched_evses(&v201_clear_charging_profile_matches(Some(20), None, &store)),
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
            matched_evses(&v201_clear_charging_profile_matches(
                Some(20),
                Some(&criteria),
                &store
            )),
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
            matched_evses(&v201_clear_charging_profile_matches(
                None,
                Some(&criteria),
                &store
            )),
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
            matched_evses(&v201_clear_charging_profile_matches(
                None,
                Some(&criteria),
                &store
            )),
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
            matched_evses(&v201_clear_charging_profile_matches(
                None,
                Some(&tx),
                &store
            )),
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
        // evseId 0 targets the station-wide entries the transaction-scoped store
        // never holds (its keys are real EVSEs ≥ 1); the whole-station default /
        // ceiling matches are covered by `clear_evse_id_zero_matches_station_wide_
        // default_and_ceiling` over the combined store below.
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
            matched_evses(&v201_clear_charging_profile_matches(None, None, &store)),
            vec![1, 2]
        );
        // An empty request against an empty store matches nothing (→ Unknown).
        assert!(v201_clear_charging_profile_matches(None, None, &[]).is_empty());
    }

    /// A combined three-store snapshot for the ClearChargingProfile selector: a
    /// `TxProfile` on EVSE 1 (id 10), a `TxDefaultProfile` on EVSE 1 (id 30) and
    /// the whole-station key 0 (id 31), a `ChargingStationMaxProfile` ceiling on
    /// key 0 (id 40), and a `ChargingStationExternalConstraints` ceiling on EVSE 2
    /// (id 50). Mirrors the shape the wiring layer builds by concatenating the
    /// three store snapshots (the ceiling store's `CeilingKind` dropped, recovered
    /// downstream from each profile's `chargingProfilePurpose`).
    fn combined_clear_store() -> Vec<(i32, ChargingProfileType)> {
        vec![
            (
                1,
                clear_test_profile(10, 0, ChargingProfilePurposeEnumType::TxProfile),
            ),
            (
                1,
                clear_test_profile(30, 0, ChargingProfilePurposeEnumType::TxDefaultProfile),
            ),
            (
                0,
                clear_test_profile(31, 0, ChargingProfilePurposeEnumType::TxDefaultProfile),
            ),
            (
                0,
                clear_test_profile(
                    40,
                    0,
                    ChargingProfilePurposeEnumType::ChargingStationMaxProfile,
                ),
            ),
            (
                2,
                clear_test_profile(
                    50,
                    0,
                    ChargingProfilePurposeEnumType::ChargingStationExternalConstraints,
                ),
            ),
        ]
    }

    /// The `(id, purpose)` projection of a match set — the readable form the
    /// combined-store tests assert store routing against (the wiring layer routes
    /// each removal to its store by the matched profile's purpose).
    fn matched_id_purposes(
        matches: &[(i32, ChargingProfileType)],
    ) -> Vec<(i32, ChargingProfilePurposeEnumType)> {
        matches
            .iter()
            .map(|(_, p)| (p.id, p.charging_profile_purpose))
            .collect()
    }

    #[test]
    fn clear_purpose_criterion_selects_only_the_matching_store_over_the_combined_set() {
        let store = combined_clear_store();
        // A TxDefaultProfile purpose filter narrows the combined set to the two
        // defaults (ids 30, 31) — never the TxProfile or either ceiling.
        let default = ClearChargingProfileType {
            evse_id: None,
            charging_profile_purpose: Some(ChargingProfilePurposeEnumType::TxDefaultProfile),
            stack_level: None,
            custom_data: None,
        };
        let mut got = matched_id_purposes(&v201_clear_charging_profile_matches(
            None,
            Some(&default),
            &store,
        ));
        got.sort_unstable_by_key(|(id, _)| *id);
        assert_eq!(
            got,
            vec![
                (30, ChargingProfilePurposeEnumType::TxDefaultProfile),
                (31, ChargingProfilePurposeEnumType::TxDefaultProfile),
            ]
        );
        // A ceiling purpose filter selects only that one ceiling — the two ceiling
        // purposes are distinct stores' keys, so a Max filter never hits External.
        let max = ClearChargingProfileType {
            evse_id: None,
            charging_profile_purpose: Some(
                ChargingProfilePurposeEnumType::ChargingStationMaxProfile,
            ),
            stack_level: None,
            custom_data: None,
        };
        assert_eq!(
            matched_id_purposes(&v201_clear_charging_profile_matches(
                None,
                Some(&max),
                &store
            )),
            vec![(
                40,
                ChargingProfilePurposeEnumType::ChargingStationMaxProfile
            )]
        );
    }

    #[test]
    fn clear_purposeless_request_spans_all_three_stores() {
        let store = combined_clear_store();
        // An empty request is the "clear all" wildcard across the combined set —
        // every installed profile in all three stores.
        let mut got = matched_id_purposes(&v201_clear_charging_profile_matches(None, None, &store));
        got.sort_unstable_by_key(|(id, _)| *id);
        assert_eq!(
            got,
            vec![
                (10, ChargingProfilePurposeEnumType::TxProfile),
                (30, ChargingProfilePurposeEnumType::TxDefaultProfile),
                (31, ChargingProfilePurposeEnumType::TxDefaultProfile),
                (
                    40,
                    ChargingProfilePurposeEnumType::ChargingStationMaxProfile
                ),
                (
                    50,
                    ChargingProfilePurposeEnumType::ChargingStationExternalConstraints
                ),
            ]
        );
    }

    #[test]
    fn clear_evse_id_zero_matches_station_wide_default_and_ceiling() {
        let store = combined_clear_store();
        // evseId 0 now targets the whole-station entries the default/ceiling stores
        // hold (ids 31, 40) and never a specific EVSE's TxProfile (id 10 on EVSE 1)
        // or the EVSE-2 external ceiling (id 50).
        let criteria = ClearChargingProfileType {
            evse_id: Some(0),
            charging_profile_purpose: None,
            stack_level: None,
            custom_data: None,
        };
        let mut got = matched_id_purposes(&v201_clear_charging_profile_matches(
            None,
            Some(&criteria),
            &store,
        ));
        got.sort_unstable_by_key(|(id, _)| *id);
        assert_eq!(
            got,
            vec![
                (31, ChargingProfilePurposeEnumType::TxDefaultProfile),
                (
                    40,
                    ChargingProfilePurposeEnumType::ChargingStationMaxProfile
                ),
            ]
        );
    }

    #[test]
    fn clear_by_profile_id_selects_a_default_or_ceiling_over_the_combined_set() {
        let store = combined_clear_store();
        // The exclusive id selector reaches into any store — here the whole-station
        // ceiling (id 40), proving Clear can retract a ceiling, not just a TxProfile.
        assert_eq!(
            matched_id_purposes(&v201_clear_charging_profile_matches(Some(40), None, &store)),
            vec![(
                40,
                ChargingProfilePurposeEnumType::ChargingStationMaxProfile
            )]
        );
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

    // ---- GetChargingProfiles → ReportChargingProfiles (#476) ----

    /// The EVSE keys the matcher returned, in order — the readable projection the
    /// `GetChargingProfiles` selector tests assert against.
    fn matched_evses(matches: &[(i32, ChargingProfileType)]) -> Vec<i32> {
        matches.iter().map(|(evse_id, _)| *evse_id).collect()
    }

    /// An all-`None` `ChargingProfileCriterionType` (the `{}` "any profile"
    /// criterion), which individual tests then narrow one field at a time.
    fn any_criterion() -> ChargingProfileCriterionType {
        ChargingProfileCriterionType {
            charging_profile_purpose: None,
            stack_level: None,
            charging_profile_id: None,
            charging_limit_source: None,
            custom_data: None,
        }
    }

    #[test]
    fn get_empty_criterion_without_evse_id_matches_every_installed_profile() {
        // Reuses the two-EVSE store from the ClearChargingProfile tests: EVSE 1
        // holds id 10 (stack 0), EVSE 2 holds id 20 (stack 3), both TxProfiles.
        let store = clear_test_store();
        assert_eq!(
            matched_evses(&v201_get_charging_profiles_matches(
                None,
                &any_criterion(),
                &store
            )),
            vec![1, 2]
        );
    }

    #[test]
    fn get_evse_id_restricts_the_report_to_that_evse() {
        let store = clear_test_store();
        assert_eq!(
            matched_evses(&v201_get_charging_profiles_matches(
                Some(1),
                &any_criterion(),
                &store
            )),
            vec![1]
        );
        assert_eq!(
            matched_evses(&v201_get_charging_profiles_matches(
                Some(2),
                &any_criterion(),
                &store
            )),
            vec![2]
        );
    }

    #[test]
    fn get_evse_id_zero_or_out_of_range_matches_nothing() {
        let store = clear_test_store();
        // evseId 0 targets the station-wide profiles the transaction-scoped store
        // never holds; a huge / negative id is simply absent from the snapshot.
        // A malformed CSMS evseId is a trust boundary — it misses, never panics.
        for evse_id in [0, -1, 999, i32::MIN] {
            assert!(
                v201_get_charging_profiles_matches(Some(evse_id), &any_criterion(), &store)
                    .is_empty(),
                "evseId {evse_id} must match nothing installed"
            );
        }
    }

    #[test]
    fn get_by_purpose_criterion_matches_txprofiles_and_excludes_others() {
        let store = clear_test_store();
        let tx = ChargingProfileCriterionType {
            charging_profile_purpose: Some(ChargingProfilePurposeEnumType::TxProfile),
            ..any_criterion()
        };
        assert_eq!(
            matched_evses(&v201_get_charging_profiles_matches(None, &tx, &store)),
            vec![1, 2]
        );
        let default = ChargingProfileCriterionType {
            charging_profile_purpose: Some(ChargingProfilePurposeEnumType::TxDefaultProfile),
            ..any_criterion()
        };
        assert!(v201_get_charging_profiles_matches(None, &default, &store).is_empty());
    }

    #[test]
    fn get_by_stack_level_criterion_selects_the_matching_profile() {
        let store = clear_test_store();
        let criterion = ChargingProfileCriterionType {
            stack_level: Some(3),
            ..any_criterion()
        };
        // Only EVSE 2's profile is at stack level 3.
        assert_eq!(
            matched_evses(&v201_get_charging_profiles_matches(
                None, &criterion, &store
            )),
            vec![2]
        );
    }

    #[test]
    fn get_by_profile_id_list_criterion_filters_by_membership() {
        let store = clear_test_store();
        // A single id names EVSE 2's profile...
        let one = ChargingProfileCriterionType {
            charging_profile_id: Some(vec![20]),
            ..any_criterion()
        };
        assert_eq!(
            matched_evses(&v201_get_charging_profiles_matches(None, &one, &store)),
            vec![2]
        );
        // ...both ids name both slots...
        let both = ChargingProfileCriterionType {
            charging_profile_id: Some(vec![10, 20]),
            ..any_criterion()
        };
        assert_eq!(
            matched_evses(&v201_get_charging_profiles_matches(None, &both, &store)),
            vec![1, 2]
        );
        // ...and an id nothing holds matches nothing.
        let none = ChargingProfileCriterionType {
            charging_profile_id: Some(vec![999]),
            ..any_criterion()
        };
        assert!(v201_get_charging_profiles_matches(None, &none, &store).is_empty());
    }

    #[test]
    fn get_by_charging_limit_source_criterion_treats_stored_profiles_as_cso() {
        let store = clear_test_store();
        // Every simulator-installed profile is CSMS-sourced (CSO), so a source
        // list that includes CSO does not exclude...
        let cso = ChargingProfileCriterionType {
            charging_limit_source: Some(vec![ChargingLimitSourceEnumType::Cso]),
            ..any_criterion()
        };
        assert_eq!(
            matched_evses(&v201_get_charging_profiles_matches(None, &cso, &store)),
            vec![1, 2]
        );
        let with_cso = ChargingProfileCriterionType {
            charging_limit_source: Some(vec![
                ChargingLimitSourceEnumType::Ems,
                ChargingLimitSourceEnumType::Cso,
            ]),
            ..any_criterion()
        };
        assert_eq!(
            matched_evses(&v201_get_charging_profiles_matches(None, &with_cso, &store)),
            vec![1, 2]
        );
        // ...but a source list that excludes CSO matches nothing installed.
        let ems_only = ChargingProfileCriterionType {
            charging_limit_source: Some(vec![ChargingLimitSourceEnumType::Ems]),
            ..any_criterion()
        };
        assert!(v201_get_charging_profiles_matches(None, &ems_only, &store).is_empty());
    }

    #[test]
    fn charging_limit_source_maps_purpose_to_provenance() {
        // Operator configuration → CSO; an external-constraints ceiling → SO (#551).
        for purpose in [
            ChargingProfilePurposeEnumType::TxProfile,
            ChargingProfilePurposeEnumType::TxDefaultProfile,
            ChargingProfilePurposeEnumType::ChargingStationMaxProfile,
        ] {
            assert_eq!(
                v201_charging_limit_source(purpose),
                ChargingLimitSourceEnumType::Cso,
                "{purpose:?} is CSO operator configuration"
            );
        }
        assert_eq!(
            v201_charging_limit_source(
                ChargingProfilePurposeEnumType::ChargingStationExternalConstraints
            ),
            ChargingLimitSourceEnumType::So,
            "an external-constraints ceiling reports under the external SO source"
        );
    }

    #[test]
    fn get_criteria_and_evse_id_are_conjunctive() {
        let store = clear_test_store();
        // evseId 1 AND stackLevel 3 — EVSE 1's profile is at stack 0, so the two
        // together exclude it, matching nothing.
        let criterion = ChargingProfileCriterionType {
            stack_level: Some(3),
            ..any_criterion()
        };
        assert!(v201_get_charging_profiles_matches(Some(1), &criterion, &store).is_empty());
        // The same criterion on EVSE 2 (stack 3) does match.
        assert_eq!(
            matched_evses(&v201_get_charging_profiles_matches(
                Some(2),
                &criterion,
                &store
            )),
            vec![2]
        );
    }

    #[test]
    fn get_empty_store_matches_nothing() {
        assert!(v201_get_charging_profiles_matches(None, &any_criterion(), &[]).is_empty());
    }

    #[test]
    fn get_response_maps_matched_to_accepted_and_unmatched_to_no_profiles() {
        assert_eq!(
            v201_get_charging_profiles_response(true).status,
            GetChargingProfileStatusEnumType::Accepted
        );
        assert_eq!(
            v201_get_charging_profiles_response(false).status,
            GetChargingProfileStatusEnumType::NoProfiles
        );
        // The synchronous response carries no profile data or status detail.
        let resp = v201_get_charging_profiles_response(true);
        assert!(resp.status_info.is_none());
    }

    /// Wire fidelity: both built `GetChargingProfiles.conf` values satisfy the
    /// bundled OCPP 2.0.1 `GetChargingProfilesResponse` JSON Schema.
    #[test]
    fn built_get_charging_profiles_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        for matched in [true, false] {
            let resp = v201_get_charging_profiles_response(matched);
            let payload = serde_json::to_value(&resp).unwrap();
            assert!(
                validator
                    .validate_call_result("GetChargingProfiles", &payload)
                    .is_ok(),
                "built GetChargingProfilesResponse (matched={matched}) should be schema-valid, got: {payload}"
            );
        }
    }

    #[test]
    fn report_pages_single_evse_is_one_unpaged_cso_page() {
        let matched = vec![(
            1,
            clear_test_profile(10, 0, ChargingProfilePurposeEnumType::TxProfile),
        )];
        let pages = v201_report_charging_profiles_pages(77, &matched);
        assert_eq!(pages.len(), 1, "one matched EVSE → one page");
        let page = &pages[0];
        assert_eq!(
            page.request_id, 77,
            "the page echoes the GetChargingProfiles requestId"
        );
        assert_eq!(page.evse_id, 1);
        assert_eq!(page.charging_limit_source, ChargingLimitSourceEnumType::Cso);
        assert_eq!(
            page.charging_profile.len(),
            1,
            "the page carries the EVSE's profile"
        );
        assert_eq!(page.charging_profile[0].id, 10);
        assert!(
            !page.tbc.unwrap_or(false),
            "a single page is not 'to be continued'"
        );
    }

    #[test]
    fn report_pages_multi_evse_are_ordered_by_evse_and_tbc_paged() {
        // Deliberately out of order to prove the builder sorts: EVSE 2 before 1.
        let matched = vec![
            (
                2,
                clear_test_profile(20, 3, ChargingProfilePurposeEnumType::TxProfile),
            ),
            (
                1,
                clear_test_profile(10, 0, ChargingProfilePurposeEnumType::TxProfile),
            ),
        ];
        let pages = v201_report_charging_profiles_pages(5, &matched);
        assert_eq!(pages.len(), 2);
        // Sorted ascending by evseId regardless of snapshot order.
        assert_eq!(pages[0].evse_id, 1);
        assert_eq!(pages[1].evse_id, 2);
        // Every page but the last is 'to be continued'; the last is not.
        assert_eq!(pages[0].tbc, Some(true), "the first of two pages continues");
        assert!(
            !pages[1].tbc.unwrap_or(false),
            "the last page is not 'to be continued'"
        );
        // Both echo the same requestId.
        assert!(pages.iter().all(|p| p.request_id == 5));
    }

    #[test]
    fn report_pages_empty_match_builds_no_pages() {
        assert!(v201_report_charging_profiles_pages(1, &[]).is_empty());
    }

    /// Wire fidelity: every built `ReportChargingProfiles` CALL — single-page and
    /// each page of a multi-page stream — satisfies the bundled OCPP 2.0.1
    /// `ReportChargingProfiles` request JSON Schema.
    #[test]
    fn built_report_charging_profiles_pages_are_schema_valid() {
        let validator = SchemaValidator::v201();
        let matched = vec![
            (
                1,
                clear_test_profile(10, 0, ChargingProfilePurposeEnumType::TxProfile),
            ),
            (
                2,
                clear_test_profile(20, 3, ChargingProfilePurposeEnumType::TxProfile),
            ),
        ];
        let pages = v201_report_charging_profiles_pages(42, &matched);
        assert_eq!(pages.len(), 2);
        for page in &pages {
            let payload = serde_json::to_value(page).unwrap();
            assert!(
                validator
                    .validate_call("ReportChargingProfiles", &payload)
                    .is_ok(),
                "built ReportChargingProfiles page (evse={}) should be schema-valid, got: {payload}",
                page.evse_id
            );
        }
    }

    // ---- GetChargingProfiles across all three stores (#519) ----

    /// The combined `(evse_id, profile)` snapshot the #519 wiring layer feeds the
    /// selector: a per-EVSE `TxProfile` and `TxDefaultProfile` on EVSE 1, plus the
    /// two station ceilings on the whole-station key `0`. Deliberately unsorted to
    /// mimic the stores' `HashMap`-walk snapshot order.
    fn combined_profile_store() -> Vec<(i32, ChargingProfileType)> {
        vec![
            (
                1,
                clear_test_profile(10, 0, ChargingProfilePurposeEnumType::TxProfile),
            ),
            (
                0,
                clear_test_profile(
                    50,
                    0,
                    ChargingProfilePurposeEnumType::ChargingStationExternalConstraints,
                ),
            ),
            (
                1,
                clear_test_profile(30, 1, ChargingProfilePurposeEnumType::TxDefaultProfile),
            ),
            (
                0,
                clear_test_profile(
                    40,
                    0,
                    ChargingProfilePurposeEnumType::ChargingStationMaxProfile,
                ),
            ),
        ]
    }

    /// The profile ids the selector returned, sorted — a store-order-independent
    /// projection the #519 enumeration tests assert against.
    fn matched_ids(matches: &[(i32, ChargingProfileType)]) -> Vec<i32> {
        let mut ids: Vec<i32> = matches.iter().map(|(_, p)| p.id).collect();
        ids.sort_unstable();
        ids
    }

    #[test]
    fn get_empty_criterion_enumerates_tx_default_and_ceiling_profiles_too() {
        // The #519 fix: with no criterion and no evseId, the report covers every
        // installed profile across all three stores — not just the TxProfiles.
        let store = combined_profile_store();
        assert_eq!(
            matched_ids(&v201_get_charging_profiles_matches(
                None,
                &any_criterion(),
                &store
            )),
            vec![10, 30, 40, 50],
            "TxProfile (10), TxDefaultProfile (30), and both ceilings (40, 50) are all reported"
        );
    }

    #[test]
    fn get_purpose_criterion_narrows_to_that_store_purpose() {
        let store = combined_profile_store();
        // Only the external-constraints ceiling.
        let ext = ChargingProfileCriterionType {
            charging_profile_purpose: Some(
                ChargingProfilePurposeEnumType::ChargingStationExternalConstraints,
            ),
            ..any_criterion()
        };
        assert_eq!(
            matched_ids(&v201_get_charging_profiles_matches(None, &ext, &store)),
            vec![50],
            "a ChargingStationExternalConstraints criterion returns only the ceiling"
        );
        // Only the TxDefaultProfile.
        let def = ChargingProfileCriterionType {
            charging_profile_purpose: Some(ChargingProfilePurposeEnumType::TxDefaultProfile),
            ..any_criterion()
        };
        assert_eq!(
            matched_ids(&v201_get_charging_profiles_matches(None, &def, &store)),
            vec![30],
            "a TxDefaultProfile criterion returns only the default, not the TxProfile or ceilings"
        );
    }

    #[test]
    fn get_evse_zero_selects_only_whole_station_entries() {
        let store = combined_profile_store();
        // evseId = 0 → the whole-station ceilings the default/ceiling stores hold
        // there, and never the EVSE-1 transaction/default profiles.
        assert_eq!(
            matched_ids(&v201_get_charging_profiles_matches(
                Some(0),
                &any_criterion(),
                &store
            )),
            vec![40, 50],
            "a whole-station (evseId 0) request reports exactly the station-wide ceilings"
        );
        // A specific-EVSE request never mis-scopes a whole-station `0` entry to itself.
        assert_eq!(
            matched_ids(&v201_get_charging_profiles_matches(
                Some(1),
                &any_criterion(),
                &store
            )),
            vec![10, 30],
            "an evseId 1 request reports only EVSE-1 entries, not the whole-station ceilings"
        );
    }

    #[test]
    fn get_charging_limit_source_criterion_distinguishes_cso_from_external() {
        // The combined store holds three CSO profiles (TxProfile 10, TxDefault 30,
        // Max ceiling 40) and one external-constraints ceiling (50) that reports
        // under SO. A source criterion now filters by each profile's derived
        // source, not a blanket CSO (#551).
        let store = combined_profile_store();
        // `[Cso]` selects the operator profiles and never the external ceiling.
        let cso = ChargingProfileCriterionType {
            charging_limit_source: Some(vec![ChargingLimitSourceEnumType::Cso]),
            ..any_criterion()
        };
        assert_eq!(
            matched_ids(&v201_get_charging_profiles_matches(None, &cso, &store)),
            vec![10, 30, 40],
            "a [Cso] criterion returns the operator profiles, excluding the external ceiling"
        );
        // `[SO]` selects only the external-constraints ceiling.
        let so = ChargingProfileCriterionType {
            charging_limit_source: Some(vec![ChargingLimitSourceEnumType::So]),
            ..any_criterion()
        };
        assert_eq!(
            matched_ids(&v201_get_charging_profiles_matches(None, &so, &store)),
            vec![50],
            "an [SO] criterion returns only the external-constraints ceiling"
        );
        // A source present in neither (`Other`) matches nothing.
        let other = ChargingProfileCriterionType {
            charging_limit_source: Some(vec![ChargingLimitSourceEnumType::Other]),
            ..any_criterion()
        };
        assert!(
            v201_get_charging_profiles_matches(None, &other, &store).is_empty(),
            "an [Other] criterion matches nothing installed"
        );
        // A union of both sources reports the whole combined set.
        let both = ChargingProfileCriterionType {
            charging_limit_source: Some(vec![
                ChargingLimitSourceEnumType::Cso,
                ChargingLimitSourceEnumType::So,
            ]),
            ..any_criterion()
        };
        assert_eq!(
            matched_ids(&v201_get_charging_profiles_matches(None, &both, &store)),
            vec![10, 30, 40, 50],
            "a [Cso, SO] criterion reports every installed profile"
        );
    }

    #[test]
    fn report_pages_group_same_source_profiles_and_split_cross_source() {
        // EVSE 1 holds a TxProfile + a TxDefaultProfile (both CSO source): they
        // share a source, so they group into one page. EVSE 0 holds a
        // ChargingStationMaxProfile (CSO) and a ChargingStationExternalConstraints
        // ceiling (SO): distinct sources, so they split into two pages (#551).
        // Profiles within a page are sorted by id, pages ordered by (evseId,
        // source), tbc set on every page but the last. Ids/inputs deliberately out
        // of order to prove the sorts.
        let matched = vec![
            (
                1,
                clear_test_profile(30, 1, ChargingProfilePurposeEnumType::TxDefaultProfile),
            ),
            (
                0,
                clear_test_profile(
                    50,
                    0,
                    ChargingProfilePurposeEnumType::ChargingStationExternalConstraints,
                ),
            ),
            (
                1,
                clear_test_profile(10, 0, ChargingProfilePurposeEnumType::TxProfile),
            ),
            (
                0,
                clear_test_profile(
                    40,
                    0,
                    ChargingProfilePurposeEnumType::ChargingStationMaxProfile,
                ),
            ),
        ];
        let pages = v201_report_charging_profiles_pages(9, &matched);
        assert_eq!(
            pages.len(),
            3,
            "EVSE 0 splits into an SO + a CSO page; EVSE 1's two CSO profiles group into one"
        );
        // Ordered by (evseId, source); on EVSE 0 the SO page precedes the CSO page
        // (declaration order EMS < Other < SO < CSO).
        // Page 0: EVSE 0, the external ceiling under SO.
        assert_eq!(pages[0].evse_id, 0);
        assert_eq!(
            pages[0].charging_limit_source,
            ChargingLimitSourceEnumType::So
        );
        assert_eq!(
            pages[0]
                .charging_profile
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            vec![50],
            "the external-constraints ceiling reports alone under SO"
        );
        assert_eq!(pages[0].tbc, Some(true), "not the last page");
        // Page 1: EVSE 0, the operator max-profile ceiling under CSO.
        assert_eq!(pages[1].evse_id, 0);
        assert_eq!(
            pages[1].charging_limit_source,
            ChargingLimitSourceEnumType::Cso
        );
        assert_eq!(
            pages[1]
                .charging_profile
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            vec![40],
            "the operator ceiling reports alone under CSO"
        );
        assert_eq!(pages[1].tbc, Some(true), "not the last page");
        // Page 2: EVSE 1, the TxProfile + TxDefaultProfile grouped under CSO.
        assert_eq!(pages[2].evse_id, 1);
        assert_eq!(
            pages[2].charging_limit_source,
            ChargingLimitSourceEnumType::Cso
        );
        assert_eq!(
            pages[2]
                .charging_profile
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            vec![10, 30],
            "same-source profiles on one EVSE group into a single page, sorted by id"
        );
        assert!(
            !pages[2].tbc.unwrap_or(false),
            "the last page is not 'to be continued'"
        );
        assert!(pages.iter().all(|p| p.request_id == 9));
    }

    #[test]
    fn built_multi_profile_report_page_is_schema_valid() {
        // A single EVSE-1 page carrying two same-source (CSO) profiles — a
        // TxProfile and a TxDefaultProfile — must satisfy the bundled OCPP 2.0.1
        // ReportChargingProfiles schema (minItems: 1 on chargingProfile, multiple
        // entries allowed).
        let validator = SchemaValidator::v201();
        let matched = vec![
            (
                1,
                clear_test_profile(10, 0, ChargingProfilePurposeEnumType::TxProfile),
            ),
            (
                1,
                clear_test_profile(30, 1, ChargingProfilePurposeEnumType::TxDefaultProfile),
            ),
        ];
        let pages = v201_report_charging_profiles_pages(1, &matched);
        assert_eq!(
            pages.len(),
            1,
            "two CSO profiles on EVSE 1 group into one page"
        );
        assert_eq!(pages[0].charging_profile.len(), 2);
        let payload = serde_json::to_value(&pages[0]).unwrap();
        assert!(
            validator
                .validate_call("ReportChargingProfiles", &payload)
                .is_ok(),
            "a multi-profile ReportChargingProfiles page should be schema-valid, got: {payload}"
        );
    }

    /// Wire fidelity: an SO-sourced `ReportChargingProfiles` page (an external
    /// `ChargingStationExternalConstraints` ceiling) satisfies the bundled OCPP
    /// 2.0.1 schema — the `chargingLimitSource` enum accepts `SO`, not just `CSO`.
    #[test]
    fn built_external_constraint_report_page_reports_so_and_is_schema_valid() {
        let validator = SchemaValidator::v201();
        let matched = vec![(
            0,
            clear_test_profile(
                50,
                0,
                ChargingProfilePurposeEnumType::ChargingStationExternalConstraints,
            ),
        )];
        let pages = v201_report_charging_profiles_pages(7, &matched);
        assert_eq!(pages.len(), 1);
        assert_eq!(
            pages[0].charging_limit_source,
            ChargingLimitSourceEnumType::So
        );
        let payload = serde_json::to_value(&pages[0]).unwrap();
        assert!(
            validator
                .validate_call("ReportChargingProfiles", &payload)
                .is_ok(),
            "an SO-sourced ReportChargingProfiles page should be schema-valid, got: {payload}"
        );
    }

    // --- ReserveNow (v201) -------------------------------------------------

    #[test]
    fn reserve_now_accepts_a_free_evse() {
        // The happy path: a known, idle EVSE with a still-valid expiry → Accepted,
        // no statusInfo (the enum is the whole story).
        let (status, info) = v201_reserve_now_status(false, Some(1), ConnectorReservState::Free);
        assert_eq!(status, ReserveNowStatusEnumType::Accepted);
        assert!(
            info.is_none(),
            "an accepted reservation carries no statusInfo"
        );
    }

    #[test]
    fn reserve_now_reports_occupied_for_a_busy_evse() {
        // A connector mid-cycle (transaction under way / already reserved) → Occupied.
        let (status, info) = v201_reserve_now_status(false, Some(1), ConnectorReservState::Busy);
        assert_eq!(status, ReserveNowStatusEnumType::Occupied);
        assert!(info.is_none());
    }

    #[test]
    fn reserve_now_maps_inoperative_states_faithfully() {
        // Faulted → Faulted, Unavailable → Unavailable — the connector-status
        // matrix mirrored from the 1.6J handler, each reported in its own dialect.
        assert_eq!(
            v201_reserve_now_status(false, Some(2), ConnectorReservState::Faulted).0,
            ReserveNowStatusEnumType::Faulted
        );
        assert_eq!(
            v201_reserve_now_status(false, Some(2), ConnectorReservState::Unavailable).0,
            ReserveNowStatusEnumType::Unavailable
        );
    }

    #[test]
    fn reserve_now_rejects_an_unknown_evse() {
        // A structurally-valid id the station has no EVSE for → Rejected + reason.
        let (status, info) =
            v201_reserve_now_status(false, Some(99), ConnectorReservState::Missing);
        assert_eq!(status, ReserveNowStatusEnumType::Rejected);
        assert_eq!(
            info.expect("a refusal carries a reason").reason_code,
            "UnknownEvse"
        );
    }

    #[test]
    fn reserve_now_rejects_station_level_reservation() {
        // evseId omitted = reserve the whole station, which the flat simulator
        // cannot hold → Rejected, regardless of any connector's state.
        let (status, info) = v201_reserve_now_status(false, None, ConnectorReservState::Free);
        assert_eq!(status, ReserveNowStatusEnumType::Rejected);
        assert_eq!(
            info.expect("a refusal carries a reason").reason_code,
            "StationLevel"
        );
    }

    #[test]
    fn reserve_now_rejects_a_structurally_invalid_evse_id() {
        // evseId 0 / negative addresses no EVSE — never a panic, always Rejected,
        // even when the (irrelevant) distilled state would otherwise be Free.
        for evse_id in [0, -1, i32::MIN] {
            let (status, info) =
                v201_reserve_now_status(false, Some(evse_id), ConnectorReservState::Free);
            assert_eq!(
                status,
                ReserveNowStatusEnumType::Rejected,
                "evseId {evse_id} must be rejected"
            );
            assert_eq!(info.unwrap().reason_code, "UnknownEvse");
        }
    }

    #[test]
    fn reserve_now_rejects_an_already_expired_reservation() {
        // An expiry at or before now would auto-free instantly; rejected outright,
        // ahead of (and independent of) the target's live state.
        let (status, info) = v201_reserve_now_status(true, Some(1), ConnectorReservState::Free);
        assert_eq!(status, ReserveNowStatusEnumType::Rejected);
        assert_eq!(
            info.expect("a refusal carries a reason").reason_code,
            "ExpiredReservation"
        );
    }

    #[test]
    fn reserve_now_response_round_trips_status_and_info_and_is_schema_valid() {
        let validator = SchemaValidator::v201();
        // Accepted, no statusInfo.
        let accepted = v201_reserve_now_response(ReserveNowStatusEnumType::Accepted, None);
        assert_eq!(accepted.status, ReserveNowStatusEnumType::Accepted);
        assert!(accepted.status_info.is_none());
        let payload = serde_json::to_value(&accepted).unwrap();
        assert!(
            validator
                .validate_call_result("ReserveNow", &payload)
                .is_ok(),
            "built ReserveNow Accepted response should be schema-valid, got: {payload}"
        );
        // Rejected carrying the refusal reason.
        let (status, info) = v201_reserve_now_status(false, None, ConnectorReservState::Free);
        let rejected = v201_reserve_now_response(status, info);
        let payload = serde_json::to_value(&rejected).unwrap();
        assert!(
            validator
                .validate_call_result("ReserveNow", &payload)
                .is_ok(),
            "built ReserveNow Rejected response should be schema-valid, got: {payload}"
        );
    }

    // --- CancelReservation (v201) ------------------------------------------

    /// A `reservationId` the station currently holds is `Accepted` — whether it
    /// is the only reservation or one among several.
    #[test]
    fn cancel_reservation_accepts_a_held_id() {
        assert_eq!(
            v201_cancel_reservation_status(7, &[7]),
            CancelReservationStatusEnumType::Accepted
        );
        assert_eq!(
            v201_cancel_reservation_status(7, &[1, 7, 42]),
            CancelReservationStatusEnumType::Accepted,
            "a held id among several is still Accepted"
        );
    }

    /// An id the station is not holding — and an idle station holding nothing —
    /// both fold to `Rejected` (nothing to cancel), mirroring the 1.6J
    /// `None => Rejected` arm.
    #[test]
    fn cancel_reservation_rejects_an_unknown_id() {
        assert_eq!(
            v201_cancel_reservation_status(99, &[1, 7, 42]),
            CancelReservationStatusEnumType::Rejected,
            "an id naming no held reservation is Rejected"
        );
        assert_eq!(
            v201_cancel_reservation_status(7, &[]),
            CancelReservationStatusEnumType::Rejected,
            "a station holding no reservations Rejects any id"
        );
    }

    /// Trust boundary on the CSMS-supplied id: a zero, negative, or `i32::MIN`
    /// `reservationId` is a plain membership miss — `Rejected`, never a panic,
    /// index, or cast.
    #[test]
    fn cancel_reservation_rejects_structurally_odd_ids_without_panicking() {
        for requested in [0, -1, i32::MIN] {
            assert_eq!(
                v201_cancel_reservation_status(requested, &[1, 2, 3]),
                CancelReservationStatusEnumType::Rejected,
                "reservationId {requested} should miss and be Rejected"
            );
        }
        // …and such a value is honored when it genuinely names a held reservation
        // (the store keys on the raw i32, so no id is structurally excluded).
        assert_eq!(
            v201_cancel_reservation_status(i32::MIN, &[i32::MIN]),
            CancelReservationStatusEnumType::Accepted
        );
    }

    #[test]
    fn cancel_reservation_response_carries_status_and_optional_status_info() {
        let bare =
            v201_cancel_reservation_response(CancelReservationStatusEnumType::Accepted, None);
        assert_eq!(bare.status, CancelReservationStatusEnumType::Accepted);
        assert!(bare.status_info.is_none());

        let info = StatusInfoType {
            reason_code: "NoReservation".to_string(),
            additional_info: Some("no reservation with that id is held".to_string()),
            custom_data: None,
        };
        let rejected =
            v201_cancel_reservation_response(CancelReservationStatusEnumType::Rejected, Some(info));
        assert_eq!(rejected.status, CancelReservationStatusEnumType::Rejected);
        assert_eq!(
            rejected
                .status_info
                .as_ref()
                .map(|i| i.reason_code.as_str()),
            Some("NoReservation")
        );
    }

    /// Wire fidelity: every built `CancelReservation.conf` — with and without
    /// `statusInfo`, across both status values — satisfies the bundled OCPP 2.0.1
    /// `CancelReservationResponse` JSON Schema, the same guarantee the CP's
    /// version-aware validator gives on the live path.
    #[test]
    fn built_cancel_reservation_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "NoReservation".to_string(),
            additional_info: Some("no reservation with that id is held".to_string()),
            custom_data: None,
        };
        for status in [
            CancelReservationStatusEnumType::Accepted,
            CancelReservationStatusEnumType::Rejected,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_cancel_reservation_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("CancelReservation", &payload)
                        .is_ok(),
                    "built {status:?} CancelReservationResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    #[test]
    fn reservation_status_update_carries_id_and_status() {
        // The builder forwards the reservationId and the decided status verbatim,
        // and leaves the vendor extension unset.
        let req = v201_reservation_status_update(42, ReservationUpdateStatusEnumType::Expired);
        assert_eq!(req.reservation_id, 42);
        assert_eq!(
            req.reservation_update_status,
            ReservationUpdateStatusEnumType::Expired
        );
        assert!(req.custom_data.is_none());
    }

    /// Wire fidelity: a built `ReservationStatusUpdate.req` satisfies the bundled
    /// OCPP 2.0.1 schema for both status values and for extreme `reservationId`s
    /// (the id is only echoed, never parsed — `i32::MIN`/`MAX` must not panic and
    /// must still serialize to a schema-valid CALL).
    #[test]
    fn built_reservation_status_updates_are_schema_valid() {
        let validator = SchemaValidator::v201();
        for status in [
            ReservationUpdateStatusEnumType::Expired,
            ReservationUpdateStatusEnumType::Removed,
        ] {
            for id in [0, 1, -1, i32::MIN, i32::MAX] {
                let req = v201_reservation_status_update(id, status);
                let payload = serde_json::to_value(&req).unwrap();
                assert!(
                    validator
                        .validate_call("ReservationStatusUpdate", &payload)
                        .is_ok(),
                    "built {status:?} ReservationStatusUpdate (id {id}) should be schema-valid, \
                     got: {payload}"
                );
            }
        }
    }

    #[test]
    fn clear_cache_accepts_when_the_station_supports_caching() {
        // The simulator owns an AuthCache, so the live answer is always Accepted.
        assert_eq!(
            v201_clear_cache_status(true),
            ClearCacheStatusEnumType::Accepted
        );
    }

    #[test]
    fn clear_cache_rejects_when_the_station_has_no_cache() {
        // The modeled seam for a future opt-in "no caching" station: nothing to
        // clear => Rejected.
        assert_eq!(
            v201_clear_cache_status(false),
            ClearCacheStatusEnumType::Rejected
        );
    }

    #[test]
    fn clear_cache_response_carries_status_and_optional_status_info() {
        let bare = v201_clear_cache_response(ClearCacheStatusEnumType::Accepted, None);
        assert_eq!(bare.status, ClearCacheStatusEnumType::Accepted);
        assert!(bare.status_info.is_none());

        let info = StatusInfoType {
            reason_code: "NoCache".to_string(),
            additional_info: Some("station has no authorization cache".to_string()),
            custom_data: None,
        };
        let rejected = v201_clear_cache_response(ClearCacheStatusEnumType::Rejected, Some(info));
        assert_eq!(rejected.status, ClearCacheStatusEnumType::Rejected);
        assert_eq!(
            rejected
                .status_info
                .as_ref()
                .map(|i| i.reason_code.as_str()),
            Some("NoCache")
        );
    }

    /// Wire fidelity: every built `ClearCache.conf` — with and without
    /// `statusInfo`, across both status values — satisfies the bundled OCPP 2.0.1
    /// `ClearCacheResponse` JSON Schema, the same guarantee the CP's version-aware
    /// validator gives on the live path.
    #[test]
    fn built_clear_cache_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "NoCache".to_string(),
            additional_info: Some("station has no authorization cache".to_string()),
            custom_data: None,
        };
        for status in [
            ClearCacheStatusEnumType::Accepted,
            ClearCacheStatusEnumType::Rejected,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_clear_cache_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("ClearCache", &payload)
                        .is_ok(),
                    "built {status:?} ClearCacheResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    // --- GetMonitoringReport (v201, #493) ----------------------------------

    #[test]
    fn get_monitoring_report_empty_snapshot_is_empty_result_set() {
        // No monitor matched (the simulator's standing state, option b): the
        // request was understood but there is nothing to stream. `queued` is
        // irrelevant when there are no monitors.
        assert_eq!(
            v201_get_monitoring_report_status(false, false),
            GenericDeviceModelStatusEnumType::EmptyResultSet
        );
        assert_eq!(
            v201_get_monitoring_report_status(false, true),
            GenericDeviceModelStatusEnumType::EmptyResultSet
        );
    }

    #[test]
    fn get_monitoring_report_queued_snapshot_is_accepted() {
        // A non-empty snapshot handed to the command channel → Accepted; the
        // NotifyMonitoringReport follows off the CALL path.
        assert_eq!(
            v201_get_monitoring_report_status(true, true),
            GenericDeviceModelStatusEnumType::Accepted
        );
    }

    #[test]
    fn get_monitoring_report_consumer_gone_is_rejected() {
        // Monitors matched but the command consumer is gone (CP shutting down):
        // we cannot stream the report, so we do not promise it.
        assert_eq!(
            v201_get_monitoring_report_status(true, false),
            GenericDeviceModelStatusEnumType::Rejected
        );
    }

    #[test]
    fn built_get_monitoring_report_responses_are_schema_valid() {
        // Every built response — across all three statuses, with and without a
        // statusInfo — satisfies the bundled OCPP 2.0.1 GetMonitoringReport
        // response JSON Schema.
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "NoMonitors".to_string(),
            additional_info: Some("no variable monitors are installed".to_string()),
            custom_data: None,
        };
        for status in [
            GenericDeviceModelStatusEnumType::Accepted,
            GenericDeviceModelStatusEnumType::Rejected,
            GenericDeviceModelStatusEnumType::EmptyResultSet,
            GenericDeviceModelStatusEnumType::NotSupported,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_get_monitoring_report_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("GetMonitoringReport", &payload)
                        .is_ok(),
                    "built {status:?} GetMonitoringReportResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    // --- SetMonitoringLevel: the reporting-level response builder (#500) ----

    #[test]
    fn built_set_monitoring_level_responses_are_schema_valid() {
        // Every built response — both statuses, with and without a statusInfo —
        // satisfies the bundled OCPP 2.0.1 SetMonitoringLevel response JSON
        // Schema. The Rejected/OutOfRange shape is the exact one the handler
        // emits for an out-of-range severity.
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "OutOfRange".to_string(),
            additional_info: Some("severity must be in 0..=9 (0 Danger … 9 Debug)".to_string()),
            custom_data: None,
        };
        for status in [
            GenericStatusEnumType::Accepted,
            GenericStatusEnumType::Rejected,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_set_monitoring_level_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("SetMonitoringLevel", &payload)
                        .is_ok(),
                    "built {status:?} SetMonitoringLevelResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    // --- SetMonitoringBase: the active-base response builder (#501) ---------

    #[test]
    fn built_set_monitoring_base_responses_are_schema_valid() {
        // Every built response — each device-model status, with and without a
        // statusInfo — satisfies the bundled OCPP 2.0.1 SetMonitoringBase response
        // JSON Schema. `Accepted` (no statusInfo) and `NotSupported` (the
        // HardWiredOnly seam, with statusInfo) are the exact shapes the handler
        // emits; the remaining statuses round-trip for completeness.
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "NotSupported".to_string(),
            additional_info: Some(
                "HardWiredOnly base is not modeled: no hard-wired monitors exist".to_string(),
            ),
            custom_data: None,
        };
        for status in [
            GenericDeviceModelStatusEnumType::Accepted,
            GenericDeviceModelStatusEnumType::Rejected,
            GenericDeviceModelStatusEnumType::NotSupported,
            GenericDeviceModelStatusEnumType::EmptyResultSet,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_set_monitoring_base_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("SetMonitoringBase", &payload)
                        .is_ok(),
                    "built {status:?} SetMonitoringBaseResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    // --- GetTransactionStatus (v201, #490) ---------------------------------

    #[test]
    fn get_transaction_status_station_wide_query_omits_ongoing_indicator() {
        // No transactionId → a station-wide query about queued messages only; the
        // ongoingIndicator is left absent rather than fabricating a verdict, and
        // messagesInQueue passes through.
        assert_eq!(
            v201_get_transaction_status(None, &["1", "2"], false),
            (false, None)
        );
    }

    #[test]
    fn get_transaction_status_live_id_is_ongoing() {
        // A transactionId the station is actually running → Some(true).
        assert_eq!(
            v201_get_transaction_status(Some("2"), &["1", "2"], false),
            (false, Some(true))
        );
    }

    #[test]
    fn get_transaction_status_unknown_or_ended_id_is_not_ongoing() {
        // An id not in the live set — unknown, already ended, or an idle station
        // (empty set) — all fold to the modeled "not ongoing" answer, Some(false).
        assert_eq!(
            v201_get_transaction_status(Some("9"), &["1", "2"], false),
            (false, Some(false))
        );
        assert_eq!(
            v201_get_transaction_status(Some("1"), &[], false),
            (false, Some(false))
        );
    }

    #[test]
    fn get_transaction_status_matches_on_exact_string_never_parses() {
        // Trust boundary: matching is exact string equality over the station-minted
        // decimal id, not a numeric parse. A non-canonical spelling of a live id
        // fails to match and reports Some(false) — never panics, never coerces.
        assert_eq!(
            v201_get_transaction_status(Some("07"), &["7"], false),
            (false, Some(false)),
            "a non-canonical spelling of a live id must not match"
        );
        assert_eq!(
            v201_get_transaction_status(Some(" 7 "), &["7"], false),
            (false, Some(false))
        );
    }

    #[test]
    fn get_transaction_status_reports_messages_in_queue_unchanged() {
        // The queued-message flag is reported through verbatim, independent of the
        // transactionId branch.
        assert_eq!(
            v201_get_transaction_status(None, &[], true),
            (true, None),
            "messagesInQueue passes through on a station-wide query"
        );
        assert_eq!(
            v201_get_transaction_status(Some("1"), &["1"], true),
            (true, Some(true)),
            "messagesInQueue passes through alongside a per-transaction verdict"
        );
    }

    #[test]
    fn built_get_transaction_status_responses_are_schema_valid() {
        // Every built response — messagesInQueue true/false crossed with the three
        // ongoingIndicator shapes (absent / Some(true) / Some(false)) — satisfies
        // the bundled OCPP 2.0.1 GetTransactionStatus response JSON Schema.
        let validator = SchemaValidator::v201();
        for messages_in_queue in [false, true] {
            for ongoing_indicator in [None, Some(true), Some(false)] {
                let resp =
                    v201_get_transaction_status_response(messages_in_queue, ongoing_indicator);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("GetTransactionStatus", &payload)
                        .is_ok(),
                    "built GetTransactionStatusResponse (messagesInQueue={messages_in_queue}, \
                     ongoingIndicator={ongoing_indicator:?}) should be schema-valid, got: {payload}"
                );
            }
        }
    }

    // --- SetDisplayMessage (v201, #505) ------------------------------------

    /// A minimal schema-shaped `MessageInfoType` with the given `id`, optionally
    /// bound to a `transactionId`, for the decision tests.
    fn display_message(id: i32, transaction_id: Option<&str>) -> MessageInfoType {
        use ocpp_types::v201::{
            MessageContentType, MessageFormatEnumType, MessagePriorityEnumType,
        };
        MessageInfoType {
            id,
            priority: MessagePriorityEnumType::NormalCycle,
            message: MessageContentType {
                format: MessageFormatEnumType::Utf8,
                content: "Welcome".to_string(),
                language: None,
                custom_data: None,
            },
            state: None,
            start_date_time: None,
            end_date_time: None,
            transaction_id: transaction_id.map(str::to_owned),
            display: None,
            custom_data: None,
        }
    }

    #[test]
    fn set_display_message_station_wide_message_is_accepted() {
        // A message that binds no transaction is a plain station-wide install →
        // Accepted, no statusInfo, independent of what the station is running.
        let msg = display_message(1, None);
        assert_eq!(
            v201_set_display_message_status(&msg, &["7"]),
            (DisplayMessageStatusEnumType::Accepted, None)
        );
        assert_eq!(
            v201_set_display_message_status(&msg, &[]),
            (DisplayMessageStatusEnumType::Accepted, None),
            "a station-wide message is accepted even on an idle station"
        );
    }

    #[test]
    fn set_display_message_bound_to_a_live_transaction_is_accepted() {
        // A message scoped to a transaction the station IS running → Accepted.
        let msg = display_message(2, Some("7"));
        let (status, status_info) = v201_set_display_message_status(&msg, &["7", "8"]);
        assert_eq!(status, DisplayMessageStatusEnumType::Accepted);
        assert!(status_info.is_none());
    }

    #[test]
    fn set_display_message_bound_to_an_unknown_transaction_is_unknown_transaction() {
        // A message scoped to a transaction the station is NOT running — an unknown
        // id, or an idle station (empty set) — is UnknownTransaction with a
        // populated statusInfo, and is not installed.
        let msg = display_message(3, Some("9"));
        let (status, status_info) = v201_set_display_message_status(&msg, &["7", "8"]);
        assert_eq!(status, DisplayMessageStatusEnumType::UnknownTransaction);
        assert_eq!(status_info.unwrap().reason_code, "NoTransaction");

        let (idle_status, _) = v201_set_display_message_status(&msg, &[]);
        assert_eq!(
            idle_status,
            DisplayMessageStatusEnumType::UnknownTransaction,
            "an idle station cannot scope a transaction-bound message"
        );
    }

    #[test]
    fn set_display_message_matches_transaction_id_on_exact_string_never_parses() {
        // Trust boundary: the bound transactionId is matched by exact string
        // equality over the station-minted decimal id, not a numeric parse. A
        // non-canonical spelling of a live id misses → UnknownTransaction, never a
        // panic or a coercion.
        let padded = display_message(4, Some("07"));
        assert_eq!(
            v201_set_display_message_status(&padded, &["7"]).0,
            DisplayMessageStatusEnumType::UnknownTransaction,
            "a non-canonical spelling of a live id must not match"
        );
        let spaced = display_message(5, Some(" 7 "));
        assert_eq!(
            v201_set_display_message_status(&spaced, &["7"]).0,
            DisplayMessageStatusEnumType::UnknownTransaction
        );
    }

    #[test]
    fn built_set_display_message_responses_are_schema_valid() {
        // Every built response — each DisplayMessageStatusEnumType crossed with
        // with/without a statusInfo — satisfies the bundled OCPP 2.0.1
        // SetDisplayMessage response JSON Schema.
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "NoTransaction".to_string(),
            additional_info: Some("bound to a transaction the station is not running".to_string()),
            custom_data: None,
        };
        for status in [
            DisplayMessageStatusEnumType::Accepted,
            DisplayMessageStatusEnumType::NotSupportedMessageFormat,
            DisplayMessageStatusEnumType::Rejected,
            DisplayMessageStatusEnumType::NotSupportedPriority,
            DisplayMessageStatusEnumType::NotSupportedState,
            DisplayMessageStatusEnumType::UnknownTransaction,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_set_display_message_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("SetDisplayMessage", &payload)
                        .is_ok(),
                    "built {status:?} SetDisplayMessageResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    // --- GetDisplayMessages (v201, #508) -----------------------------------

    /// A `MessageInfoType` with the given `id`, `priority`, and (optional)
    /// `state`, so the selector tests can tell messages apart on every selector.
    fn display_message_pss(
        id: i32,
        priority: MessagePriorityEnumType,
        state: Option<MessageStateEnumType>,
    ) -> MessageInfoType {
        let mut msg = display_message(id, None);
        msg.priority = priority;
        msg.state = state;
        msg
    }

    #[test]
    fn get_display_messages_empty_selector_matches_every_installed_message() {
        let installed = vec![
            display_message_pss(1, MessagePriorityEnumType::NormalCycle, None),
            display_message_pss(2, MessagePriorityEnumType::AlwaysFront, None),
        ];
        // No id/priority/state selector → every installed message matches.
        let mut got = v201_get_display_messages_matches(None, None, None, &installed);
        got.sort_by_key(|m| m.id);
        assert_eq!(got, installed);
        // An empty store matches nothing, whatever the selector.
        assert!(v201_get_display_messages_matches(None, None, None, &[]).is_empty());
    }

    #[test]
    fn get_display_messages_id_selector_filters_and_tolerates_unknown_ids() {
        let installed = vec![
            display_message_pss(1, MessagePriorityEnumType::NormalCycle, None),
            display_message_pss(2, MessagePriorityEnumType::NormalCycle, None),
            display_message_pss(3, MessagePriorityEnumType::NormalCycle, None),
        ];
        // Only the listed ids match; a present-but-unknown id (99) simply misses.
        let ids = [2, 99];
        let mut got = v201_get_display_messages_matches(Some(&ids), None, None, &installed);
        got.sort_by_key(|m| m.id);
        assert_eq!(got.iter().map(|m| m.id).collect::<Vec<_>>(), vec![2]);
        // An id list naming nothing installed matches nothing — never a panic on a
        // CSMS-supplied id (i32::MIN / MAX / negative all just miss).
        assert!(v201_get_display_messages_matches(
            Some(&[i32::MIN, i32::MAX, -1]),
            None,
            None,
            &installed
        )
        .is_empty());
    }

    #[test]
    fn get_display_messages_priority_selector_filters_on_exact_priority() {
        let installed = vec![
            display_message_pss(1, MessagePriorityEnumType::AlwaysFront, None),
            display_message_pss(2, MessagePriorityEnumType::NormalCycle, None),
            display_message_pss(3, MessagePriorityEnumType::AlwaysFront, None),
        ];
        let mut got = v201_get_display_messages_matches(
            None,
            Some(MessagePriorityEnumType::AlwaysFront),
            None,
            &installed,
        );
        got.sort_by_key(|m| m.id);
        assert_eq!(got.iter().map(|m| m.id).collect::<Vec<_>>(), vec![1, 3]);
        // A priority naming no installed message matches nothing.
        assert!(v201_get_display_messages_matches(
            None,
            Some(MessagePriorityEnumType::InFront),
            None,
            &installed
        )
        .is_empty());
    }

    #[test]
    fn get_display_messages_state_selector_includes_stateless_messages() {
        // A stateless message shows in EVERY station state, so a state query must
        // include it; a message with a different explicit state is excluded.
        let installed = vec![
            display_message_pss(1, MessagePriorityEnumType::NormalCycle, None), // any state
            display_message_pss(
                2,
                MessagePriorityEnumType::NormalCycle,
                Some(MessageStateEnumType::Charging),
            ),
            display_message_pss(
                3,
                MessagePriorityEnumType::NormalCycle,
                Some(MessageStateEnumType::Idle),
            ),
        ];
        // state=Charging → the stateless (1) and the Charging (2) match, not Idle (3).
        let mut got = v201_get_display_messages_matches(
            None,
            None,
            Some(MessageStateEnumType::Charging),
            &installed,
        );
        got.sort_by_key(|m| m.id);
        assert_eq!(got.iter().map(|m| m.id).collect::<Vec<_>>(), vec![1, 2]);
        // state=Idle → the stateless (1) and the Idle (3) match, not Charging (2).
        let mut got = v201_get_display_messages_matches(
            None,
            None,
            Some(MessageStateEnumType::Idle),
            &installed,
        );
        got.sort_by_key(|m| m.id);
        assert_eq!(got.iter().map(|m| m.id).collect::<Vec<_>>(), vec![1, 3]);
    }

    #[test]
    fn get_display_messages_selectors_and_conjunctively() {
        let installed = vec![
            display_message_pss(
                1,
                MessagePriorityEnumType::AlwaysFront,
                Some(MessageStateEnumType::Charging),
            ),
            display_message_pss(
                2,
                MessagePriorityEnumType::AlwaysFront,
                Some(MessageStateEnumType::Idle),
            ),
            display_message_pss(
                3,
                MessagePriorityEnumType::NormalCycle,
                Some(MessageStateEnumType::Charging),
            ),
        ];
        // id ∈ {1,2,3} AND priority=AlwaysFront AND state=Charging → only 1.
        let got = v201_get_display_messages_matches(
            Some(&[1, 2, 3]),
            Some(MessagePriorityEnumType::AlwaysFront),
            Some(MessageStateEnumType::Charging),
            &installed,
        );
        assert_eq!(got.iter().map(|m| m.id).collect::<Vec<_>>(), vec![1]);
        // Tighten the id filter to exclude 1 → the AND matches nothing.
        assert!(v201_get_display_messages_matches(
            Some(&[2, 3]),
            Some(MessagePriorityEnumType::AlwaysFront),
            Some(MessageStateEnumType::Charging),
            &installed
        )
        .is_empty());
    }

    #[test]
    fn get_display_messages_response_maps_matched_to_accepted_or_unknown() {
        assert_eq!(
            v201_get_display_messages_response(true).status,
            GetDisplayMessagesStatusEnumType::Accepted
        );
        assert_eq!(
            v201_get_display_messages_response(false).status,
            GetDisplayMessagesStatusEnumType::Unknown
        );
        // No message data rides on the synchronous acknowledgement.
        let resp = v201_get_display_messages_response(true);
        assert!(resp.status_info.is_none());
        assert!(resp.custom_data.is_none());
    }

    #[test]
    fn notify_display_messages_pages_are_ordered_and_tbc_flagged() {
        // Snapshot order is a HashMap walk; the pages must come out sorted by id
        // with `tbc` set on every page but the last (which leaves it absent).
        let matched = vec![
            display_message_pss(30, MessagePriorityEnumType::NormalCycle, None),
            display_message_pss(10, MessagePriorityEnumType::NormalCycle, None),
            display_message_pss(20, MessagePriorityEnumType::NormalCycle, None),
        ];
        let pages = v201_notify_display_messages_pages(77, &matched);
        assert_eq!(pages.len(), 3, "one page per matched message");
        // Ascending id order, one message per page, requestId echoed on every page.
        let ids: Vec<i32> = pages
            .iter()
            .map(|p| p.message_info.as_ref().unwrap()[0].id)
            .collect();
        assert_eq!(ids, vec![10, 20, 30]);
        assert!(pages.iter().all(|p| p.request_id == 77));
        assert!(pages
            .iter()
            .all(|p| p.message_info.as_ref().unwrap().len() == 1));
        // tbc set on all but the last.
        assert_eq!(pages[0].tbc, Some(true));
        assert_eq!(pages[1].tbc, Some(true));
        assert_eq!(
            pages[2].tbc, None,
            "the final page leaves tbc absent (= false)"
        );
    }

    #[test]
    fn notify_display_messages_pages_empty_match_builds_no_pages() {
        assert!(v201_notify_display_messages_pages(1, &[]).is_empty());
    }

    #[test]
    fn built_get_display_messages_payloads_are_schema_valid() {
        // Both synchronous answers, and a streamed NotifyDisplayMessages page,
        // satisfy the bundled OCPP 2.0.1 JSON Schemas.
        let validator = SchemaValidator::v201();
        for matched in [true, false] {
            let resp = v201_get_display_messages_response(matched);
            validator
                .validate_call_result("GetDisplayMessages", &serde_json::to_value(&resp).unwrap())
                .expect("GetDisplayMessages response is schema-valid");
        }
        let matched = vec![
            display_message_pss(
                1,
                MessagePriorityEnumType::AlwaysFront,
                Some(MessageStateEnumType::Charging),
            ),
            display_message_pss(2, MessagePriorityEnumType::NormalCycle, None),
        ];
        for page in v201_notify_display_messages_pages(9, &matched) {
            validator
                .validate_call(
                    "NotifyDisplayMessages",
                    &serde_json::to_value(&page).unwrap(),
                )
                .expect("NotifyDisplayMessages CALL is schema-valid");
        }
    }

    // --- NotifyCustomerInformation report pages (v201, #537) ----------------

    #[test]
    fn notify_customer_information_pages_number_and_flag_tbc() {
        // seqNo runs from 0 in input order; tbc is set on every page but the
        // last (which leaves it absent = false); requestId + generatedAt echo on
        // every page.
        let data = ["page A", "page B", "page C"];
        let pages = v201_notify_customer_information_pages(42, "2022-01-01T10:00:00Z", &data);
        assert_eq!(pages.len(), 3, "one page per data chunk");

        let seq_nos: Vec<i32> = pages.iter().map(|p| p.seq_no).collect();
        assert_eq!(seq_nos, vec![0, 1, 2], "seqNo runs from 0 in order");
        assert!(pages.iter().all(|p| p.request_id == 42));
        assert!(pages
            .iter()
            .all(|p| p.generated_at == "2022-01-01T10:00:00Z"));

        assert_eq!(pages[0].tbc, Some(true));
        assert_eq!(pages[1].tbc, Some(true));
        assert_eq!(
            pages[2].tbc, None,
            "the final page leaves tbc absent (= false)"
        );
    }

    #[test]
    fn notify_customer_information_single_page_has_no_tbc() {
        // A one-page report is its own terminal page — tbc absent, seqNo 0.
        let pages = v201_notify_customer_information_pages(1, "2022-01-01T10:00:00Z", &["only"]);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].seq_no, 0);
        assert_eq!(pages[0].tbc, None, "a lone page is terminal");
    }

    #[test]
    fn notify_customer_information_empty_input_builds_no_pages() {
        assert!(v201_notify_customer_information_pages(1, "2022-01-01T10:00:00Z", &[]).is_empty());
    }

    #[test]
    fn notify_customer_information_extreme_request_id_does_not_panic() {
        // requestId is echoed, never parsed — extremes flow into every page safely.
        for request_id in [i32::MIN, i32::MAX] {
            let pages = v201_notify_customer_information_pages(
                request_id,
                "2022-01-01T10:00:00Z",
                V201_SIMULATED_CUSTOMER_INFORMATION_PAGES,
            );
            assert!(pages.iter().all(|p| p.request_id == request_id));
        }
    }

    #[test]
    fn simulated_customer_information_pages_are_multi_page_and_within_schema_bounds() {
        // The simulator body is >1 page (so paging is exercised) and every page
        // is well under the schema's maxLength 512.
        assert!(
            V201_SIMULATED_CUSTOMER_INFORMATION_PAGES.len() >= 2,
            "the simulated report is multi-page so tbc paging is observable"
        );
        assert!(
            V201_SIMULATED_CUSTOMER_INFORMATION_PAGES
                .iter()
                .all(|page| page.chars().count() <= 512),
            "each simulated page fits the NotifyCustomerInformation data maxLength"
        );
    }

    #[test]
    fn built_notify_customer_information_pages_are_schema_valid() {
        // Every page of the simulated report — non-terminal (tbc: true) and
        // terminal (tbc absent) alike — satisfies the bundled OCPP 2.0.1 schema.
        let validator = SchemaValidator::v201();
        let pages = v201_notify_customer_information_pages(
            9,
            "2022-01-01T10:00:00Z",
            V201_SIMULATED_CUSTOMER_INFORMATION_PAGES,
        );
        for page in &pages {
            validator
                .validate_call(
                    "NotifyCustomerInformation",
                    &serde_json::to_value(page).unwrap(),
                )
                .expect("NotifyCustomerInformation CALL is schema-valid");
        }
    }

    // --- PublishFirmwareStatusNotification (v201, #540) ---------------------

    #[test]
    fn publish_firmware_status_notification_carries_status_and_correlating_request_id() {
        // The intermediate lifecycle states carry no location; requestId always
        // rides along so the CSMS can correlate the stream to its PublishFirmware.
        let note = v201_publish_firmware_status_notification(
            PublishFirmwareStatusEnumType::Downloading,
            None,
            42,
        );
        assert_eq!(note.status, PublishFirmwareStatusEnumType::Downloading);
        assert_eq!(note.request_id, Some(42));
        assert!(
            note.location.is_none(),
            "an intermediate state carries no location list"
        );
    }

    #[test]
    fn publish_firmware_status_notification_published_carries_the_location_list() {
        // The terminal Published state advertises the URIs the image can be
        // pulled from — the simulator-supplied location list, forwarded verbatim.
        let locations: Vec<String> = V201_SIMULATED_PUBLISH_FIRMWARE_LOCATIONS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let note = v201_publish_firmware_status_notification(
            PublishFirmwareStatusEnumType::Published,
            Some(locations.clone()),
            7,
        );
        assert_eq!(note.status, PublishFirmwareStatusEnumType::Published);
        assert_eq!(note.location, Some(locations));
        assert_eq!(note.request_id, Some(7));
    }

    #[test]
    fn publish_firmware_status_notification_extreme_request_id_does_not_panic() {
        // requestId is echoed, never parsed — extremes flow into the message safely.
        for request_id in [i32::MIN, i32::MAX] {
            let note = v201_publish_firmware_status_notification(
                PublishFirmwareStatusEnumType::Idle,
                None,
                request_id,
            );
            assert_eq!(note.request_id, Some(request_id));
        }
    }

    #[test]
    fn simulated_publish_firmware_locations_are_non_empty_and_within_schema_bounds() {
        // The simulated location list advertises at least one URI, each well
        // under the schema's per-URI maxLength 512.
        assert!(
            !V201_SIMULATED_PUBLISH_FIRMWARE_LOCATIONS.is_empty(),
            "the Published state advertises at least one download URI"
        );
        assert!(
            V201_SIMULATED_PUBLISH_FIRMWARE_LOCATIONS
                .iter()
                .all(|uri| uri.chars().count() <= 512),
            "each simulated location fits the schema's per-URI maxLength"
        );
    }

    #[test]
    fn built_publish_firmware_status_notifications_are_schema_valid() {
        // Every status the simulated progression emits — the intermediate states
        // (no location) and the terminal Published (with the location list) —
        // satisfies the bundled OCPP 2.0.1 schema.
        let validator = SchemaValidator::v201();
        let locations: Vec<String> = V201_SIMULATED_PUBLISH_FIRMWARE_LOCATIONS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let notes = [
            v201_publish_firmware_status_notification(PublishFirmwareStatusEnumType::Idle, None, 9),
            v201_publish_firmware_status_notification(
                PublishFirmwareStatusEnumType::DownloadScheduled,
                None,
                9,
            ),
            v201_publish_firmware_status_notification(
                PublishFirmwareStatusEnumType::Downloading,
                None,
                9,
            ),
            v201_publish_firmware_status_notification(
                PublishFirmwareStatusEnumType::Downloaded,
                None,
                9,
            ),
            v201_publish_firmware_status_notification(
                PublishFirmwareStatusEnumType::Published,
                Some(locations),
                9,
            ),
        ];
        for note in &notes {
            validator
                .validate_call(
                    "PublishFirmwareStatusNotification",
                    &serde_json::to_value(note).unwrap(),
                )
                .expect("PublishFirmwareStatusNotification CALL is schema-valid");
        }
    }

    // --- ClearDisplayMessage (v201, #509) ----------------------------------

    #[test]
    fn clear_display_message_response_maps_removed_to_accepted_and_missing_to_unknown() {
        assert_eq!(
            v201_clear_display_message_response(true).status,
            ClearMessageStatusEnumType::Accepted
        );
        assert_eq!(
            v201_clear_display_message_response(false).status,
            ClearMessageStatusEnumType::Unknown
        );
        // The builder carries no detail on either arm.
        let resp = v201_clear_display_message_response(true);
        assert!(resp.status_info.is_none());
        assert!(resp.custom_data.is_none());
    }

    /// Wire fidelity: both built `ClearDisplayMessage.conf` values satisfy the
    /// bundled OCPP 2.0.1 `ClearDisplayMessageResponse` JSON Schema.
    #[test]
    fn built_clear_display_message_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        for removed in [true, false] {
            let resp = v201_clear_display_message_response(removed);
            let payload = serde_json::to_value(&resp).unwrap();
            validator
                .validate_call_result("ClearDisplayMessage", &payload)
                .unwrap_or_else(|e| {
                    panic!("built ClearDisplayMessageResponse (removed={removed}) should be schema-valid: {e}")
                });
        }
    }

    #[test]
    fn built_cost_updated_response_is_empty_and_schema_valid() {
        // The acknowledgement carries no fields, so it serializes to `{}` and
        // satisfies the bundled OCPP 2.0.1 CostUpdated response JSON Schema.
        let payload = serde_json::to_value(v201_cost_updated_response()).unwrap();
        assert_eq!(payload, serde_json::json!({}));
        assert!(
            SchemaValidator::v201()
                .validate_call_result("CostUpdated", &payload)
                .is_ok(),
            "built CostUpdatedResponse should be schema-valid, got: {payload}"
        );
    }

    // --- InstallCertificate (v201) decision + response builder (Issue #518) ---

    /// A minimal but structurally-valid PEM certificate, armor + one body line.
    const SAMPLE_PEM: &str = "-----BEGIN CERTIFICATE-----\nMIIBkTCB+w==\n-----END CERTIFICATE-----";

    #[test]
    fn install_certificate_accepts_a_pem_shaped_certificate() {
        assert_eq!(
            v201_install_certificate_status(SAMPLE_PEM),
            InstallCertificateStatusEnumType::Accepted
        );
        // Surrounding whitespace does not change the decision.
        assert_eq!(
            v201_install_certificate_status(&format!("  \n{SAMPLE_PEM}\n  ")),
            InstallCertificateStatusEnumType::Accepted
        );
    }

    #[test]
    fn install_certificate_rejects_empty_or_non_pem_input() {
        // Nothing to install.
        for empty in ["", "   ", "\n\t "] {
            assert_eq!(
                v201_install_certificate_status(empty),
                InstallCertificateStatusEnumType::Rejected,
                "an empty/blank certificate is refused up front"
            );
        }
        // Non-empty but not PEM-armored at all → still refused, never a panic on
        // arbitrary CSMS input.
        for garbage in [
            "not a certificate",
            "-----BEGIN CERTIFICATE-----",
            "MIIBkTCB+w==",
        ] {
            assert_eq!(
                v201_install_certificate_status(garbage),
                InstallCertificateStatusEnumType::Rejected,
                "a non-PEM-armored string is refused: {garbage:?}"
            );
        }
    }

    #[test]
    fn install_certificate_fails_on_a_pem_armored_but_empty_body() {
        // Recognized as a certificate (both markers present) but no key material
        // between them → attempted, could not complete.
        let empty_body = "-----BEGIN CERTIFICATE-----\n\n-----END CERTIFICATE-----";
        assert_eq!(
            v201_install_certificate_status(empty_body),
            InstallCertificateStatusEnumType::Failed
        );
        // Whitespace-only body is likewise unusable.
        let blank_body = "-----BEGIN CERTIFICATE-----\n   \n-----END CERTIFICATE-----";
        assert_eq!(
            v201_install_certificate_status(blank_body),
            InstallCertificateStatusEnumType::Failed
        );
    }

    #[test]
    fn install_certificate_does_not_panic_on_hostile_input() {
        // Very long and control-char-laden strings are inspected, never parsed.
        let long = "-".repeat(100_000);
        let _ = v201_install_certificate_status(&long);
        let _ = v201_install_certificate_status("\0\u{1}\u{2}-----BEGIN-----\u{7f}");
        // A body made only of the armor prefix on every line has no key material.
        let all_armor = "-----BEGIN CERTIFICATE-----\n-----X-----\n-----END CERTIFICATE-----";
        assert_eq!(
            v201_install_certificate_status(all_armor),
            InstallCertificateStatusEnumType::Failed
        );
    }

    #[test]
    fn built_install_certificate_responses_are_schema_valid() {
        // All three wire statuses, with and without a statusInfo, satisfy the
        // bundled OCPP 2.0.1 InstallCertificate response JSON Schema.
        let validator = SchemaValidator::v201();
        for status in [
            InstallCertificateStatusEnumType::Accepted,
            InstallCertificateStatusEnumType::Rejected,
            InstallCertificateStatusEnumType::Failed,
        ] {
            let info = (!matches!(status, InstallCertificateStatusEnumType::Accepted)).then(|| {
                StatusInfoType {
                    reason_code: "Unusable".to_string(),
                    additional_info: Some("simulated".to_string()),
                    custom_data: None,
                }
            });
            let resp = v201_install_certificate_response(status, info);
            validator
                .validate_call_result("InstallCertificate", &serde_json::to_value(&resp).unwrap())
                .expect("built InstallCertificate response is schema-valid");
        }
    }

    // --- SignCertificate (v201) CP-initiated request builder (Issue #547) ---

    #[test]
    fn placeholder_csr_is_pem_certificate_request_shaped_and_within_the_schema_cap() {
        let csr = v201_placeholder_csr();
        assert!(
            csr.contains("-----BEGIN CERTIFICATE REQUEST-----"),
            "the placeholder CSR is PEM `CERTIFICATE REQUEST`-armored"
        );
        assert!(csr.contains("-----END CERTIFICATE REQUEST-----"));
        // A non-empty base64 body between the armor lines.
        let has_body = csr
            .lines()
            .map(str::trim)
            .any(|line| !line.is_empty() && !line.starts_with("-----"));
        assert!(
            has_body,
            "the placeholder CSR carries a body between the armor"
        );
        // The `SignCertificate` schema caps `csr` at 5500 chars.
        assert!(
            csr.len() <= 5500,
            "placeholder CSR ({} chars) stays under the schema's 5500 cap",
            csr.len()
        );
        // Deterministic: the simulator has no per-request key material to vary, so
        // successive requests are byte-identical.
        assert_eq!(csr, v201_placeholder_csr());
    }

    #[test]
    fn sign_certificate_request_carries_the_selected_certificate_type() {
        // Each explicit `certificateType` round-trips onto the request.
        for ct in [
            CertificateSigningUseEnumType::ChargingStationCertificate,
            CertificateSigningUseEnumType::V2GCertificate,
        ] {
            let req = v201_sign_certificate_request(Some(ct));
            assert_eq!(req.certificate_type, Some(ct));
            assert_eq!(req.csr, v201_placeholder_csr());
        }
    }

    #[test]
    fn sign_certificate_request_omits_certificate_type_when_none() {
        let req = v201_sign_certificate_request(None);
        assert_eq!(
            req.certificate_type, None,
            "a None certificate_type is omitted — the spec reads that as both connections"
        );
        // `skip_serializing_if = Option::is_none` drops the key from the wire.
        let wire = serde_json::to_value(&req).expect("serialize SignCertificate.req");
        assert!(
            wire.get("certificateType").is_none(),
            "the omitted certificateType is absent on the wire, not null: {wire}"
        );
        assert!(
            wire.get("csr").is_some(),
            "csr is always present (required)"
        );
    }

    #[test]
    fn built_sign_certificate_requests_are_schema_valid() {
        // Both explicit certificate types and the omitted case satisfy the bundled
        // OCPP 2.0.1 SignCertificate request JSON Schema.
        let validator = SchemaValidator::v201();
        for ct in [
            Some(CertificateSigningUseEnumType::ChargingStationCertificate),
            Some(CertificateSigningUseEnumType::V2GCertificate),
            None,
        ] {
            let req = v201_sign_certificate_request(ct);
            validator
                .validate_call("SignCertificate", &serde_json::to_value(&req).unwrap())
                .unwrap_or_else(|e| {
                    panic!("built SignCertificate request (certificate_type={ct:?}) is schema-valid, got: {e}")
                });
        }
    }

    // --- Get15118EVCertificate (v201) CP-initiated request builder (Issue #558) ---

    #[test]
    fn get_15118_ev_certificate_request_threads_its_fields_verbatim() {
        // The builder is a pure pass-through: schema version, action, and the
        // opaque EXI request land on the request unchanged, with no vendor
        // extension added.
        for action in [
            CertificateActionEnumType::Install,
            CertificateActionEnumType::Update,
        ] {
            let req = v201_get_15118_ev_certificate_request(
                "urn:iso:15118:2:2013:MsgDef",
                action,
                "b64-exi-blob",
            );
            assert_eq!(req.iso15118_schema_version, "urn:iso:15118:2:2013:MsgDef");
            assert_eq!(req.action, action);
            assert_eq!(
                req.exi_request, "b64-exi-blob",
                "the EXI request is relayed verbatim — the station does not decode it"
            );
            assert_eq!(
                req.custom_data, None,
                "the builder adds no vendor extension"
            );
        }
    }

    #[test]
    fn built_get_15118_ev_certificate_requests_are_schema_valid() {
        // Both the Install and Update actions satisfy the bundled OCPP 2.0.1
        // Get15118EVCertificate request JSON Schema.
        let validator = SchemaValidator::v201();
        for action in [
            CertificateActionEnumType::Install,
            CertificateActionEnumType::Update,
        ] {
            let req = v201_get_15118_ev_certificate_request(
                "urn:iso:15118:2:2013:MsgDef",
                action,
                "b64-exi-CertificateInstallationReq",
            );
            let payload = serde_json::to_value(&req).unwrap();
            validator
                .validate_call("Get15118EVCertificate", &payload)
                .unwrap_or_else(|e| {
                    panic!("built Get15118EVCertificate request (action={action:?}) is schema-valid, got: {e}")
                });
            // All three fields are required, so each must be present on the wire.
            assert!(payload.get("iso15118SchemaVersion").is_some());
            assert!(payload.get("action").is_some());
            assert!(payload.get("exiRequest").is_some());
        }
    }

    // --- SetNetworkProfile (v201) decision + response builder (Issue #528) ---

    /// A minimal, well-formed `SetNetworkProfileRequest` for `slot`, whose
    /// profile names `csms_url`.
    fn set_network_profile_request(slot: i32, csms_url: &str) -> SetNetworkProfileRequest {
        use ocpp_types::v201::{
            NetworkConnectionProfileType, OCPPInterfaceEnumType, OCPPTransportEnumType,
            OCPPVersionEnumType,
        };
        SetNetworkProfileRequest {
            configuration_slot: slot,
            connection_data: NetworkConnectionProfileType {
                ocpp_version: OCPPVersionEnumType::Ocpp20,
                ocpp_transport: OCPPTransportEnumType::Json,
                ocpp_csms_url: csms_url.to_string(),
                message_timeout: 30,
                security_profile: 2,
                ocpp_interface: OCPPInterfaceEnumType::Wireless0,
                apn: None,
                vpn: None,
                custom_data: None,
            },
            custom_data: None,
        }
    }

    #[test]
    fn set_network_profile_accepts_a_profile_with_a_reachable_url() {
        assert_eq!(
            v201_set_network_profile_decision(&set_network_profile_request(
                1,
                "wss://csms.example.com/ocpp"
            )),
            SetNetworkProfileStatusEnumType::Accepted
        );
        // The configuration slot does not change the decision — extreme slots are
        // accepted just the same (the store keys on them; the decision does not).
        for slot in [i32::MIN, 0, i32::MAX] {
            assert_eq!(
                v201_set_network_profile_decision(&set_network_profile_request(
                    slot,
                    "wss://csms.example.com/ocpp"
                )),
                SetNetworkProfileStatusEnumType::Accepted,
                "slot {slot} is accepted"
            );
        }
    }

    #[test]
    fn set_network_profile_rejects_a_blank_csms_url() {
        // A profile that names no reachable CSMS is refused up front. These are
        // all schema-valid on the wire (ocppCsmsUrl has no minLength), so the
        // refusal is genuinely input-reachable, never a panic on CSMS input.
        for blank in ["", "   ", "\n\t "] {
            assert_eq!(
                v201_set_network_profile_decision(&set_network_profile_request(1, blank)),
                SetNetworkProfileStatusEnumType::Rejected,
                "a blank ocppCsmsUrl ({blank:?}) is refused"
            );
        }
    }

    #[test]
    fn built_set_network_profile_responses_are_schema_valid() {
        // All three wire statuses — including the unproduced `Failed` seam — with
        // and without a statusInfo, satisfy the bundled OCPP 2.0.1
        // SetNetworkProfile response JSON Schema.
        let validator = SchemaValidator::v201();
        for status in [
            SetNetworkProfileStatusEnumType::Accepted,
            SetNetworkProfileStatusEnumType::Rejected,
            SetNetworkProfileStatusEnumType::Failed,
        ] {
            for status_info in [
                None,
                Some(StatusInfoType {
                    reason_code: "InvalidProfile".to_string(),
                    additional_info: Some("simulated".to_string()),
                    custom_data: None,
                }),
            ] {
                let resp = v201_set_network_profile_response(status, status_info);
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("SetNetworkProfile", &payload)
                        .is_ok(),
                    "built {status:?} SetNetworkProfileResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    // --- GetLog (v201) decision + filename + response builder (Issue #517) ----

    /// A schema-shaped `GetLogRequest` for `(log_type, request_id)`, with an
    /// arbitrary but valid remote-location URI. The retry hints are left absent —
    /// the decision ignores them.
    fn get_log_request(log_type: LogEnumType, request_id: i32) -> GetLogRequest {
        GetLogRequest {
            log: ocpp_types::v201::LogParametersType {
                remote_location: "https://logs.example.test/upload".to_string(),
                oldest_timestamp: None,
                latest_timestamp: None,
                custom_data: None,
            },
            log_type,
            request_id,
            retries: None,
            retry_interval: None,
            custom_data: None,
        }
    }

    #[test]
    fn get_log_accepts_and_names_a_file_when_idle() {
        // No upload in flight: both log kinds are Accepted and carry a non-empty,
        // kind-tagged filename.
        for (log_type, prefix) in [
            (LogEnumType::DiagnosticsLog, "diagnostics_"),
            (LogEnumType::SecurityLog, "security_"),
        ] {
            let (status, filename) = v201_get_log_decision(&get_log_request(log_type, 42), None);
            assert_eq!(status, LogStatusEnumType::Accepted);
            let filename = filename.expect("an accepted GetLog names a file");
            assert!(!filename.is_empty(), "the filename must be non-empty");
            assert_eq!(filename, format!("{prefix}42.log"));
        }
    }

    #[test]
    fn get_log_is_idempotent_for_a_retry_of_the_in_flight_request() {
        // A GetLog carrying the SAME requestId as the in-flight upload is a retry:
        // idempotently Accepted (not AcceptedCanceled), same filename as the idle
        // answer — a retry must not report a spurious cancel.
        let req = get_log_request(LogEnumType::DiagnosticsLog, 7);
        let idle = v201_get_log_decision(&req, None);
        let retry = v201_get_log_decision(&req, Some(7));
        assert_eq!(retry.0, LogStatusEnumType::Accepted);
        assert_eq!(retry.1, idle.1, "a retry reports the same filename");
    }

    #[test]
    fn get_log_supersedes_a_different_in_flight_upload() {
        // A new requestId while a different upload is in flight supersedes it:
        // AcceptedCanceled, still naming the new file.
        let (status, filename) =
            v201_get_log_decision(&get_log_request(LogEnumType::SecurityLog, 9), Some(8));
        assert_eq!(status, LogStatusEnumType::AcceptedCanceled);
        assert_eq!(
            filename,
            Some("security_9.log".to_string()),
            "a supersede still names the file it will upload"
        );
    }

    #[test]
    fn get_log_filename_is_deterministic_and_bounded() {
        // Deterministic: the same (kind, id) always yields the same name.
        assert_eq!(
            v201_log_filename(LogEnumType::DiagnosticsLog, 3),
            v201_log_filename(LogEnumType::DiagnosticsLog, 3)
        );
        // Distinct per kind and per id — no two requests collide on a name.
        assert_ne!(
            v201_log_filename(LogEnumType::DiagnosticsLog, 3),
            v201_log_filename(LogEnumType::SecurityLog, 3)
        );
        assert_ne!(
            v201_log_filename(LogEnumType::DiagnosticsLog, 3),
            v201_log_filename(LogEnumType::DiagnosticsLog, 4)
        );
        // Extreme wire request ids: non-empty, well under the schema's 255 bound,
        // no panic.
        for request_id in [i32::MIN, i32::MAX, 0, -1] {
            let name = v201_log_filename(LogEnumType::SecurityLog, request_id);
            assert!(!name.is_empty());
            assert!(name.len() <= 255, "filename {name:?} exceeds maxLength 255");
        }
    }

    #[test]
    fn built_get_log_responses_are_schema_valid() {
        // Every built response — each LogStatusEnumType crossed with with/without a
        // statusInfo and with/without a filename — satisfies the bundled OCPP 2.0.1
        // GetLog response JSON Schema.
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "Superseded".to_string(),
            additional_info: Some("a previous log upload was canceled".to_string()),
            custom_data: None,
        };
        for status in [
            LogStatusEnumType::Accepted,
            LogStatusEnumType::Rejected,
            LogStatusEnumType::AcceptedCanceled,
        ] {
            for status_info in [None, Some(info.clone())] {
                for filename in [None, Some("diagnostics_1.log".to_string())] {
                    let resp = v201_get_log_response(status, status_info.clone(), filename);
                    let payload = serde_json::to_value(&resp).unwrap();
                    assert!(
                        validator.validate_call_result("GetLog", &payload).is_ok(),
                        "built {status:?} GetLogResponse should be schema-valid, got: {payload}"
                    );
                }
            }
        }
    }

    // --- UpdateFirmware (v201) decision + response builder (Issue #532) --------

    /// A schema-shaped `UpdateFirmwareRequest` for `request_id`, carrying an
    /// arbitrary but valid image `location` and the given optional signing
    /// certificate. The retry hints are left absent — the decision ignores them.
    fn update_firmware_request(
        request_id: i32,
        signing_certificate: Option<&str>,
    ) -> UpdateFirmwareRequest {
        UpdateFirmwareRequest {
            request_id,
            firmware: ocpp_types::v201::FirmwareType {
                location: "https://firmware.example.test/image.bin".to_string(),
                retrieve_date_time: "2026-08-20T00:00:00Z".to_string(),
                install_date_time: None,
                signing_certificate: signing_certificate.map(str::to_string),
                signature: None,
                custom_data: None,
            },
            retries: None,
            retry_interval: None,
            custom_data: None,
        }
    }

    #[test]
    fn update_firmware_accepts_a_fresh_request_when_idle() {
        // Nothing in flight, no signing certificate → a fresh update is Accepted,
        // for ordinary and extreme request ids alike (the id is only compared).
        for request_id in [42, 0, -1, i32::MIN, i32::MAX] {
            let status =
                v201_update_firmware_decision(&update_firmware_request(request_id, None), None);
            assert_eq!(
                status,
                UpdateFirmwareStatusEnumType::Accepted,
                "an idle station accepts request {request_id}"
            );
        }
    }

    #[test]
    fn update_firmware_accepts_a_well_formed_signing_certificate() {
        // A present, usable PEM signing certificate does NOT take the
        // InvalidCertificate arm — the update is accepted on its in-flight merits.
        let status =
            v201_update_firmware_decision(&update_firmware_request(1, Some(SAMPLE_PEM)), None);
        assert_eq!(status, UpdateFirmwareStatusEnumType::Accepted);
        // Padded with surrounding whitespace is still a usable certificate.
        let padded = format!("  \n{SAMPLE_PEM}\n  ");
        let status =
            v201_update_firmware_decision(&update_firmware_request(1, Some(&padded)), None);
        assert_eq!(status, UpdateFirmwareStatusEnumType::Accepted);
    }

    #[test]
    fn update_firmware_is_idempotent_for_a_retry_of_the_in_flight_request() {
        // An UpdateFirmware carrying the SAME requestId as the in-flight update is
        // a retry: idempotently Accepted, never AcceptedCanceled — a retry must not
        // report a spurious cancel.
        let status = v201_update_firmware_decision(&update_firmware_request(7, None), Some(7));
        assert_eq!(status, UpdateFirmwareStatusEnumType::Accepted);
    }

    #[test]
    fn update_firmware_supersedes_a_different_in_flight_update() {
        // A new requestId while a different update is in flight supersedes it →
        // AcceptedCanceled.
        let status = v201_update_firmware_decision(&update_firmware_request(9, None), Some(8));
        assert_eq!(status, UpdateFirmwareStatusEnumType::AcceptedCanceled);
    }

    #[test]
    fn update_firmware_rejects_a_present_but_malformed_signing_certificate() {
        // A present-but-unusable signing certificate is refused up front with
        // InvalidCertificate: empty, whitespace-only, non-PEM garbage, and a
        // PEM-armored-but-empty-bodied certificate all take the arm. None of them
        // panics on hostile bytes.
        let empty_body = "-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----";
        for bad in [
            "",
            "   \n\t  ",
            "not a certificate at all",
            "\u{0}\u{1}\u{2}garbage\u{7f}",
            empty_body,
        ] {
            let status =
                v201_update_firmware_decision(&update_firmware_request(1, Some(bad)), None);
            assert_eq!(
                status,
                UpdateFirmwareStatusEnumType::InvalidCertificate,
                "a present-but-malformed certificate {bad:?} → InvalidCertificate"
            );
        }
    }

    #[test]
    fn update_firmware_certificate_check_precedes_the_in_flight_decision() {
        // The signing-certificate axis wins over the in-flight axis: a bad
        // certificate answers InvalidCertificate even when a *different* request is
        // in flight (where a good request would supersede → AcceptedCanceled). The
        // refusal must never be masked by the supersede path, so nothing gets
        // recorded for an untrusted image.
        let status =
            v201_update_firmware_decision(&update_firmware_request(9, Some("garbage")), Some(8));
        assert_eq!(status, UpdateFirmwareStatusEnumType::InvalidCertificate);
    }

    #[test]
    fn built_update_firmware_responses_are_schema_valid() {
        // Every built response — each UpdateFirmwareStatusEnumType crossed with
        // with/without a statusInfo — satisfies the bundled OCPP 2.0.1
        // UpdateFirmware response JSON Schema. This is where the two unproduced
        // seams (Rejected / RevokedCertificate) earn their wire + schema coverage.
        let validator = SchemaValidator::v201();
        let info = StatusInfoType {
            reason_code: "InvalidCertificate".to_string(),
            additional_info: Some("the firmware signing certificate is not usable".to_string()),
            custom_data: None,
        };
        for status in [
            UpdateFirmwareStatusEnumType::Accepted,
            UpdateFirmwareStatusEnumType::Rejected,
            UpdateFirmwareStatusEnumType::AcceptedCanceled,
            UpdateFirmwareStatusEnumType::InvalidCertificate,
            UpdateFirmwareStatusEnumType::RevokedCertificate,
        ] {
            for status_info in [None, Some(info.clone())] {
                let resp = v201_update_firmware_response(status, status_info.clone());
                let payload = serde_json::to_value(&resp).unwrap();
                assert!(
                    validator
                        .validate_call_result("UpdateFirmware", &payload)
                        .is_ok(),
                    "built {status:?} UpdateFirmwareResponse should be schema-valid, got: {payload}"
                );
            }
        }
    }

    // --- LogStatusNotification (v201) async upload flow (#526) -----------------

    #[test]
    fn log_upload_terminal_status_precedence() {
        use UploadLogStatusEnumType::{AcceptedCanceled, UploadFailure, Uploaded};
        // Owner + happy path → Uploaded.
        assert_eq!(
            v201_log_upload_terminal_status(false, false),
            Uploaded,
            "an owning upload on the happy path completes as Uploaded"
        );
        // Owner + fault injection → UploadFailure.
        assert_eq!(
            v201_log_upload_terminal_status(false, true),
            UploadFailure,
            "an owning upload under fault injection reports UploadFailure"
        );
        // Superseded wins over should_fail — a canceled upload never "fails".
        assert_eq!(
            v201_log_upload_terminal_status(true, false),
            AcceptedCanceled
        );
        assert_eq!(
            v201_log_upload_terminal_status(true, true),
            AcceptedCanceled,
            "supersede takes precedence over fault injection"
        );
        // The opening status is always Uploading.
        assert_eq!(
            V201_LOG_UPLOAD_IN_PROGRESS,
            UploadLogStatusEnumType::Uploading
        );
    }

    #[test]
    fn log_status_notification_carries_the_request_id() {
        let req = v201_log_status_notification(UploadLogStatusEnumType::Uploading, 42);
        assert_eq!(req.status, UploadLogStatusEnumType::Uploading);
        assert_eq!(
            req.request_id,
            Some(42),
            "the async report is correlated by the GetLog requestId"
        );
        assert!(req.custom_data.is_none());
    }

    #[test]
    fn built_log_status_notifications_are_schema_valid() {
        // Every status this flow can emit — the Uploading opener, the three
        // terminal outcomes, and every remaining ported UploadLogStatusEnumType
        // value — is schema-valid as a LogStatusNotification.req, including at
        // extreme requestIds (no wire value can produce an invalid payload).
        let validator = SchemaValidator::v201();
        for status in [
            UploadLogStatusEnumType::Uploading,
            UploadLogStatusEnumType::Uploaded,
            UploadLogStatusEnumType::UploadFailure,
            UploadLogStatusEnumType::AcceptedCanceled,
            UploadLogStatusEnumType::Idle,
            UploadLogStatusEnumType::BadMessage,
            UploadLogStatusEnumType::NotSupportedOperation,
            UploadLogStatusEnumType::PermissionDenied,
        ] {
            for request_id in [0, 1, -1, i32::MIN, i32::MAX] {
                let req = v201_log_status_notification(status, request_id);
                let payload = serde_json::to_value(&req).unwrap();
                assert!(
                    validator
                        .validate_call("LogStatusNotification", &payload)
                        .is_ok(),
                    "built {status:?} LogStatusNotification.req (requestId {request_id}) \
                     should be schema-valid, got: {payload}"
                );
            }
        }
    }

    // --- FirmwareStatusNotification (v201) async update flow (#534) -------------

    #[test]
    fn firmware_status_notification_carries_the_request_id() {
        let req = v201_firmware_status_notification(FirmwareStatusEnumType::Downloading, 42);
        assert_eq!(req.status, FirmwareStatusEnumType::Downloading);
        assert_eq!(
            req.request_id,
            Some(42),
            "the async report is correlated by the UpdateFirmware requestId"
        );
        assert!(req.custom_data.is_none());
    }

    #[test]
    fn built_firmware_status_notifications_are_schema_valid() {
        // Every status this flow can emit — the Downloading/Downloaded/Installing
        // progression, the Installed terminal, the DownloadFailed/InstallationFailed
        // fault terminals, and every remaining ported FirmwareStatusEnumType value —
        // is schema-valid as a FirmwareStatusNotification.req, including at extreme
        // requestIds (no wire value can produce an invalid payload).
        let validator = SchemaValidator::v201();
        for status in [
            FirmwareStatusEnumType::Downloading,
            FirmwareStatusEnumType::Downloaded,
            FirmwareStatusEnumType::Installing,
            FirmwareStatusEnumType::Installed,
            FirmwareStatusEnumType::DownloadFailed,
            FirmwareStatusEnumType::InstallationFailed,
            FirmwareStatusEnumType::DownloadScheduled,
            FirmwareStatusEnumType::DownloadPaused,
            FirmwareStatusEnumType::Idle,
            FirmwareStatusEnumType::InstallRebooting,
            FirmwareStatusEnumType::InstallScheduled,
            FirmwareStatusEnumType::InstallVerificationFailed,
            FirmwareStatusEnumType::InvalidSignature,
            FirmwareStatusEnumType::SignatureVerified,
        ] {
            for request_id in [0, 1, -1, i32::MIN, i32::MAX] {
                let req = v201_firmware_status_notification(status, request_id);
                let payload = serde_json::to_value(&req).unwrap();
                assert!(
                    validator
                        .validate_call("FirmwareStatusNotification", &payload)
                        .is_ok(),
                    "built {status:?} FirmwareStatusNotification.req (requestId {request_id}) \
                     should be schema-valid, got: {payload}"
                );
            }
        }
    }

    // --- GetInstalledCertificateIds (v201) decision + response builder (#521) ---

    /// Two distinct installed anchors, as a `snapshot()` would present them.
    fn two_anchors() -> Vec<(InstallCertificateUseEnumType, String)> {
        vec![
            (
                InstallCertificateUseEnumType::CSMSRootCertificate,
                "pem-csms".to_string(),
            ),
            (
                InstallCertificateUseEnumType::V2GRootCertificate,
                "pem-v2g".to_string(),
            ),
        ]
    }

    #[test]
    fn certificate_hash_is_deterministic_and_distinct_per_anchor() {
        // Same (use, PEM) → identical hash: the property #522's delete-by-hash
        // round-trip relies on.
        let a = v201_certificate_hash_data(InstallCertificateUseEnumType::CSMSRootCertificate, "p");
        let b = v201_certificate_hash_data(InstallCertificateUseEnumType::CSMSRootCertificate, "p");
        assert_eq!(a, b);

        // A different use, or a different PEM (a rotation), changes the hash.
        let other_use =
            v201_certificate_hash_data(InstallCertificateUseEnumType::V2GRootCertificate, "p");
        let other_pem =
            v201_certificate_hash_data(InstallCertificateUseEnumType::CSMSRootCertificate, "q");
        assert_ne!(a, other_use, "distinct uses hash distinctly");
        assert_ne!(a, other_pem, "a rotation changes the hash");

        // The three hash fields are independently salted, so they differ from one
        // another, and every field satisfies its schema length bound.
        assert_ne!(a.issuer_name_hash, a.issuer_key_hash);
        assert_ne!(a.issuer_name_hash, a.serial_number);
        assert!(a.issuer_name_hash.len() <= 128 && a.issuer_key_hash.len() <= 128);
        assert!(a.serial_number.len() <= 40);
        assert_eq!(a.hash_algorithm, HashAlgorithmEnumType::Sha256);
    }

    #[test]
    fn get_installed_certificate_ids_enumerates_all_with_no_filter() {
        let snapshot = two_anchors();
        let chain = v201_get_installed_certificate_ids_matches(None, &snapshot);
        assert_eq!(chain.len(), 2, "a wildcard query returns every anchor");
        // Deterministically ordered by the fixed use order (V2G before CSMS).
        assert_eq!(
            chain[0].certificate_type,
            GetCertificateIdUseEnumType::V2GRootCertificate
        );
        assert_eq!(
            chain[1].certificate_type,
            GetCertificateIdUseEnumType::CSMSRootCertificate
        );
        // Each entry carries the placeholder hash for its (use, PEM); no child chain.
        assert_eq!(
            chain[1].certificate_hash_data,
            v201_certificate_hash_data(
                InstallCertificateUseEnumType::CSMSRootCertificate,
                "pem-csms"
            )
        );
        assert!(chain[0].child_certificate_hash_data.is_none());
    }

    #[test]
    fn get_installed_certificate_ids_applies_the_filter() {
        let snapshot = two_anchors();
        let filter = [GetCertificateIdUseEnumType::CSMSRootCertificate];
        let chain = v201_get_installed_certificate_ids_matches(Some(&filter), &snapshot);
        assert_eq!(chain.len(), 1, "only the named use is returned");
        assert_eq!(
            chain[0].certificate_type,
            GetCertificateIdUseEnumType::CSMSRootCertificate
        );
    }

    // --- CertificateSigned (v201) decision + response builder (Issue #516) ---

    /// A minimal but structurally-valid signed chain, armor + one body line.
    const SAMPLE_SIGNED_CHAIN: &str =
        "-----BEGIN CERTIFICATE-----\nMIIBkTCB+w==\n-----END CERTIFICATE-----";

    #[test]
    fn certificate_signed_accepts_a_pem_shaped_chain() {
        assert_eq!(
            v201_certificate_signed_status(SAMPLE_SIGNED_CHAIN),
            CertificateSignedStatusEnumType::Accepted
        );
        // Surrounding whitespace does not change the decision.
        assert_eq!(
            v201_certificate_signed_status(&format!("  \n{SAMPLE_SIGNED_CHAIN}\n  ")),
            CertificateSignedStatusEnumType::Accepted
        );
        // A multi-certificate chain (leaf + sub-CA) is likewise accepted.
        let chain = format!("{SAMPLE_SIGNED_CHAIN}\n{SAMPLE_SIGNED_CHAIN}");
        assert_eq!(
            v201_certificate_signed_status(&chain),
            CertificateSignedStatusEnumType::Accepted
        );
    }

    #[test]
    fn get_installed_certificate_ids_filter_matching_nothing_is_empty() {
        let snapshot = two_anchors();
        // A root that is not installed, and V2GCertificateChain which the store can
        // never hold — both match nothing.
        for filter in [
            vec![GetCertificateIdUseEnumType::ManufacturerRootCertificate],
            vec![GetCertificateIdUseEnumType::V2GCertificateChain],
        ] {
            assert!(
                v201_get_installed_certificate_ids_matches(Some(&filter), &snapshot).is_empty(),
                "a filter naming nothing installed matches nothing: {filter:?}"
            );
        }
    }

    #[test]
    fn certificate_signed_rejects_empty_blank_or_non_pem_input() {
        // Nothing to install.
        for empty in ["", "   ", "\n\t "] {
            assert_eq!(
                v201_certificate_signed_status(empty),
                CertificateSignedStatusEnumType::Rejected,
                "an empty/blank chain is refused"
            );
        }
        // Non-empty but not PEM-armored at all → refused, never a panic.
        for garbage in [
            "not a certificate",
            "-----BEGIN CERTIFICATE-----",
            "MIIBkTCB+w==",
        ] {
            assert_eq!(
                v201_certificate_signed_status(garbage),
                CertificateSignedStatusEnumType::Rejected,
                "a non-PEM-armored string is refused: {garbage:?}"
            );
        }
    }

    #[test]
    fn get_installed_certificate_ids_tolerates_duplicate_and_extreme_filters() {
        let snapshot = two_anchors();
        // Duplicate entries are membership, not iteration — no duplicate output, no
        // panic.
        let dup = [
            GetCertificateIdUseEnumType::V2GRootCertificate,
            GetCertificateIdUseEnumType::V2GRootCertificate,
        ];
        let chain = v201_get_installed_certificate_ids_matches(Some(&dup), &snapshot);
        assert_eq!(chain.len(), 1, "a duplicated filter value yields one entry");

        // All five categories at once returns exactly the two installed anchors.
        let all = [
            GetCertificateIdUseEnumType::V2GRootCertificate,
            GetCertificateIdUseEnumType::MORootCertificate,
            GetCertificateIdUseEnumType::CSMSRootCertificate,
            GetCertificateIdUseEnumType::V2GCertificateChain,
            GetCertificateIdUseEnumType::ManufacturerRootCertificate,
        ];
        assert_eq!(
            v201_get_installed_certificate_ids_matches(Some(&all), &snapshot).len(),
            2
        );

        // An empty store snapshots to an empty chain regardless of filter.
        assert!(v201_get_installed_certificate_ids_matches(None, &[]).is_empty());
    }

    #[test]
    fn get_installed_certificate_ids_response_maps_chain_to_status() {
        // A non-empty chain → Accepted, carrying the chain.
        let chain = v201_get_installed_certificate_ids_matches(None, &two_anchors());
        let resp = v201_get_installed_certificate_ids_response(chain);
        assert_eq!(resp.status, GetInstalledCertificateStatusEnumType::Accepted);
        assert_eq!(resp.certificate_hash_data_chain.map(|c| c.len()), Some(2));

        // An empty chain → NotFound, with no chain (absent, not an empty array).
        let resp = v201_get_installed_certificate_ids_response(vec![]);
        assert_eq!(resp.status, GetInstalledCertificateStatusEnumType::NotFound);
        assert!(resp.certificate_hash_data_chain.is_none());
    }

    #[test]
    fn built_get_installed_certificate_ids_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        // Accepted-with-chain and NotFound-without both serialize schema-valid.
        let accepted = v201_get_installed_certificate_ids_response(
            v201_get_installed_certificate_ids_matches(None, &two_anchors()),
        );
        let not_found = v201_get_installed_certificate_ids_response(vec![]);
        for resp in [accepted, not_found] {
            validator
                .validate_call_result(
                    "GetInstalledCertificateIds",
                    &serde_json::to_value(&resp).unwrap(),
                )
                .expect("built GetInstalledCertificateIds response is schema-valid");
        }
    }

    // --- DeleteCertificate (v201) decision + response builder (#522) ---

    #[test]
    fn certificate_hash_matches_on_identity_fields_only() {
        let base = v201_certificate_hash_data(
            InstallCertificateUseEnumType::CSMSRootCertificate,
            "pem-csms",
        );
        // Reflexive: a hash matches itself.
        assert!(v201_certificate_hash_matches(&base, &base));

        // `customData` is a vendor annotation, not identity — a request carrying it
        // still matches the derived (customData-less) hash of the same anchor.
        let with_custom = CertificateHashDataType {
            custom_data: Some(ocpp_types::v201::CustomDataType {
                vendor_id: "acme".to_string(),
                extra: serde_json::Map::new(),
            }),
            ..base.clone()
        };
        assert!(
            v201_certificate_hash_matches(&base, &with_custom),
            "customData is excluded from the identity compare"
        );

        // Changing any single identity field breaks the match.
        let mut algo = base.clone();
        algo.hash_algorithm = HashAlgorithmEnumType::Sha384;
        let mut name = base.clone();
        name.issuer_name_hash = "different".to_string();
        let mut key = base.clone();
        key.issuer_key_hash = "different".to_string();
        let mut serial = base.clone();
        serial.serial_number = "different".to_string();
        for differ in [algo, name, key, serial] {
            assert!(
                !v201_certificate_hash_matches(&base, &differ),
                "a differing identity field is not a match: {differ:?}"
            );
        }
    }

    #[test]
    fn delete_certificate_target_resolves_the_named_anchor() {
        let snapshot = two_anchors();
        // The hash `GetInstalledCertificateIds` reports for the CSMS anchor resolves
        // back to that anchor's use — the round-trip #522 guarantees.
        let csms_hash = v201_certificate_hash_data(
            InstallCertificateUseEnumType::CSMSRootCertificate,
            "pem-csms",
        );
        assert_eq!(
            v201_delete_certificate_target(&csms_hash, &snapshot),
            Some(InstallCertificateUseEnumType::CSMSRootCertificate)
        );
        // The V2G anchor's hash resolves to the V2G use, not the CSMS one — the
        // resolver picks the right entry among several.
        let v2g_hash = v201_certificate_hash_data(
            InstallCertificateUseEnumType::V2GRootCertificate,
            "pem-v2g",
        );
        assert_eq!(
            v201_delete_certificate_target(&v2g_hash, &snapshot),
            Some(InstallCertificateUseEnumType::V2GRootCertificate)
        );
    }

    #[test]
    fn delete_certificate_target_is_none_for_unknown_empty_or_hostile() {
        let snapshot = two_anchors();
        // A hash for the right use but a stale PEM (a rotation) resolves to nothing.
        let stale = v201_certificate_hash_data(
            InstallCertificateUseEnumType::CSMSRootCertificate,
            "pem-rotated",
        );
        assert_eq!(v201_delete_certificate_target(&stale, &snapshot), None);

        // An anchor that isn't installed matches nothing.
        let uninstalled =
            v201_certificate_hash_data(InstallCertificateUseEnumType::MORootCertificate, "pem-mo");
        assert_eq!(
            v201_delete_certificate_target(&uninstalled, &snapshot),
            None
        );

        // An empty store never matches.
        assert_eq!(v201_delete_certificate_target(&stale, &[]), None);

        // Hostile hash fields are only string-compared, never parsed — no panic,
        // no match.
        let hostile = CertificateHashDataType {
            hash_algorithm: HashAlgorithmEnumType::Sha512,
            issuer_name_hash: "\0\u{1}".repeat(10_000),
            issuer_key_hash: "-".repeat(100_000),
            serial_number: "💥".to_string(),
            custom_data: None,
        };
        assert_eq!(v201_delete_certificate_target(&hostile, &snapshot), None);
    }

    #[test]
    fn built_delete_certificate_responses_are_schema_valid() {
        let validator = SchemaValidator::v201();
        // All three wire values the handler can emit serialize schema-valid:
        // Accepted (no statusInfo), plus NotFound and Failed each with a reason.
        let cases = [
            (DeleteCertificateStatusEnumType::Accepted, None),
            (
                DeleteCertificateStatusEnumType::NotFound,
                Some(StatusInfoType {
                    reason_code: "NotFound".to_string(),
                    additional_info: Some("no installed certificate matches".to_string()),
                    custom_data: None,
                }),
            ),
            (
                DeleteCertificateStatusEnumType::Failed,
                Some(StatusInfoType {
                    reason_code: "RemovalFailed".to_string(),
                    additional_info: None,
                    custom_data: None,
                }),
            ),
        ];
        for (status, status_info) in cases {
            let resp = v201_delete_certificate_response(status, status_info);
            assert_eq!(resp.status, status);
            validator
                .validate_call_result("DeleteCertificate", &serde_json::to_value(&resp).unwrap())
                .expect("built DeleteCertificate response is schema-valid");
        }
    }

    #[test]
    fn certificate_signed_rejects_a_pem_armored_but_empty_body() {
        // Both markers present but no key material between them — the "recognized
        // but unusable" case that `InstallCertificate` reports as `Failed`
        // collapses into `Rejected` for `CertificateSigned`'s binary enum.
        let empty_body = "-----BEGIN CERTIFICATE-----\n\n-----END CERTIFICATE-----";
        assert_eq!(
            v201_certificate_signed_status(empty_body),
            CertificateSignedStatusEnumType::Rejected
        );
        let blank_body = "-----BEGIN CERTIFICATE-----\n   \n-----END CERTIFICATE-----";
        assert_eq!(
            v201_certificate_signed_status(blank_body),
            CertificateSignedStatusEnumType::Rejected
        );
    }

    #[test]
    fn certificate_signed_does_not_panic_on_hostile_input() {
        // Very long and control-char-laden strings are inspected, never parsed.
        let long = "-".repeat(100_000);
        let _ = v201_certificate_signed_status(&long);
        let _ = v201_certificate_signed_status("\0\u{1}\u{2}-----BEGIN-----\u{7f}");
        // A body made only of armor lines has no key material.
        let all_armor = "-----BEGIN CERTIFICATE-----\n-----X-----\n-----END CERTIFICATE-----";
        assert_eq!(
            v201_certificate_signed_status(all_armor),
            CertificateSignedStatusEnumType::Rejected
        );
    }

    #[test]
    fn built_certificate_signed_responses_are_schema_valid() {
        // Both wire statuses, with and without a statusInfo, satisfy the bundled
        // OCPP 2.0.1 CertificateSigned response JSON Schema.
        let validator = SchemaValidator::v201();
        for status in [
            CertificateSignedStatusEnumType::Accepted,
            CertificateSignedStatusEnumType::Rejected,
        ] {
            let info = matches!(status, CertificateSignedStatusEnumType::Rejected).then(|| {
                StatusInfoType {
                    reason_code: "InvalidChain".to_string(),
                    additional_info: Some("simulated".to_string()),
                    custom_data: None,
                }
            });
            let resp = v201_certificate_signed_response(status, info);
            validator
                .validate_call_result("CertificateSigned", &serde_json::to_value(&resp).unwrap())
                .expect("built CertificateSigned response is schema-valid");
        }
    }

    // --- CustomerInformation (v201) decision + response builder (Issue #530) ---

    fn sample_customer_hash() -> CertificateHashDataType {
        CertificateHashDataType {
            hash_algorithm: HashAlgorithmEnumType::Sha256,
            issuer_name_hash: "a1".to_string(),
            issuer_key_hash: "b2".to_string(),
            serial_number: "c3".to_string(),
            custom_data: None,
        }
    }

    fn sample_customer_id_token() -> ocpp_types::v201::IdTokenType {
        ocpp_types::v201::IdTokenType {
            id_token: "RFID-1234".to_string(),
            kind: ocpp_types::v201::IdTokenEnumType::Iso14443,
            additional_info: None,
            custom_data: None,
        }
    }

    /// Build a `CustomerInformationRequest` with a fixed `request_id`, the given
    /// actions, and the given selector presence.
    fn customer_information_req(
        report: bool,
        clear: bool,
        customer_certificate: Option<CertificateHashDataType>,
        id_token: Option<ocpp_types::v201::IdTokenType>,
        customer_identifier: Option<String>,
    ) -> CustomerInformationRequest {
        CustomerInformationRequest {
            request_id: 1,
            report,
            clear,
            customer_certificate,
            id_token,
            customer_identifier,
            custom_data: None,
        }
    }

    #[test]
    fn customer_information_accepts_a_selector_with_an_action() {
        // Each of the three selector kinds, crossed with report-only / clear-only /
        // both, names a customer and asks for something → Accepted.
        for selector_kind in 0..3 {
            for (report, clear) in [(true, false), (false, true), (true, true)] {
                let (cert, token, ident) = match selector_kind {
                    0 => (Some(sample_customer_hash()), None, None),
                    1 => (None, Some(sample_customer_id_token()), None),
                    _ => (None, None, Some("customer-abc".to_string())),
                };
                let req = customer_information_req(report, clear, cert, token, ident);
                assert_eq!(
                    v201_customer_information_decision(&req),
                    CustomerInformationStatusEnumType::Accepted,
                    "selector_kind={selector_kind}, report={report}, clear={clear}"
                );
            }
        }
    }

    #[test]
    fn customer_information_invalid_without_a_selector() {
        // No selector at all — even with both actions requested — has no customer to
        // act on, so it is malformed.
        for (report, clear) in [(true, false), (false, true), (true, true)] {
            let req = customer_information_req(report, clear, None, None, None);
            assert_eq!(
                v201_customer_information_decision(&req),
                CustomerInformationStatusEnumType::Invalid,
                "no selector is Invalid (report={report}, clear={clear})"
            );
        }
    }

    #[test]
    fn customer_information_invalid_without_an_action() {
        // A named customer but neither report nor clear: nothing to do.
        let one = customer_information_req(false, false, Some(sample_customer_hash()), None, None);
        assert_eq!(
            v201_customer_information_decision(&one),
            CustomerInformationStatusEnumType::Invalid
        );
        // Even all three selectors present, no action is still Invalid.
        let all = customer_information_req(
            false,
            false,
            Some(sample_customer_hash()),
            Some(sample_customer_id_token()),
            Some("customer-abc".to_string()),
        );
        assert_eq!(
            v201_customer_information_decision(&all),
            CustomerInformationStatusEnumType::Invalid
        );
    }

    #[test]
    fn customer_information_empty_request_is_invalid() {
        // No selector and no action: the fully-degenerate request.
        let req = customer_information_req(false, false, None, None, None);
        assert_eq!(
            v201_customer_information_decision(&req),
            CustomerInformationStatusEnumType::Invalid
        );
    }

    #[test]
    fn customer_information_extreme_request_id_never_panics() {
        // request_id is echoed only by the async follow-up, never read by the
        // decision — an actionable request is Accepted at every extreme without
        // panicking.
        for request_id in [0, 1, -1, i32::MIN, i32::MAX] {
            let mut req = customer_information_req(true, false, None, None, Some("c".to_string()));
            req.request_id = request_id;
            assert_eq!(
                v201_customer_information_decision(&req),
                CustomerInformationStatusEnumType::Accepted,
                "request_id={request_id} is Accepted"
            );
        }
    }

    #[test]
    fn built_customer_information_responses_are_schema_valid() {
        // Every status the wire can carry, with and without a statusInfo, satisfies
        // the bundled OCPP 2.0.1 CustomerInformation response JSON Schema (including
        // the unproduced `Rejected` seam).
        let validator = SchemaValidator::v201();
        for status in [
            CustomerInformationStatusEnumType::Accepted,
            CustomerInformationStatusEnumType::Rejected,
            CustomerInformationStatusEnumType::Invalid,
        ] {
            for status_info in [
                None,
                Some(StatusInfoType {
                    reason_code: "InvalidRequest".to_string(),
                    additional_info: Some(
                        "no usable customer selector, or neither report nor clear".to_string(),
                    ),
                    custom_data: None,
                }),
            ] {
                let resp = v201_customer_information_response(status, status_info);
                assert_eq!(resp.status, status);
                validator
                    .validate_call_result(
                        "CustomerInformation",
                        &serde_json::to_value(&resp).unwrap(),
                    )
                    .expect("built CustomerInformation response is schema-valid");
            }
        }
    }

    // --- PublishFirmware (v201) decision + response builder (Issue #538) ---

    /// A syntactically valid 32-char lowercase-hex MD5 digest for the fixtures.
    const SAMPLE_MD5: &str = "0123456789abcdef0123456789abcdef";

    /// Build a `PublishFirmwareRequest` with a fixed `request_id` and no retry
    /// tuning, varying only the two shape-relevant fields.
    fn publish_firmware_req(location: &str, checksum: &str) -> PublishFirmwareRequest {
        PublishFirmwareRequest {
            location: location.to_string(),
            retries: None,
            checksum: checksum.to_string(),
            request_id: 7,
            retry_interval: None,
            custom_data: None,
        }
    }

    #[test]
    fn publish_firmware_accepts_a_location_with_a_32_char_hex_checksum() {
        // A non-empty location plus a well-shaped 32-char hex checksum is
        // actionable → Accepted. Hex is case-insensitive (both nibble cases).
        for checksum in [
            SAMPLE_MD5,
            &SAMPLE_MD5.to_uppercase(),
            "ABCDEF0123456789abcdef0123456789",
        ] {
            let req = publish_firmware_req("https://fw.example/img.bin", checksum);
            assert_eq!(
                v201_publish_firmware_decision(&req),
                GenericStatusEnumType::Accepted,
                "checksum={checksum}"
            );
        }
        // retries / retryInterval are advisory and do not change the decision.
        let mut tuned = publish_firmware_req("ftp://host/fw", SAMPLE_MD5);
        tuned.retries = Some(3);
        tuned.retry_interval = Some(60);
        assert_eq!(
            v201_publish_firmware_decision(&tuned),
            GenericStatusEnumType::Accepted
        );
    }

    #[test]
    fn publish_firmware_rejects_an_empty_or_blank_location() {
        // No place to download from → nothing actionable, even with a good checksum.
        for location in ["", "   ", "\t\n"] {
            let req = publish_firmware_req(location, SAMPLE_MD5);
            assert_eq!(
                v201_publish_firmware_decision(&req),
                GenericStatusEnumType::Rejected,
                "location={location:?} is Rejected"
            );
        }
    }

    #[test]
    fn publish_firmware_rejects_a_misshaped_checksum() {
        // The checksum must be exactly 32 hex chars: too short, too long, or with a
        // non-hex character is not an MD5 digest we can verify against → Rejected.
        let bad = [
            "",                                  // empty
            "0123456789abcdef",                  // 16 chars — too short
            "0123456789abcdef0123456789abcde",   // 31 chars — one short
            "0123456789abcdef0123456789abcdef0", // 33 chars — one long
            "0123456789abcdef0123456789abcdeg",  // 32 chars but 'g' is not hex
            "0123456789abcdef0123456789abcde ",  // 32 chars but a trailing space
        ];
        for checksum in bad {
            let req = publish_firmware_req("https://fw.example/img.bin", checksum);
            assert_eq!(
                v201_publish_firmware_decision(&req),
                GenericStatusEnumType::Rejected,
                "checksum={checksum:?} is Rejected"
            );
        }
    }

    #[test]
    fn publish_firmware_does_not_panic_on_hostile_input() {
        // location / checksum are opaque CSMS input — inspected, never opened or
        // parsed — and request_id is not read by the decision, so no wire value
        // (very long, control chars, extreme id) can panic.
        let long = "x".repeat(4096);
        let _ = v201_publish_firmware_decision(&publish_firmware_req(&long, &long));
        let _ = v201_publish_firmware_decision(&publish_firmware_req(
            "\0\u{1}\u{2}://\u{7f}",
            "\0\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{7}\u{8}\u{9}\u{a}\u{b}\u{c}\u{d}\u{e}\u{f}0123456789abcdef",
        ));
        for request_id in [0, 1, -1, i32::MIN, i32::MAX] {
            let mut req = publish_firmware_req("https://fw.example/img.bin", SAMPLE_MD5);
            req.request_id = request_id;
            assert_eq!(
                v201_publish_firmware_decision(&req),
                GenericStatusEnumType::Accepted,
                "request_id={request_id} does not affect the decision"
            );
        }
    }

    #[test]
    fn built_publish_firmware_responses_are_schema_valid() {
        // Both statuses the wire can carry, with and without a statusInfo, satisfy
        // the bundled OCPP 2.0.1 PublishFirmware response JSON Schema.
        let validator = SchemaValidator::v201();
        for status in [
            GenericStatusEnumType::Accepted,
            GenericStatusEnumType::Rejected,
        ] {
            for status_info in [
                None,
                Some(StatusInfoType {
                    reason_code: "InvalidRequest".to_string(),
                    additional_info: Some(
                        "location empty or checksum not a 32-char hex digest".to_string(),
                    ),
                    custom_data: None,
                }),
            ] {
                let resp = v201_publish_firmware_response(status, status_info);
                assert_eq!(resp.status, status);
                validator
                    .validate_call_result("PublishFirmware", &serde_json::to_value(&resp).unwrap())
                    .expect("built PublishFirmware response is schema-valid");
            }
        }
    }

    // --- UnpublishFirmware (v201) decision + response builder (Issue #542) ---

    /// Build an `UnpublishFirmwareRequest` carrying the given checksum.
    fn unpublish_firmware_req(checksum: &str) -> UnpublishFirmwareRequest {
        UnpublishFirmwareRequest {
            checksum: checksum.to_string(),
            custom_data: None,
        }
    }

    #[test]
    fn unpublish_firmware_unpublishes_a_32_char_hex_checksum() {
        // A well-shaped MD5 digest — lower, upper, and mixed case are all hex.
        for checksum in [
            SAMPLE_MD5,
            &SAMPLE_MD5.to_uppercase(),
            "0123456789ABCDEFabcdef0123456789",
            "ffffffffffffffffffffffffffffffff",
            "00000000000000000000000000000000",
        ] {
            assert_eq!(
                v201_unpublish_firmware_decision(&unpublish_firmware_req(checksum)),
                UnpublishFirmwareStatusEnumType::Unpublished,
                "a 32-char hex checksum {checksum:?} unpublishes",
            );
        }
    }

    #[test]
    fn unpublish_firmware_reports_no_firmware_for_a_misshaped_checksum() {
        // A checksum that is not exactly 32 hex chars names no cached image: empty,
        // too short, 32 chars with a non-hex character, or hex but the wrong length.
        for checksum in [
            "",
            "   ",
            "abc",
            "0123456789abcdef0123456789abcde",  // 31 hex chars
            "0123456789abcdef0123456789abcdeg", // 32 chars, trailing non-hex 'g'
            "0123456789abcdef 123456789abcdef", // 32 chars, embedded space
            "not-a-hex-digest-not-a-hex-diges", // 32 chars, non-hex
        ] {
            assert_eq!(
                v201_unpublish_firmware_decision(&unpublish_firmware_req(checksum)),
                UnpublishFirmwareStatusEnumType::NoFirmware,
                "a mis-shaped checksum {checksum:?} reports NoFirmware",
            );
        }
    }

    #[test]
    fn unpublish_firmware_does_not_panic_on_hostile_input() {
        // Attacker-influenced checksum values must never panic; every one is only
        // shape-inspected, never opened, followed, parsed, or indexed. A schema-
        // over-length value (>32) would be refused before the handler, but a
        // decision on one still yields NoFirmware without panicking.
        for checksum in [
            &"f".repeat(4096),
            &"a\u{0}b".to_string(),
            &"héllo-wörld-with-non-ascii-chars".to_string(),
            &"\u{1F600}".repeat(32),
        ] {
            assert_eq!(
                v201_unpublish_firmware_decision(&unpublish_firmware_req(checksum)),
                UnpublishFirmwareStatusEnumType::NoFirmware,
                "hostile checksum {checksum:?} is NoFirmware without panic",
            );
        }
    }

    #[test]
    fn built_unpublish_firmware_responses_are_schema_valid() {
        // Every status arm the wire can carry — including the documented unproduced
        // `DownloadOngoing` seam — satisfies the bundled OCPP 2.0.1 UnpublishFirmware
        // response JSON Schema.
        let validator = SchemaValidator::v201();
        for status in [
            UnpublishFirmwareStatusEnumType::DownloadOngoing,
            UnpublishFirmwareStatusEnumType::NoFirmware,
            UnpublishFirmwareStatusEnumType::Unpublished,
        ] {
            let resp = v201_unpublish_firmware_response(status);
            assert_eq!(resp.status, status);
            assert!(
                resp.custom_data.is_none(),
                "the constructor attaches no vendor extension"
            );
            validator
                .validate_call_result("UnpublishFirmware", &serde_json::to_value(&resp).unwrap())
                .expect("built UnpublishFirmware response is schema-valid");
        }
    }
}
