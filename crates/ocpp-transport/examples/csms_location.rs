//! Deriving an OCPI-shaped Location / EVSE inventory from CSMS Boot + Status events.
//!
//! Demonstrates the *other* half of the embeddable-CSMS event surface (issue #66):
//! `examples/csms_cdr.rs` turns the transaction lifecycle into billing-shaped CDRs
//! (the Sessions / CDRs half); this turns the two events a charge point emits about
//! *itself* — a [`BootNotification`](TransportEvent::BootNotification) on connect
//! (self-reported identity) and a [`StatusNotification`](TransportEvent::StatusNotification)
//! per connector (availability) — into an OCPI-shaped **Location / EVSE inventory**.
//! That is exactly what the charge-hub pure-CPO adapter does to derive OCPI
//! Locations from OCPP telemetry.
//!
//! In a live deployment the events arrive on the channel returned by
//! `OcppServer::new(config, handler)`:
//!
//! ```ignore
//! let (mut server, mut events) = OcppServer::new(TransportConfig::default(), handler);
//! server.start("0.0.0.0:9000").await?;
//! let mut tracker = LocationTracker::default();
//! while let Some(event) = events.recv().await {
//!     if let Some(location) = tracker.observe(&event) {
//!         println!("{location:?}"); // upsert as an OCPI Location
//!     }
//! }
//! ```
//!
//! To keep this example deterministic (no sockets, no wall clock), `main` feeds the
//! tracker the exact events the server's receive loop emits for an accepted
//! `BootNotification` followed by per-connector `StatusNotification`s. The
//! end-to-end wiring over a real WebSocket is pinned by `tests/location_events.rs`.
//!
//! Run with: `cargo run -p ocpp-transport --example csms_location`

use std::collections::BTreeMap;

use ocpp_transport::TransportEvent;
use ocpp_types::v16j::ChargePointStatus;

/// Coarse, OCPI-EVSE-status-inspired availability, folded from the OCPP 1.6J
/// [`ChargePointStatus`] a charge point reports via `StatusNotification`.
///
/// Deliberately coarse: OCPP 1.6J has nine connector states, OCPI's EVSE `status`
/// far fewer, and the *precise* mapping (and whether `Preparing`/`Finishing` count
/// as free or occupied for a given CPO) is host policy — this is an illustrative
/// default, the same way `csms_cdr.rs` leaves the exact CDR schema to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvseStatus {
    /// Free to start a new session (`Available`).
    Available,
    /// Actively delivering energy (`Charging`).
    Charging,
    /// A session is in progress but not delivering energy
    /// (`Preparing` / `SuspendedEV` / `SuspendedEVSE` / `Finishing`).
    Occupied,
    /// Held for a specific user (`Reserved`).
    Reserved,
    /// Faulted / out of order (`Faulted`).
    OutOfOrder,
    /// Administratively unavailable (`Unavailable`).
    Inoperative,
}

impl EvseStatus {
    /// Fold a 1.6J connector state into the coarse OCPI-style status.
    fn from_ocpp(status: ChargePointStatus) -> Self {
        match status {
            ChargePointStatus::Available => EvseStatus::Available,
            ChargePointStatus::Charging => EvseStatus::Charging,
            ChargePointStatus::Preparing
            | ChargePointStatus::SuspendedEV
            | ChargePointStatus::SuspendedEVSE
            | ChargePointStatus::Finishing => EvseStatus::Occupied,
            ChargePointStatus::Reserved => EvseStatus::Reserved,
            ChargePointStatus::Faulted => EvseStatus::OutOfOrder,
            ChargePointStatus::Unavailable => EvseStatus::Inoperative,
        }
    }
}

/// An OCPI-shaped Location snapshot derived from a single charge point's Boot +
/// Status events.
///
/// One OCPP 1.6J connector maps to one OCPI EVSE (the usual CPO convention — 1.6J
/// has no native EVSE concept), keyed by the reported `connectorId`. The reserved
/// `connectorId == 0` addresses the charge point as a whole, so its status is held
/// separately as [`station_status`](Self::station_status) rather than as an EVSE.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Location {
    /// Charge point that owns this Location (the WebSocket path segment).
    cp_id: String,
    /// Self-reported vendor (`chargePointVendor`), empty until a `BootNotification`.
    vendor: String,
    /// Self-reported model (`chargePointModel`), empty until a `BootNotification`.
    model: String,
    /// Serial number, when the CP supplies one.
    serial_number: Option<String>,
    /// Firmware version, when the CP supplies one.
    firmware_version: Option<String>,
    /// Whether the CP has announced itself yet (a `StatusNotification` seen before
    /// any `BootNotification` still yields a Location, but with empty identity).
    booted: bool,
    /// Station-wide status from a `connectorId == 0` `StatusNotification`.
    station_status: Option<EvseStatus>,
    /// Per-connector EVSE availability, keyed by 1-based `connectorId`.
    evses: BTreeMap<u32, EvseStatus>,
}

