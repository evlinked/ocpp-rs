# ocpp-rs

*A modern, production-grade OCPP implementation in Rust. Batteries included for CSMS (server), Charge Point simulator (client), conformance tests, and observability.*

---

## Why this project
- **Reliability & scale**: WebSockets for thousands of chargers per node with predictable tail latencies (Tokio).  
- **Safety**: Memory-safe, fearless concurrency for 24/7 operations.  
- **Portability**: Small static binaries, easy on-prem and edge deployments.  

Target protocols: **OCPP 1.6J** first, then **OCPP 2.0.1** modules incrementally.

**Current status — supported versions:**

| Version | Status |
| --- | --- |
| OCPP **1.6J** | Implemented — framing, CSMS, transactions, CP simulator, commands, hardening (M1–M6) |
| OCPP **2.0.1** | 🚧 In progress (M7) — message-type coverage complete (all 64 CALL messages ported & draft-06 schema-validated); routing + server-side wiring landed; **CP-simulator wiring** is the sole remaining M7 item |

---

## Scope & Non-Goals

### In scope
- OCPP **WebSocket JSON** framing (CALL/CALLRESULT/CALLERROR)  
- Minimal **CSMS** (server) handling core 1.6J actions  
- **Charge Point simulator** (client) for local/integration tests  
- Persistence (Postgres) with idempotency & audit  
- Observability (metrics, tracing) & operational tooling  
- Transport hardening: backpressure, queue bounds, graceful drain  

### Out of scope (for this repo)
- End-user apps, billing, pricing, vouchers, e-invoicing  
- OCPI roaming (separate project)  
- Vendor-specific charger drivers/diagnostics beyond standard OCPP  

---

## Milestones

- [x] **M0**: Bootstrap & CI ¹
- [x] **M1**: Framing & envelopes (1.6J)
- [x] **M2**: CSMS minimal
- [x] **M3**: Transactions (Authorize, StartTransaction, StopTransaction)
- [x] **M4**: CP simulator (scenarios, jitter, reconnect storm)
- [x] **M5**: Commands & control (RemoteStartTransaction, Reset)
- [x] **M6**: Hardening (drain, rate limits, security)
- [ ] **M7**: OCPP 2.0.1 — 🚧 **in progress** (message-type coverage complete; routing + server-side wiring landed; CP-simulator wiring is the sole remaining item)

  **Message coverage — all 64 CALL messages ported.** All message
  types, payload datatypes, and enums are ported from the
  [mobilityhouse/ocpp](https://github.com/mobilityhouse/ocpp) 2.0.1 reference, each
  round-tripped in serde and validated against its bundled FINAL JSON Schema:

  - [x] **Provisioning & core** — BootNotification, Heartbeat, StatusNotification, SetNetworkProfile, GetBaseReport, GetReport, NotifyReport, Reset, TriggerMessage, DataTransfer
  - [x] **Availability & connector** — ChangeAvailability, UnlockConnector
  - [x] **Device model** (variables & monitoring) — GetVariables, SetVariables, SetVariableMonitoring, ClearVariableMonitoring, SetMonitoringBase, SetMonitoringLevel, GetMonitoringReport, NotifyMonitoringReport, NotifyEvent
  - [x] **Authorization & local list** — Authorize (incl. ISO 15118 certificate path), ClearCache, GetLocalListVersion, SendLocalList
  - [x] **Transactions & metering** — TransactionEvent (incl. `meterValue`), MeterValues, RequestStartTransaction, RequestStopTransaction, GetTransactionStatus, CostUpdated
  - [x] **Smart charging** — SetChargingProfile, GetChargingProfiles, ReportChargingProfiles, ClearChargingProfile, GetCompositeSchedule, NotifyChargingLimit, ClearedChargingLimit, NotifyEVChargingSchedule, NotifyEVChargingNeeds
  - [x] **Reservation** — ReserveNow, CancelReservation, ReservationStatusUpdate
  - [x] **Display messages** — SetDisplayMessage, GetDisplayMessages, ClearDisplayMessage, NotifyDisplayMessages
  - [x] **Firmware** — UpdateFirmware, FirmwareStatusNotification, PublishFirmware, PublishFirmwareStatusNotification, UnpublishFirmware
  - [x] **Certificates & security** (ISO 15118 / PKI) — CertificateSigned, SignCertificate, InstallCertificate, DeleteCertificate, GetInstalledCertificateIds, GetCertificateStatus, Get15118EVCertificate, SecurityEventNotification
  - [x] **Diagnostics & logging** — GetLog, LogStatusNotification
  - [x] **Customer information** — CustomerInformation, NotifyCustomerInformation

  **M7 wiring progress:**
  - [x] Route 2.0.1 CALLs through the version-generic dispatcher against `SchemaValidator::v201()` ([#259](https://github.com/EVLinked/ocpp-rs/issues/259))
  - [x] Negotiate `ocpp2.0.1` in the CSMS handshake ([#338](https://github.com/EVLinked/ocpp-rs/issues/338)) and round-trip a 2.0.1 `BootNotification`/`Heartbeat` CALL→CALLRESULT over a real `OcppServer` ([#342](https://github.com/EVLinked/ocpp-rs/issues/342)), plus a batteries-included v201 CSMS builder ([#343](https://github.com/EVLinked/ocpp-rs/issues/343))
  - [ ] Teach the `ocpp-cp` Charge Point simulator to speak 2.0.1 (the `sub_protocols: vec!["ocpp1.6"]` seam in `crates/ocpp-cp/src/lib.rs`) — **sole remaining M7 item**; direction tracked in [#256](https://github.com/EVLinked/ocpp-rs/issues/256)
- [ ] **M8**: Conformance & docs

¹ The `main` CI badge currently shows red only because of the GitHub Pages
deploy-permission issue tracked in [#41](https://github.com/EVLinked/ocpp-rs/issues/41)
(a maintainer permissions fix) — the build, lint, test, coverage, security, MSRV,
and conformance jobs all pass.

---

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
