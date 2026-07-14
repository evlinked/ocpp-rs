//! Deriving a Charge Detail Record (CDR) from CSMS transaction events.
//!
//! Demonstrates the embeddable-CSMS event surface (issue #66): a host embeds the
//! Central System as a library and observes the transaction lifecycle as typed
//! [`TransportEvent`]s, from which it synthesizes billing-shaped records —
//! exactly what the charge-hub pure-CPO adapter does to turn OCPP telemetry into
//! OCPI CDRs.
//!
//! In a live deployment the events arrive on the channel returned by
//! `OcppServer::new(config, handler)`:
//!
//! ```ignore
//! let (mut server, mut events) = OcppServer::new(TransportConfig::default(), handler);
//! server.start("0.0.0.0:9000").await?;
//! let mut tracker = SessionTracker::default();
//! while let Some(event) = events.recv().await {
//!     if let Some(cdr) = tracker.observe(&event) {
//!         println!("{cdr:?}"); // persist / forward as an OCPI CDR
//!     }
//! }
//! ```
//!
//! To keep this example deterministic (no sockets, no wall clock), `main` feeds
//! the tracker the exact two events the server's receive loop emits for an
//! accepted `StartTransaction` → `StopTransaction` pair. The end-to-end wiring
//! over a real WebSocket is pinned by `tests/transaction_events.rs`.
//!
//! Run with: `cargo run -p ocpp-transport --example csms_cdr`

use std::collections::HashMap;

use chrono::{DateTime, TimeZone, Utc};
use ocpp_transport::TransportEvent;

/// A billing-shaped Charge Detail Record derived from a completed transaction —
/// the `TransactionStarted`/`TransactionStopped` pair that bracket a session.
///
/// Deliberately a plain data record: ocpp-rs surfaces the OCPP lifecycle; the
/// mapping to a specific billing schema (e.g. an OCPI CDR) belongs to the host.
#[derive(Debug, Clone, PartialEq)]
struct ChargeDetailRecord {
    /// Charge point that ran the session.
    cp_id: String,
    /// CSMS-assigned transaction id.
    transaction_id: i32,
    /// Connector the energy was delivered on.
    connector_id: u32,
    /// Authorizing id tag.
    id_tag: String,
    /// Session start / stop timestamps as reported by the charge point.
    start_time: DateTime<Utc>,
    stop_time: DateTime<Utc>,
    /// Energy delivered over the session, in watt-hours (`meterStop − meterStart`).
    energy_wh: i32,
}

impl ChargeDetailRecord {
    /// Session duration in whole seconds (`stop_time − start_time`).
    fn duration_secs(&self) -> i64 {
        (self.stop_time - self.start_time).num_seconds()
    }
}

/// The open half of a session, retained between the start and stop events so the
/// two can be joined into a [`ChargeDetailRecord`].
#[derive(Debug, Clone)]
struct OpenSession {
    cp_id: String,
    connector_id: u32,
    id_tag: String,
    meter_start: i32,
    start_time: DateTime<Utc>,
}

/// Correlates `TransactionStarted` with its later `TransactionStopped` (keyed by
/// `transaction_id`) and emits a [`ChargeDetailRecord`] when the session closes.
#[derive(Debug, Default)]
struct SessionTracker {
    open: HashMap<i32, OpenSession>,
}

impl SessionTracker {
    /// Feed one transport event. Returns a finished CDR when this event closes a
    /// session (a `TransactionStopped` matching a tracked start); otherwise `None`.
    fn observe(&mut self, event: &TransportEvent) -> Option<ChargeDetailRecord> {
        match event {
            TransportEvent::TransactionStarted {
                cp_id,
                connector_id,
                id_tag,
                meter_start,
                timestamp,
                transaction_id,
            } => {
                self.open.insert(
                    *transaction_id,
                    OpenSession {
                        cp_id: cp_id.clone(),
                        connector_id: *connector_id,
                        id_tag: id_tag.clone(),
                        meter_start: *meter_start,
                        start_time: *timestamp,
                    },
                );
                None
            }
            TransportEvent::TransactionStopped {
                transaction_id,
                meter_stop,
                timestamp,
                ..
            } => {
                // A stop with no tracked start (e.g. a transaction begun before
                // this process started) can't be turned into a complete CDR.
                let start = self.open.remove(transaction_id)?;
                Some(ChargeDetailRecord {
                    cp_id: start.cp_id,
                    transaction_id: *transaction_id,
                    connector_id: start.connector_id,
                    id_tag: start.id_tag,
                    start_time: start.start_time,
                    stop_time: *timestamp,
                    energy_wh: meter_stop - start.meter_start,
                })
            }
            _ => None,
        }
    }
}

fn main() {
    // The two events the CSMS receive loop emits for an accepted
    // StartTransaction → StopTransaction pair (see tests/transaction_events.rs).
    let started = TransportEvent::TransactionStarted {
        cp_id: "CP-0001".to_string(),
        connector_id: 2,
        id_tag: "RFID-ABC".to_string(),
        meter_start: 1_000,
        timestamp: Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap(),
        transaction_id: 77,
    };
    let stopped = TransportEvent::TransactionStopped {
        cp_id: "CP-0001".to_string(),
        transaction_id: 77,
        meter_stop: 8_500,
        timestamp: Utc.with_ymd_and_hms(2026, 7, 14, 11, 30, 0).unwrap(),
        reason: None,
        id_tag: None,
    };

    let mut tracker = SessionTracker::default();

    // The start opens a session but doesn't yet complete a record.
    assert!(tracker.observe(&started).is_none());

    // The stop closes it and yields the CDR.
    let cdr = tracker
        .observe(&stopped)
        .expect("the matching stop must complete a CDR");

    println!("Derived Charge Detail Record:");
    println!("  charge point:   {}", cdr.cp_id);
    println!("  transaction:    {}", cdr.transaction_id);
    println!("  connector:      {}", cdr.connector_id);
    println!("  id tag:         {}", cdr.id_tag);
    println!("  start:          {}", cdr.start_time.to_rfc3339());
    println!("  stop:           {}", cdr.stop_time.to_rfc3339());
    println!("  duration:       {} s", cdr.duration_secs());
    println!("  energy:         {} Wh", cdr.energy_wh);

    // 8500 Wh − 1000 Wh over 90 minutes.
    assert_eq!(cdr.energy_wh, 7_500);
    assert_eq!(cdr.duration_secs(), 90 * 60);
}
