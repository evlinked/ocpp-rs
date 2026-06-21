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
| OCPP **2.0.1** | 🚧 In progress — message types & draft-06 schema validation landing incrementally (M7) |

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
- [ ] **M7**: OCPP 2.0.1 (initial) — 🚧 **in progress**
  - [x] BootNotification
  - [x] Heartbeat
  - [x] StatusNotification
  - [x] Authorize (incl. ISO 15118 certificate path)
  - [x] GetVariables / SetVariables (device model)
  - [x] TransactionEvent (transaction model)
  - [ ] Reset _(in review)_
  - [ ] TransactionEvent `meterValue` _(in review)_
  - [ ] RequestStartTransaction / RequestStopTransaction
  - [ ] ChargingProfileType
- [ ] **M8**: Conformance & docs

¹ The `main` CI badge currently shows red only because of the GitHub Pages
deploy-permission issue tracked in [#41](https://github.com/EVLinked/ocpp-rs/issues/41)
(a maintainer permissions fix) — the build, lint, test, coverage, security, MSRV,
and conformance jobs all pass.

---

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