/// Folds a charge point's `BootNotification` + `StatusNotification` events into an
/// OCPI-shaped [`Location`] per `cp_id`, so a host embedding the CSMS can derive
/// Locations without parsing raw frames.
#[derive(Debug, Default)]
struct LocationTracker {
    locations: BTreeMap<String, Location>,
}

impl LocationTracker {
    /// Feed one transport event. Returns a snapshot of the affected Location when
    /// this event was a `BootNotification` / `StatusNotification` that updated the
    /// inventory; otherwise `None` (e.g. transaction or housekeeping events).
    fn observe(&mut self, event: &TransportEvent) -> Option<Location> {
        match event {
            TransportEvent::BootNotification {
                cp_id,
                vendor,
                model,
                serial_number,
                firmware_version,
            } => {
                let loc = self.entry(cp_id);
                loc.vendor = vendor.clone();
                loc.model = model.clone();
                loc.serial_number = serial_number.clone();
                loc.firmware_version = firmware_version.clone();
                loc.booted = true;
                Some(loc.clone())
            }
            TransportEvent::StatusNotification {
                cp_id,
                connector_id,
                status,
            } => {
                let mapped = EvseStatus::from_ocpp(*status);
                let loc = self.entry(cp_id);
                if *connector_id == 0 {
                    loc.station_status = Some(mapped);
                } else {
                    loc.evses.insert(*connector_id, mapped);
                }
                Some(loc.clone())
            }
            _ => None,
        }
    }

    /// The Location for `cp_id`, inserting an identity-less one on first sight (a
    /// `StatusNotification` can arrive before the `BootNotification`, or before this
    /// process started observing).
    fn entry(&mut self, cp_id: &str) -> &mut Location {
        self.locations
            .entry(cp_id.to_string())
            .or_insert_with(|| Location {
                cp_id: cp_id.to_string(),
                ..Location::default()
            })
    }
}

fn main() {
    // The events the CSMS receive loop emits for an accepted BootNotification
    // followed by per-connector StatusNotifications (see tests/location_events.rs).
    let boot = TransportEvent::BootNotification {
        cp_id: "CP-0001".to_string(),
        vendor: "AcmeCharge".to_string(),
        model: "AC-22".to_string(),
        serial_number: Some("SN-0001".to_string()),
        firmware_version: Some("1.4.2".to_string()),
    };
    let connector1_available = TransportEvent::StatusNotification {
        cp_id: "CP-0001".to_string(),
        connector_id: 1,
        status: ChargePointStatus::Available,
    };
    let connector2_charging = TransportEvent::StatusNotification {
        cp_id: "CP-0001".to_string(),
        connector_id: 2,
        status: ChargePointStatus::Charging,
    };

    let mut tracker = LocationTracker::default();

    // The boot establishes identity; no EVSEs reported yet.
    let after_boot = tracker.observe(&boot).expect("boot updates the Location");
    assert!(after_boot.booted);
    assert_eq!(after_boot.vendor, "AcmeCharge");
    assert!(after_boot.evses.is_empty());

    // Each StatusNotification adds/updates one EVSE.
    tracker.observe(&connector1_available);
    let location = tracker
        .observe(&connector2_charging)
        .expect("status updates the Location");

    println!("Derived OCPI-shaped Location:");
    println!("  charge point:   {}", location.cp_id);
    println!("  vendor / model: {} / {}", location.vendor, location.model);
    println!(
        "  serial / fw:    {} / {}",
        location.serial_number.as_deref().unwrap_or("-"),
        location.firmware_version.as_deref().unwrap_or("-"),
    );
    for (connector_id, status) in &location.evses {
        println!("  EVSE (connector {connector_id}): {status:?}");
    }

    // Identity from the boot; one EVSE per reported connector, coarse-mapped.
    assert_eq!(location.serial_number.as_deref(), Some("SN-0001"));
    assert_eq!(location.firmware_version.as_deref(), Some("1.4.2"));
    assert_eq!(location.evses.get(&1), Some(&EvseStatus::Available));
    assert_eq!(location.evses.get(&2), Some(&EvseStatus::Charging));
    assert_eq!(location.evses.len(), 2);
    assert!(location.station_status.is_none());
}
