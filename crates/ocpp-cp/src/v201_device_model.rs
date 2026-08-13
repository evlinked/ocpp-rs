//! OCPP 2.0.1 device-model store for the Charge Point simulator.
//!
//! The 2.0.1 device model replaces 1.6J's flat `key`/`value` configuration
//! (`GetConfiguration` / `ChangeConfiguration`) with a structured
//! **component → variable → attribute** tree: a value is addressed by a
//! [`ComponentType`] (name, optional instance, optional EVSE) plus a
//! [`VariableType`] (name, optional instance) plus an [`AttributeEnumType`]
//! (`Actual` / `Target` / `MinSet` / `MaxSet`).
//!
//! [`V201DeviceModel`] is the simulator's in-memory realization of that tree.
//! It backs both device-model seams:
//! - the **read** seam ([`get`](V201DeviceModel::get)) consumed by the
//!   `GetVariables` handler, and
//! - the **write** seam ([`set`](V201DeviceModel::set)) consumed by the
//!   `SetVariables` handler.
//!
//! Ports the data the CSMS reads/writes via
//! [`ocpp.v201.call.GetVariables`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py)
//! and
//! [`ocpp.v201.call.SetVariables`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py)
//! — the message types and their FINAL JSON Schema already live in
//! `ocpp-messages`; this is the Charging Station's answering behavior.
//!
//! **Write policy.** Each variable carries a write policy: whether the CSMS
//! may write it at all (`read_only` → a write is `Rejected`) and whether an
//! accepted write only takes effect after a reboot (`reboot_required` →
//! `RebootRequired`). A simulator applies the value immediately in either case;
//! `RebootRequired` is the signal the operator must still reboot for the change
//! to be fully in effect.
//!
//! **Case-insensitivity.** Per the 2.0.1 spec, component and variable *names*
//! (and instances) are case-insensitive, so lookups normalize them to
//! lowercase. EVSE ids are numeric and matched exactly. The store keeps only
//! normalized keys; the `GetVariables` handler echoes the CSMS's original
//! (un-normalized) `component` / `variable` back on each result, as the schema
//! requires.

use ocpp_types::v201::{
    AttributeEnumType, ClearMonitoringResultType, ClearMonitoringStatusEnumType,
    ComponentCriterionEnumType, ComponentType, ComponentVariableType,
    GenericDeviceModelStatusEnumType, GetVariableStatusEnumType, MonitorBaseEnumType,
    MonitorEnumType, MonitoringCriterionEnumType, MonitoringDataType, MutabilityEnumType,
    ReportBaseEnumType, ReportDataType, SetMonitoringDataType, SetMonitoringResultType,
    SetMonitoringStatusEnumType, SetVariableStatusEnumType, VariableAttributeType,
    VariableMonitoringType, VariableType,
};
use std::collections::{HashMap, HashSet};
use std::ops::RangeInclusive;

/// Normalized identity of a component: lowercased name + instance, plus the
/// (numeric) EVSE id / connector id it is scoped to. A station-wide component
/// carries no EVSE.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ComponentKey {
    name: String,
    instance: Option<String>,
    evse_id: Option<i32>,
    connector_id: Option<i32>,
}

impl ComponentKey {
    fn from_request(component: &ComponentType) -> Self {
        ComponentKey {
            name: component.name.to_lowercase(),
            instance: component.instance.as_ref().map(|s| s.to_lowercase()),
            evse_id: component.evse.as_ref().map(|e| e.id),
            connector_id: component.evse.as_ref().and_then(|e| e.connector_id),
        }
    }
}

/// Normalized identity of a variable *within* a component: the component key
/// plus the lowercased variable name + instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct VariableKey {
    component: ComponentKey,
    name: String,
    instance: Option<String>,
}

impl VariableKey {
    fn from_request(component: &ComponentType, variable: &VariableType) -> Self {
        VariableKey {
            component: ComponentKey::from_request(component),
            name: variable.name.to_lowercase(),
            instance: variable.instance.as_ref().map(|s| s.to_lowercase()),
        }
    }
}

/// Whether — and how — the CSMS may write a variable via `SetVariables`.
///
/// A `read_only` variable rejects writes; a `reboot_required` variable accepts
/// them but signals that a reboot is needed for the change to take full effect.
/// The default (`ReadWrite`, no reboot) matches a plainly configurable variable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct WritePolicy {
    read_only: bool,
    reboot_required: bool,
}

/// One variable's stored state: its attribute values, its write policy, and the
/// **display forms** of its component / variable identity.
///
/// A variable exposes at most the four `AttributeEnumType`s, so a small `Vec`
/// (linear scan) is cheaper and simpler than a map — and `AttributeEnumType` is
/// not `Hash`.
///
/// The [`VariableKey`] that addresses this entry is lowercased for
/// case-insensitive lookup, which loses the original casing. `component` /
/// `variable` keep the identity in its CSMS-visible form so `GetBaseReport` →
/// `NotifyReport` can reproduce the real names (`OCPPCommCtrlr`, not
/// `ocppcommctrlr`) rather than the normalized keys.
#[derive(Debug, Clone)]
struct VariableEntry {
    attributes: Vec<(AttributeEnumType, String)>,
    policy: WritePolicy,
    component: ComponentType,
    variable: VariableType,
}

/// One installed variable monitor: the normalized identity of the
/// component-variable it watches, that variable's display (CSMS-visible) casing,
/// and the monitor definition itself.
///
/// The [`key`](Self::key) drives filtering and grouping in
/// [`monitoring_snapshot`](V201DeviceModel::monitoring_snapshot) (case-insensitive,
/// exactly like the `GetVariables`/`GetReport` seams); `component`/`variable`
/// keep the original casing so the streamed `NotifyMonitoringReport` reproduces
/// the real names rather than the lowercased keys.
#[derive(Debug, Clone)]
struct MonitorEntry {
    key: VariableKey,
    component: ComponentType,
    variable: VariableType,
    monitor: VariableMonitoringType,
}

/// The most permissive monitoring reporting level (`9`, Debug). Every installed
/// monitor's severity is in `0..=9`, so at this level nothing is suppressed. The
/// device model's default until a CSMS narrows it via `SetMonitoringLevel`.
pub const DEFAULT_MONITORING_LEVEL: i32 = 9;

/// The inclusive severity range a `SetMonitoringLevel` may set: `0` (Danger) …
/// `9` (Debug); a lower number is more severe. This range is **not** pinned by
/// the payload schema (which only constrains `severity` to `integer`), so the
/// station enforces it here — an out-of-range value is a modeled rejection.
const MONITORING_LEVEL_RANGE: RangeInclusive<i32> = 0..=9;

/// The default active monitoring base (`All`): every monitor — pre-configured
/// plus CSMS-installed — is in scope. The device model's default until a CSMS
/// narrows it via `SetMonitoringBase`.
pub const DEFAULT_MONITORING_BASE: MonitorBaseEnumType = MonitorBaseEnumType::All;

/// The Charge Point simulator's OCPP 2.0.1 device model: the attribute values a
/// CSMS can read via `GetVariables` and write via `SetVariables`, plus the
/// variable monitors it installs via `SetVariableMonitoring` and streams via
/// `GetMonitoringReport` → `NotifyMonitoringReport`.
///
/// `components` is tracked separately from `variables` so a read/write can tell
/// an *unknown component* apart from a *known component with an unknown
/// variable* — the two map to distinct status outcomes.
///
/// `Default` is hand-written (not derived) so `active_monitoring_level` starts at
/// [`DEFAULT_MONITORING_LEVEL`] (`9`) rather than the numeric zero a derive would
/// give — `0` is the *most* severe level, which would silently suppress all but
/// `Danger` monitors on a fresh station. `active_monitoring_base` starts at
/// [`DEFAULT_MONITORING_BASE`] (`All`) for the same reason.
#[derive(Debug, Clone)]
pub struct V201DeviceModel {
    /// Every component identity the model knows about (even those whose
    /// requested variable is absent), for the `UnknownComponent` distinction.
    components: HashSet<ComponentKey>,
    /// Per-variable stored attribute values and write policy.
    variables: HashMap<VariableKey, VariableEntry>,
    /// Installed variable monitors, keyed by their station-assigned id — the
    /// primary index, so `ClearVariableMonitoring` (a follow-up) can clear by id
    /// and `SetVariableMonitoring` can replace an existing monitor by id.
    monitors: HashMap<i32, MonitorEntry>,
    /// The last monitor id assigned. Monotonic (never reused), so a cleared id
    /// is never handed to a later monitor. Starts at 0 (`Default`), so the first
    /// assigned id is 1.
    last_monitor_id: i32,
    /// The active monitoring **reporting level** (severity threshold) set by
    /// `SetMonitoringLevel`: the station reports monitors whose `severity` is at
    /// or below this number (lower = more severe; `0` Danger … `9` Debug).
    /// Defaults to [`DEFAULT_MONITORING_LEVEL`] (`9`, report everything) until a
    /// CSMS narrows it.
    ///
    /// Recorded but not yet **enforced**: there is no live `NotifyEvent` emitter
    /// on the simulator to gate, so no installed monitor's reporting changes
    /// today. The level is stored and readable ([`active_monitoring_level`]) so a
    /// future emitter can honor it — mirroring how the monitor store landed
    /// before the report seam that reads it.
    ///
    /// [`active_monitoring_level`]: V201DeviceModel::active_monitoring_level
    active_monitoring_level: i32,
    /// The active monitoring **base** selected by `SetMonitoringBase`: which set
    /// of pre-configured variable monitors is active — [`All`], the
    /// [`FactoryDefault`] set, or [`HardWiredOnly`]. Defaults to
    /// [`DEFAULT_MONITORING_BASE`] ([`All`]) until a CSMS narrows it.
    ///
    /// Recorded but not (fully) **enforced**: the monitor store models only the
    /// CSMS-installed monitors and has no pre-configured / hard-wired seam yet, so
    /// selecting a base changes no monitor's activation today. The selected base
    /// is stored and readable ([`active_monitoring_base`]) so a future
    /// pre-configured-monitor seam can honor it — mirroring how the reporting
    /// level ([`active_monitoring_level`]) landed before the emitter that reads
    /// it. Because the store has no hard-wired monitors, [`HardWiredOnly`] is a
    /// modeled seam — see [`set_monitoring_base`].
    ///
    /// [`All`]: MonitorBaseEnumType::All
    /// [`FactoryDefault`]: MonitorBaseEnumType::FactoryDefault
    /// [`HardWiredOnly`]: MonitorBaseEnumType::HardWiredOnly
    /// [`active_monitoring_base`]: V201DeviceModel::active_monitoring_base
    /// [`active_monitoring_level`]: V201DeviceModel::active_monitoring_level
    /// [`set_monitoring_base`]: V201DeviceModel::set_monitoring_base
    active_monitoring_base: MonitorBaseEnumType,
}

impl Default for V201DeviceModel {
    fn default() -> Self {
        Self {
            components: HashSet::new(),
            variables: HashMap::new(),
            monitors: HashMap::new(),
            last_monitor_id: 0,
            active_monitoring_level: DEFAULT_MONITORING_LEVEL,
            active_monitoring_base: DEFAULT_MONITORING_BASE,
        }
    }
}

impl V201DeviceModel {
    /// An empty device model.
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a station-wide (no instance, no EVSE) variable attribute as plainly
    /// writable (`ReadWrite`, no reboot). Used by
    /// [`with_standard_profile`](Self::with_standard_profile) and available for
    /// tests / future wiring.
    pub fn set_station_variable(
        &mut self,
        component: &str,
        variable: &str,
        attribute: AttributeEnumType,
        value: impl Into<String>,
    ) {
        self.seed_station_variable(
            component,
            variable,
            attribute,
            value,
            WritePolicy::default(),
        );
    }

    /// Seed a station-wide variable attribute with an explicit `WritePolicy`.
    /// The policy is a property of the variable, so a later attribute added to
    /// the same variable inherits (and may update) it — the seed sets each
    /// special variable exactly once.
    fn seed_station_variable(
        &mut self,
        component: &str,
        variable: &str,
        attribute: AttributeEnumType,
        value: impl Into<String>,
        policy: WritePolicy,
    ) {
        let component_key = ComponentKey {
            name: component.to_lowercase(),
            instance: None,
            evse_id: None,
            connector_id: None,
        };
        let variable_key = VariableKey {
            component: component_key.clone(),
            name: variable.to_lowercase(),
            instance: None,
        };
        self.components.insert(component_key);
        // Seed the display forms once, on first insert, so a later attribute
        // added to the same variable keeps the original casing.
        let entry = self
            .variables
            .entry(variable_key)
            .or_insert_with(|| VariableEntry {
                attributes: Vec::new(),
                policy,
                component: ComponentType {
                    name: component.to_string(),
                    instance: None,
                    evse: None,
                    custom_data: None,
                },
                variable: VariableType {
                    name: variable.to_string(),
                    instance: None,
                    custom_data: None,
                },
            });
        entry.policy = policy;
        let value = value.into();
        match entry.attributes.iter_mut().find(|(a, _)| *a == attribute) {
            Some(slot) => slot.1 = value,
            None => entry.attributes.push((attribute, value)),
        }
    }

    /// A minimal, representative 2.0.1 device-model profile: a handful of
    /// standard controller variables a real Charging Station exposes. These are
    /// the simulator's defaults (like the 1.6J `ConfigurationStore` seed), not a
    /// certifiable full component inventory — a follow-up can grow the set and
    /// make it writable via `SetVariables`.
    pub fn with_standard_profile() -> Self {
        use AttributeEnumType::Actual;
        let mut model = Self::new();
        // OCPPCommCtrlr — communication/session timing.
        model.set_station_variable("OCPPCommCtrlr", "HeartbeatInterval", Actual, "300");
        model.set_station_variable("OCPPCommCtrlr", "WebSocketPingInterval", Actual, "60");
        model.set_station_variable("OCPPCommCtrlr", "MessageTimeout", Actual, "30");
        // AlignedDataCtrlr — clock-aligned MeterValues.
        model.set_station_variable(
            "AlignedDataCtrlr",
            "Measurands",
            Actual,
            "Energy.Active.Import.Register",
        );
        model.set_station_variable("AlignedDataCtrlr", "Interval", Actual, "900");
        // SampledDataCtrlr — per-transaction sampling.
        model.set_station_variable("SampledDataCtrlr", "TxUpdatedInterval", Actual, "60");
        model.set_station_variable(
            "SampledDataCtrlr",
            "TxUpdatedMeasurands",
            Actual,
            "Energy.Active.Import.Register",
        );
        // SecurityCtrlr — identity.
        model.set_station_variable("SecurityCtrlr", "OrganizationName", Actual, "EVLinked");
        // A read-only capability constant: the maximum certificate-chain size the
        // station accepts is a fixed property, not something the CSMS configures.
        // Writing it is `Rejected` (a real read-only variable, per the 2.0.1
        // SecurityCtrlr controller).
        model.seed_station_variable(
            "SecurityCtrlr",
            "MaxCertificateChainSize",
            Actual,
            "3",
            WritePolicy {
                read_only: true,
                reboot_required: false,
            },
        );
        // A reboot-required variable: changing the network-configuration priority
        // is accepted and stored, but only takes full effect after the station
        // reboots — so a write returns `RebootRequired`.
        model.seed_station_variable(
            "OCPPCommCtrlr",
            "NetworkConfigurationPriority",
            Actual,
            "1",
            WritePolicy {
                read_only: false,
                reboot_required: true,
            },
        );
        model
    }

    /// Read one component-variable attribute.
    ///
    /// Returns the `GetVariables` outcome for this entry:
    /// - [`GetVariableStatusEnumType::UnknownComponent`] — no component with this
    ///   (normalized) identity exists.
    /// - [`GetVariableStatusEnumType::UnknownVariable`] — the component exists but
    ///   carries no such variable.
    /// - [`GetVariableStatusEnumType::NotSupportedAttributeType`] — the variable
    ///   exists but does not expose the requested attribute.
    /// - [`GetVariableStatusEnumType::Accepted`] with the value otherwise.
    ///
    /// [`GetVariableStatusEnumType::Rejected`] is reserved for internal read
    /// failures (e.g. an unreadable/secret value); the seeded read path never
    /// produces it.
    pub fn get(
        &self,
        component: &ComponentType,
        variable: &VariableType,
        attribute: AttributeEnumType,
    ) -> (GetVariableStatusEnumType, Option<String>) {
        let component_key = ComponentKey::from_request(component);
        if !self.components.contains(&component_key) {
            return (GetVariableStatusEnumType::UnknownComponent, None);
        }
        let variable_key = VariableKey::from_request(component, variable);
        let Some(entry) = self.variables.get(&variable_key) else {
            return (GetVariableStatusEnumType::UnknownVariable, None);
        };
        match entry.attributes.iter().find(|(a, _)| *a == attribute) {
            Some((_, value)) => (GetVariableStatusEnumType::Accepted, Some(value.clone())),
            None => (GetVariableStatusEnumType::NotSupportedAttributeType, None),
        }
    }

    /// Write one component-variable attribute.
    ///
    /// Returns the `SetVariables` outcome for this entry, evaluated in this
    /// precedence (the same identity checks as [`get`](Self::get), then the
    /// write policy):
    /// - [`SetVariableStatusEnumType::UnknownComponent`] — no component with this
    ///   (normalized) identity exists.
    /// - [`SetVariableStatusEnumType::UnknownVariable`] — the component exists but
    ///   carries no such variable.
    /// - [`SetVariableStatusEnumType::NotSupportedAttributeType`] — the variable
    ///   exists but does not expose the requested attribute (a write updates an
    ///   existing attribute slot; it does not conjure a new characteristic —
    ///   symmetric with the read seam).
    /// - [`SetVariableStatusEnumType::Rejected`] — the variable is read-only; the
    ///   stored value is left unchanged.
    /// - [`SetVariableStatusEnumType::RebootRequired`] — accepted and applied, but
    ///   a reboot is needed for the change to take full effect.
    /// - [`SetVariableStatusEnumType::Accepted`] — written and effective now.
    pub fn set(
        &mut self,
        component: &ComponentType,
        variable: &VariableType,
        attribute: AttributeEnumType,
        value: &str,
    ) -> SetVariableStatusEnumType {
        let component_key = ComponentKey::from_request(component);
        if !self.components.contains(&component_key) {
            return SetVariableStatusEnumType::UnknownComponent;
        }
        let variable_key = VariableKey::from_request(component, variable);
        let Some(entry) = self.variables.get_mut(&variable_key) else {
            return SetVariableStatusEnumType::UnknownVariable;
        };
        let Some(slot) = entry.attributes.iter_mut().find(|(a, _)| *a == attribute) else {
            return SetVariableStatusEnumType::NotSupportedAttributeType;
        };
        if entry.policy.read_only {
            // Reject *before* mutating: a read-only variable keeps its value.
            return SetVariableStatusEnumType::Rejected;
        }
        slot.1 = value.to_string();
        if entry.policy.reboot_required {
            SetVariableStatusEnumType::RebootRequired
        } else {
            SetVariableStatusEnumType::Accepted
        }
    }

    /// Enumerate the device model as `NotifyReport` [`ReportDataType`] entries,
    /// selected by the requested [`ReportBaseEnumType`].
    ///
    /// This is the pure data half of the `GetBaseReport` → `NotifyReport` report
    /// seam: the handler acks `GetBaseReport` and then streams whatever this
    /// returns. Each entry reports a variable in its **display** (CSMS-visible)
    /// casing with one [`VariableAttributeType`] per stored attribute, carrying
    /// the value and the mutability derived from the write policy
    /// (`read_only` → `ReadOnly`, otherwise `ReadWrite`).
    ///
    /// `reportBase` selects the slice:
    /// - [`FullInventory`](ReportBaseEnumType::FullInventory) — every variable.
    /// - [`ConfigurationInventory`](ReportBaseEnumType::ConfigurationInventory) —
    ///   the writable configuration only (read-only capability constants are
    ///   omitted).
    /// - [`SummaryInventory`](ReportBaseEnumType::SummaryInventory) — variables
    ///   in a non-default / abnormal state. A freshly-booted simulator has none,
    ///   so this is empty (the handler turns an empty report into
    ///   `EmptyResultSet`). Precise summary semantics (changed-from-default,
    ///   Faulted components) are a later slice.
    ///
    /// The result is **deterministically ordered** — sorted by component name /
    /// instance / EVSE then variable name / instance — so callers (and tests) see
    /// a stable report independent of the backing `HashMap`'s iteration order.
    /// `variableCharacteristics` is left unset: the seed does not model typed
    /// characteristics yet (a documented follow-up); the schema makes it
    /// optional.
    pub fn report(&self, report_base: ReportBaseEnumType) -> Vec<ReportDataType> {
        // SummaryInventory: nothing noteworthy on a healthy, freshly-booted
        // simulator — an empty report the handler maps to EmptyResultSet.
        if matches!(report_base, ReportBaseEnumType::SummaryInventory) {
            return Vec::new();
        }
        let mut report: Vec<ReportDataType> = self
            .variables
            .values()
            .filter(|entry| match report_base {
                // Configuration inventory is the *writable* configuration only.
                ReportBaseEnumType::ConfigurationInventory => !entry.policy.read_only,
                // Full inventory reports everything.
                ReportBaseEnumType::FullInventory => true,
                // Handled above; keep the match exhaustive without a wildcard so
                // a new report base is a compile error to triage here.
                ReportBaseEnumType::SummaryInventory => false,
            })
            .map(Self::entry_to_report_data)
            .collect();
        Self::sort_report(&mut report);
        report
    }

    /// Enumerate the device model as `NotifyReport` [`ReportDataType`] entries,
    /// *filtered* by the `GetReport` selectors instead of a coarse
    /// [`ReportBaseEnumType`].
    ///
    /// This is the pure data half of the `GetReport` → `NotifyReport` seam
    /// (`ocpp.v201.call.GetReport`), the selection variant of
    /// [`report`](Self::report): the handler acks `GetReport` and then streams
    /// whatever this returns. Both filters are optional and AND together; an
    /// absent or empty filter does not narrow on that axis (so both absent =
    /// every reportable variable, matching `FullInventory`):
    ///
    /// - **`component_variable`** narrows to specific component-variables. Each
    ///   entry matches case-insensitively on component/variable name + instance
    ///   and exactly on EVSE id (the same `ComponentKey`/`VariableKey`
    ///   normalization `GetVariables`/`SetVariables` use). An entry whose
    ///   `variable` is `None` matches the *whole component* (every variable
    ///   under it). A variable is kept if it matches **any** filter entry.
    /// - **`component_criteria`** narrows by component state
    ///   (`Active`/`Available`/`Enabled`/`Problem`). The seeded profile models a
    ///   **healthy** station — every reportable variable is `Active`,
    ///   `Available`, and `Enabled`, and none is in a `Problem` state — so a
    ///   variable is reported iff it satisfies **all** requested criteria. That
    ///   makes the criterion filter uniform across variables here: any list
    ///   containing `Problem` matches nothing (→ `EmptyResultSet` at the
    ///   handler), any other list admits every variable. Per-variable
    ///   `Problem`/`Disabled`/`Unavailable` state is a later slice; the pure
    ///   decision below is already shaped to grow into it.
    ///
    /// The result is deterministically ordered, exactly as
    /// [`report`](Self::report) — see its ordering note.
    pub fn report_filtered(
        &self,
        component_variable: Option<&[ComponentVariableType]>,
        component_criteria: Option<&[ComponentCriterionEnumType]>,
    ) -> Vec<ReportDataType> {
        // Criteria gate first. On the healthy seed the criteria filter is
        // uniform (see the doc comment), so a list the healthy state fails to
        // satisfy (i.e. one requesting `Problem`) short-circuits to empty
        // without scanning the store.
        if let Some(criteria) = component_criteria {
            if !criteria.is_empty() && !Self::healthy_state_matches_criteria(criteria) {
                return Vec::new();
            }
        }
        // An empty `component_variable` list is treated as "no such filter" —
        // the schema requires at least one item when the field is present, so
        // this is defensive, not a spec path.
        let variable_filter = component_variable.filter(|filters| !filters.is_empty());
        let mut report: Vec<ReportDataType> = self
            .variables
            .iter()
            .filter(|(key, _)| match variable_filter {
                None => true,
                Some(filters) => Self::variable_matches_filter(key, filters),
            })
            .map(|(_, entry)| Self::entry_to_report_data(entry))
            .collect();
        Self::sort_report(&mut report);
        report
    }

    /// Whether a stored variable (addressed by `key`) matches **any** entry of a
    /// `GetReport` `componentVariable` filter. A filter entry with no `variable`
    /// matches the whole component; one with a `variable` matches that exact
    /// component-variable. Matching reuses the case-insensitive
    /// [`ComponentKey`]/[`VariableKey`] normalization, so `EVSE`/`evse` and
    /// `OCPPCommCtrlr`/`ocppcommctrlr` are the same identity as they are for
    /// `GetVariables`.
    fn variable_matches_filter(key: &VariableKey, filters: &[ComponentVariableType]) -> bool {
        filters.iter().any(|cv| match &cv.variable {
            None => key.component == ComponentKey::from_request(&cv.component),
            Some(variable) => *key == VariableKey::from_request(&cv.component, variable),
        })
    }

    /// Whether the seeded (healthy) component state satisfies **all** requested
    /// `componentCriteria`. Every reportable variable in the standard profile is
    /// `Active`, `Available`, and `Enabled`, and none is in a `Problem` state,
    /// so those three criteria always hold and `Problem` never does. The match
    /// is exhaustive without a wildcard, so a new criterion is a compile error
    /// to triage here.
    fn healthy_state_matches_criteria(criteria: &[ComponentCriterionEnumType]) -> bool {
        criteria.iter().all(|c| match c {
            ComponentCriterionEnumType::Active
            | ComponentCriterionEnumType::Available
            | ComponentCriterionEnumType::Enabled => true,
            ComponentCriterionEnumType::Problem => false,
        })
    }

    /// Render one stored variable entry as a `NotifyReport` [`ReportDataType`],
    /// in its display (CSMS-visible) casing, with one [`VariableAttributeType`]
    /// per stored attribute carrying the value and the mutability derived from
    /// the write policy. Shared by [`report`](Self::report) and
    /// [`report_filtered`](Self::report_filtered).
    fn entry_to_report_data(entry: &VariableEntry) -> ReportDataType {
        let mutability = if entry.policy.read_only {
            MutabilityEnumType::ReadOnly
        } else {
            MutabilityEnumType::ReadWrite
        };
        let variable_attribute = entry
            .attributes
            .iter()
            .map(|(kind, value)| VariableAttributeType {
                kind: Some(*kind),
                value: Some(value.clone()),
                mutability: Some(mutability),
                persistent: None,
                constant: None,
                custom_data: None,
            })
            .collect();
        ReportDataType {
            component: entry.component.clone(),
            variable: entry.variable.clone(),
            variable_attribute,
            variable_characteristics: None,
            custom_data: None,
        }
    }

    /// Impose a stable, reviewable order on a report — sorted by component name
    /// / instance / EVSE then variable name / instance — independent of the
    /// backing `HashMap`'s iteration order, so callers and tests see a
    /// deterministic result.
    fn sort_report(report: &mut [ReportDataType]) {
        report.sort_by(|a, b| {
            (
                &a.component.name,
                &a.component.instance,
                a.component.evse.as_ref().map(|e| e.id),
                &a.variable.name,
                &a.variable.instance,
            )
                .cmp(&(
                    &b.component.name,
                    &b.component.instance,
                    b.component.evse.as_ref().map(|e| e.id),
                    &b.variable.name,
                    &b.variable.instance,
                ))
        });
    }

    /// The filtered snapshot of the station's *variable monitors* a
    /// `GetMonitoringReport` asks for — the monitoring counterpart of
    /// [`report_filtered`](Self::report_filtered) (which returns the device-model
    /// *inventory*). Where `report_filtered` reports component/variable rows, this
    /// reports the [`SetVariableMonitoring`] monitors installed *on* those
    /// variables (thresholds, deltas, periodics), narrowed by an optional
    /// `component_variable[]` and/or `monitoring_criteria[]`.
    ///
    /// Monitors are installed by [`install_monitors`](Self::install_monitors)
    /// (the `SetVariableMonitoring` handler, issue #494) and grouped here into one
    /// [`MonitoringDataType`] row per component-variable, its `variableMonitoring`
    /// list id-sorted. Both filters narrow the result and AND together:
    /// `component_variable[]` matches case-insensitively exactly as
    /// [`report_filtered`](Self::report_filtered) does, and `monitoring_criteria[]`
    /// keeps a monitor whose kind falls under **any** requested criterion (the
    /// criteria OR together). An absent or empty filter does not narrow on that
    /// axis, so both absent = every installed monitor. On a station with no
    /// monitors installed the snapshot is empty and the handler answers
    /// `EmptyResultSet`.
    ///
    /// [`SetVariableMonitoring`]: https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py
    pub fn monitoring_snapshot(
        &self,
        component_variable: Option<&[ComponentVariableType]>,
        monitoring_criteria: Option<&[MonitoringCriterionEnumType]>,
    ) -> Vec<MonitoringDataType> {
        // Both filters are optional and AND together; an absent or empty list
        // does not narrow on that axis (the schema requires ≥1 item when a
        // filter is present, so the empty case is defensive, not a spec path) —
        // matching how `report_filtered` treats its filters.
        let variable_filter = component_variable.filter(|filters| !filters.is_empty());
        let criteria_filter = monitoring_criteria.filter(|criteria| !criteria.is_empty());

        // Collect matching monitors grouped by their component-variable identity.
        // The group value carries the display-form component/variable (from the
        // first monitor seen for that identity) plus its monitors; the output is
        // sorted at the end, so `HashMap` iteration order does not leak.
        let mut groups: HashMap<
            VariableKey,
            (ComponentType, VariableType, Vec<VariableMonitoringType>),
        > = HashMap::new();
        for entry in self.monitors.values() {
            if let Some(filters) = variable_filter {
                if !Self::variable_matches_filter(&entry.key, filters) {
                    continue;
                }
            }
            if let Some(criteria) = criteria_filter {
                if !criteria
                    .iter()
                    .any(|c| Self::monitor_matches_criterion(entry.monitor.kind, *c))
                {
                    continue;
                }
            }
            groups
                .entry(entry.key.clone())
                .or_insert_with(|| (entry.component.clone(), entry.variable.clone(), Vec::new()))
                .2
                .push(entry.monitor.clone());
        }

        let mut snapshot: Vec<MonitoringDataType> = groups
            .into_values()
            .map(|(component, variable, mut variable_monitoring)| {
                // Stable order within a component-variable row: by monitor id.
                variable_monitoring.sort_by_key(|m| m.id);
                MonitoringDataType {
                    component,
                    variable,
                    variable_monitoring,
                    custom_data: None,
                }
            })
            .collect();
        Self::sort_monitoring(&mut snapshot);
        snapshot
    }

    /// Whether a monitor of the given [`MonitorEnumType`] falls under a requested
    /// `GetMonitoringReport` [`MonitoringCriterionEnumType`]. The three criteria
    /// partition the five monitor kinds: `ThresholdMonitoring` covers
    /// `Upper`/`LowerThreshold`, `DeltaMonitoring` covers `Delta`, and
    /// `PeriodicMonitoring` covers `Periodic`/`PeriodicClockAligned`. The match is
    /// exhaustive without a wildcard so a new monitor kind is a compile error to
    /// triage here.
    fn monitor_matches_criterion(
        kind: MonitorEnumType,
        criterion: MonitoringCriterionEnumType,
    ) -> bool {
        match criterion {
            MonitoringCriterionEnumType::ThresholdMonitoring => matches!(
                kind,
                MonitorEnumType::UpperThreshold | MonitorEnumType::LowerThreshold
            ),
            MonitoringCriterionEnumType::DeltaMonitoring => matches!(kind, MonitorEnumType::Delta),
            MonitoringCriterionEnumType::PeriodicMonitoring => matches!(
                kind,
                MonitorEnumType::Periodic | MonitorEnumType::PeriodicClockAligned
            ),
        }
    }

    /// Impose a stable, reviewable order on a monitoring snapshot — sorted by
    /// component name / instance / EVSE then variable name / instance — so callers
    /// and tests see a deterministic result independent of `HashMap` iteration.
    /// The monitors within each row are already id-sorted by
    /// [`monitoring_snapshot`](Self::monitoring_snapshot).
    fn sort_monitoring(snapshot: &mut [MonitoringDataType]) {
        snapshot.sort_by(|a, b| {
            (
                &a.component.name,
                &a.component.instance,
                a.component.evse.as_ref().map(|e| e.id),
                &a.variable.name,
                &a.variable.instance,
            )
                .cmp(&(
                    &b.component.name,
                    &b.component.instance,
                    b.component.evse.as_ref().map(|e| e.id),
                    &b.variable.name,
                    &b.variable.instance,
                ))
        });
    }

    /// Install (or replace) a batch of variable monitors requested by a
    /// `SetVariableMonitoring`, returning one [`SetMonitoringResultType`] per
    /// requested monitor, in request order.
    ///
    /// This is the write half of the monitoring seam — the install counterpart to
    /// [`set`](Self::set) for the device-model inventory — and the source the
    /// `SetVariableMonitoring` handler drains under the model's write lock. Each
    /// installed monitor becomes visible to a subsequent `GetMonitoringReport` via
    /// [`monitoring_snapshot`](Self::monitoring_snapshot).
    ///
    /// Ports `ocpp.v201.call.SetVariableMonitoring` /
    /// `ocpp.v201.call_result.SetVariableMonitoring`.
    pub fn install_monitors(
        &mut self,
        data: &[SetMonitoringDataType],
    ) -> Vec<SetMonitoringResultType> {
        data.iter().map(|d| self.install_monitor(d)).collect()
    }

    /// Install (or replace) a single monitor, returning its per-monitor result.
    ///
    /// Decision precedence — the same identity checks as [`set`](Self::set), then
    /// the monitoring-specific outcomes:
    /// - [`UnknownComponent`](SetMonitoringStatusEnumType::UnknownComponent) — no
    ///   component with this (normalized) identity exists.
    /// - [`UnknownVariable`](SetMonitoringStatusEnumType::UnknownVariable) — the
    ///   component exists but carries no such variable.
    /// - **Replace** (`id` present) — a request carrying an `id` targets an
    ///   existing monitor *on the same variable*: the monitor is updated in place,
    ///   keeping its id ([`Accepted`](SetMonitoringStatusEnumType::Accepted)). An
    ///   `id` that names no monitor (or one on a different variable) is
    ///   [`Rejected`](SetMonitoringStatusEnumType::Rejected).
    /// - [`Duplicate`](SetMonitoringStatusEnumType::Duplicate) — an identical
    ///   monitor (same variable + `type` + `severity` + `value`) already exists.
    /// - [`Accepted`](SetMonitoringStatusEnumType::Accepted) — installed with a
    ///   freshly-assigned id.
    ///
    /// The result echoes the requested `type` / `severity` / `component` /
    /// `variable` so the CSMS can correlate each outcome to its request entry; the
    /// assigned `id` is returned only on `Accepted`.
    ///
    /// `UnsupportedMonitorType` is not produced: the simulator supports every
    /// monitor kind on any known variable (it does not model per-variable monitor
    /// capabilities). It stays in the ported status enum for the wire.
    fn install_monitor(&mut self, data: &SetMonitoringDataType) -> SetMonitoringResultType {
        let result = |status, id| SetMonitoringResultType {
            id,
            status,
            kind: data.kind,
            component: data.component.clone(),
            variable: data.variable.clone(),
            severity: data.severity,
            status_info: None,
            custom_data: None,
        };

        let component_key = ComponentKey::from_request(&data.component);
        if !self.components.contains(&component_key) {
            return result(SetMonitoringStatusEnumType::UnknownComponent, None);
        }
        let variable_key = VariableKey::from_request(&data.component, &data.variable);
        if !self.variables.contains_key(&variable_key) {
            return result(SetMonitoringStatusEnumType::UnknownVariable, None);
        }

        let transaction = data.transaction.unwrap_or(false);

        // Replace path: an `id` in the request replaces that existing monitor,
        // but only when it names a monitor on this same variable.
        if let Some(id) = data.id {
            return match self.monitors.get_mut(&id) {
                Some(entry) if entry.key == variable_key => {
                    entry.monitor = VariableMonitoringType {
                        id,
                        transaction,
                        value: data.value,
                        kind: data.kind,
                        severity: data.severity,
                        custom_data: None,
                    };
                    result(SetMonitoringStatusEnumType::Accepted, Some(id))
                }
                _ => result(SetMonitoringStatusEnumType::Rejected, None),
            };
        }

        // Duplicate: an identical monitor already exists on this variable. `value`
        // is compared by `to_bits()` — exact bit-for-bit identity, never
        // arithmetic — so this is `clippy::float_cmp`-clean and treats an
        // identical resend as a duplicate rather than a second monitor.
        let is_duplicate = self.monitors.values().any(|e| {
            e.key == variable_key
                && e.monitor.kind == data.kind
                && e.monitor.severity == data.severity
                && e.monitor.value.to_bits() == data.value.to_bits()
        });
        if is_duplicate {
            return result(SetMonitoringStatusEnumType::Duplicate, None);
        }

        self.last_monitor_id += 1;
        let id = self.last_monitor_id;
        self.monitors.insert(
            id,
            MonitorEntry {
                key: variable_key,
                component: data.component.clone(),
                variable: data.variable.clone(),
                monitor: VariableMonitoringType {
                    id,
                    transaction,
                    value: data.value,
                    kind: data.kind,
                    severity: data.severity,
                    custom_data: None,
                },
            },
        );
        result(SetMonitoringStatusEnumType::Accepted, Some(id))
    }

    /// Clear a batch of previously-installed variable monitors by id, returning
    /// one [`ClearMonitoringResultType`] per requested id, **in request order**.
    ///
    /// This is the teardown half of the monitoring seam — the inverse of
    /// [`install_monitors`](Self::install_monitors) — and the source the
    /// `ClearVariableMonitoring` handler drains under the model's write lock. A
    /// cleared monitor no longer appears in
    /// [`monitoring_snapshot`](Self::monitoring_snapshot), so a subsequent
    /// `GetMonitoringReport` stops reporting it.
    ///
    /// Per-id outcome:
    /// - [`Accepted`](ClearMonitoringStatusEnumType::Accepted) — a monitor with
    ///   this id existed and was removed.
    /// - [`NotFound`](ClearMonitoringStatusEnumType::NotFound) — no monitor with
    ///   this id: never installed, or already cleared — including the second
    ///   occurrence of a duplicate id within one batch, whose first occurrence
    ///   already removed it.
    ///
    /// [`Rejected`](ClearMonitoringStatusEnumType::Rejected) is not produced: the
    /// store holds only CSMS-installed monitors, so there is no station-refused /
    /// preconfigured monitor to refuse yet. It stays in the ported status enum
    /// for the wire (a future hard-wired-monitor seam maps to it) — mirroring how
    /// the install path documents the unproduced `UnsupportedMonitorType`.
    ///
    /// The assigned-id counter (`last_monitor_id`) is monotonic and is **not**
    /// rewound on clear, so a later install never reuses a cleared id.
    ///
    /// Ports `ocpp.v201.call.ClearVariableMonitoring` /
    /// `ocpp.v201.call_result.ClearVariableMonitoring`.
    pub fn clear_monitors(&mut self, ids: &[i32]) -> Vec<ClearMonitoringResultType> {
        ids.iter()
            .map(|&id| {
                // `HashMap::remove` returning the entry *is* the existence probe:
                // present → removed (`Accepted`), absent → `NotFound`. A repeated
                // id in one batch clears on its first occurrence and reports
                // `NotFound` thereafter — idempotent, and it cannot panic on any
                // wire input (an unknown or huge id simply fails to match).
                let status = if self.monitors.remove(&id).is_some() {
                    ClearMonitoringStatusEnumType::Accepted
                } else {
                    ClearMonitoringStatusEnumType::NotFound
                };
                ClearMonitoringResultType {
                    status,
                    id,
                    status_info: None,
                    custom_data: None,
                }
            })
            .collect()
    }

    /// Set the active monitoring **reporting level** (the state half of
    /// `ocpp.v201.call.SetMonitoringLevel`). `severity` must be in
    /// `MONITORING_LEVEL_RANGE` (`0..=9`; `0` Danger … `9` Debug):
    ///
    /// - in range → the level is stored and this returns `true` (the handler
    ///   answers `Accepted`);
    /// - out of range → the stored level is left **unchanged** and this returns
    ///   `false` (the handler answers `Rejected`).
    ///
    /// `severity` is only range-compared, never parsed or used as an index, so
    /// any `i32` — negative, `> 9`, `i32::MIN` / `i32::MAX` — is handled without
    /// panicking or overflowing.
    pub fn set_monitoring_level(&mut self, severity: i32) -> bool {
        if MONITORING_LEVEL_RANGE.contains(&severity) {
            self.active_monitoring_level = severity;
            true
        } else {
            false
        }
    }

    /// The active monitoring reporting level (severity threshold). Every
    /// installed monitor whose `severity` is at or below this number is reported;
    /// the rest are suppressed. Defaults to [`DEFAULT_MONITORING_LEVEL`] (`9`,
    /// nothing suppressed) until a `SetMonitoringLevel` narrows it.
    pub fn active_monitoring_level(&self) -> i32 {
        self.active_monitoring_level
    }

    /// Select the active monitoring **base** (the state half of
    /// `ocpp.v201.call.SetMonitoringBase`) and report the station's decision as a
    /// [`GenericDeviceModelStatusEnumType`]:
    ///
    /// - [`All`] / [`FactoryDefault`] → the base is recorded and this returns
    ///   [`Accepted`]. Both name a coherent selection over the monitors the model
    ///   tracks (the CSMS-installed set is a subset of `All`; `FactoryDefault`
    ///   selects the pre-configured subset — currently empty, but a well-defined
    ///   choice), so the station records the selection.
    /// - [`HardWiredOnly`] → this is a **modeled seam**. The monitor store models
    ///   only CSMS-installed monitors and has no hard-wired-monitor seam, so the
    ///   station has no hard-wired set to activate. Rather than silently record a
    ///   base it cannot honor (which would make [`active_monitoring_base`] claim
    ///   `HardWiredOnly` is active while no hard-wired monitor exists), the
    ///   station answers [`NotSupported`] and leaves the stored base
    ///   **unchanged** — the same "a non-accept leaves state unchanged" shape as
    ///   [`set_monitoring_level`]'s out-of-range rejection. When a real hard-wired
    ///   seam lands, this arm records the base and returns `Accepted`.
    ///
    /// `base` is matched, never parsed or indexed, so no wire value can panic.
    ///
    /// [`All`]: MonitorBaseEnumType::All
    /// [`FactoryDefault`]: MonitorBaseEnumType::FactoryDefault
    /// [`HardWiredOnly`]: MonitorBaseEnumType::HardWiredOnly
    /// [`Accepted`]: GenericDeviceModelStatusEnumType::Accepted
    /// [`NotSupported`]: GenericDeviceModelStatusEnumType::NotSupported
    /// [`active_monitoring_base`]: V201DeviceModel::active_monitoring_base
    /// [`set_monitoring_level`]: V201DeviceModel::set_monitoring_level
    pub fn set_monitoring_base(
        &mut self,
        base: MonitorBaseEnumType,
    ) -> GenericDeviceModelStatusEnumType {
        match base {
            MonitorBaseEnumType::All | MonitorBaseEnumType::FactoryDefault => {
                self.active_monitoring_base = base;
                GenericDeviceModelStatusEnumType::Accepted
            }
            // Modeled seam: no hard-wired monitors are modeled, so there is
            // nothing to activate. Report `NotSupported` and leave the stored
            // base unchanged rather than claim a base the station cannot honor.
            MonitorBaseEnumType::HardWiredOnly => GenericDeviceModelStatusEnumType::NotSupported,
        }
    }

    /// The active monitoring base — which set of pre-configured monitors is
    /// selected. Defaults to [`DEFAULT_MONITORING_BASE`] ([`All`]) until a
    /// `SetMonitoringBase` narrows it; a [`NotSupported`] request leaves it
    /// unchanged.
    ///
    /// [`All`]: MonitorBaseEnumType::All
    /// [`NotSupported`]: GenericDeviceModelStatusEnumType::NotSupported
    pub fn active_monitoring_base(&self) -> MonitorBaseEnumType {
        self.active_monitoring_base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocpp_types::v201::EvseType;

    fn component(name: &str) -> ComponentType {
        ComponentType {
            name: name.to_string(),
            instance: None,
            evse: None,
            custom_data: None,
        }
    }

    fn variable(name: &str) -> VariableType {
        VariableType {
            name: name.to_string(),
            instance: None,
            custom_data: None,
        }
    }

    #[test]
    fn accepted_returns_the_seeded_value() {
        let model = V201DeviceModel::with_standard_profile();
        let (status, value) = model.get(
            &component("OCPPCommCtrlr"),
            &variable("HeartbeatInterval"),
            AttributeEnumType::Actual,
        );
        assert_eq!(status, GetVariableStatusEnumType::Accepted);
        assert_eq!(value.as_deref(), Some("300"));
    }

    #[test]
    fn unknown_component_when_no_such_component() {
        let model = V201DeviceModel::with_standard_profile();
        let (status, value) = model.get(
            &component("NoSuchCtrlr"),
            &variable("HeartbeatInterval"),
            AttributeEnumType::Actual,
        );
        assert_eq!(status, GetVariableStatusEnumType::UnknownComponent);
        assert_eq!(value, None);
    }

    #[test]
    fn unknown_variable_when_component_exists_but_variable_does_not() {
        let model = V201DeviceModel::with_standard_profile();
        let (status, value) = model.get(
            &component("OCPPCommCtrlr"),
            &variable("NoSuchVariable"),
            AttributeEnumType::Actual,
        );
        assert_eq!(status, GetVariableStatusEnumType::UnknownVariable);
        assert_eq!(value, None);
    }

    #[test]
    fn not_supported_attribute_type_when_attribute_absent() {
        let model = V201DeviceModel::with_standard_profile();
        // Seed only carries `Actual`; asking for `Target` is a supported *read*
        // that resolves to NotSupportedAttributeType, not an error.
        let (status, value) = model.get(
            &component("OCPPCommCtrlr"),
            &variable("HeartbeatInterval"),
            AttributeEnumType::Target,
        );
        assert_eq!(status, GetVariableStatusEnumType::NotSupportedAttributeType);
        assert_eq!(value, None);
    }

    #[test]
    fn component_and_variable_names_match_case_insensitively() {
        let model = V201DeviceModel::with_standard_profile();
        let (status, value) = model.get(
            &component("ocppcommctrlr"),
            &variable("heartbeatinterval"),
            AttributeEnumType::Actual,
        );
        assert_eq!(status, GetVariableStatusEnumType::Accepted);
        assert_eq!(value.as_deref(), Some("300"));
    }

    #[test]
    fn an_evse_scoped_request_does_not_match_a_station_wide_variable() {
        // A station-wide seed variable carries no EVSE; a request naming an EVSE
        // addresses a different component identity, so it is UnknownComponent —
        // not a silent fall-through to the station-wide value.
        let model = V201DeviceModel::with_standard_profile();
        let mut comp = component("OCPPCommCtrlr");
        comp.evse = Some(EvseType {
            id: 1,
            connector_id: None,
            custom_data: None,
        });
        let (status, _) = model.get(
            &comp,
            &variable("HeartbeatInterval"),
            AttributeEnumType::Actual,
        );
        assert_eq!(status, GetVariableStatusEnumType::UnknownComponent);
    }

    #[test]
    fn a_variable_instance_mismatch_is_unknown_variable() {
        let model = V201DeviceModel::with_standard_profile();
        let mut var = variable("HeartbeatInterval");
        var.instance = Some("secondary".to_string());
        let (status, _) = model.get(&component("OCPPCommCtrlr"), &var, AttributeEnumType::Actual);
        assert_eq!(status, GetVariableStatusEnumType::UnknownVariable);
    }

    #[test]
    fn set_accepts_a_writable_variable_and_get_reads_it_back() {
        let mut model = V201DeviceModel::with_standard_profile();
        let status = model.set(
            &component("OCPPCommCtrlr"),
            &variable("HeartbeatInterval"),
            AttributeEnumType::Actual,
            "600",
        );
        assert_eq!(status, SetVariableStatusEnumType::Accepted);
        // Round-trip: the written value is now the one a read returns.
        let (get_status, value) = model.get(
            &component("OCPPCommCtrlr"),
            &variable("HeartbeatInterval"),
            AttributeEnumType::Actual,
        );
        assert_eq!(get_status, GetVariableStatusEnumType::Accepted);
        assert_eq!(value.as_deref(), Some("600"));
    }

    #[test]
    fn set_rejects_a_read_only_variable_and_leaves_it_unchanged() {
        let mut model = V201DeviceModel::with_standard_profile();
        let status = model.set(
            &component("SecurityCtrlr"),
            &variable("MaxCertificateChainSize"),
            AttributeEnumType::Actual,
            "9",
        );
        assert_eq!(status, SetVariableStatusEnumType::Rejected);
        // A rejected write must not mutate the stored value.
        let (_, value) = model.get(
            &component("SecurityCtrlr"),
            &variable("MaxCertificateChainSize"),
            AttributeEnumType::Actual,
        );
        assert_eq!(value.as_deref(), Some("3"));
    }

    #[test]
    fn set_signals_reboot_required_but_applies_the_value() {
        let mut model = V201DeviceModel::with_standard_profile();
        let status = model.set(
            &component("OCPPCommCtrlr"),
            &variable("NetworkConfigurationPriority"),
            AttributeEnumType::Actual,
            "2",
        );
        assert_eq!(status, SetVariableStatusEnumType::RebootRequired);
        // RebootRequired is "accepted-and-applied" for the simulator.
        let (_, value) = model.get(
            &component("OCPPCommCtrlr"),
            &variable("NetworkConfigurationPriority"),
            AttributeEnumType::Actual,
        );
        assert_eq!(value.as_deref(), Some("2"));
    }

    #[test]
    fn set_unknown_component_is_unknown_component() {
        let mut model = V201DeviceModel::with_standard_profile();
        let status = model.set(
            &component("NoSuchCtrlr"),
            &variable("HeartbeatInterval"),
            AttributeEnumType::Actual,
            "1",
        );
        assert_eq!(status, SetVariableStatusEnumType::UnknownComponent);
    }

    #[test]
    fn set_unknown_variable_is_unknown_variable() {
        let mut model = V201DeviceModel::with_standard_profile();
        let status = model.set(
            &component("OCPPCommCtrlr"),
            &variable("NoSuchVariable"),
            AttributeEnumType::Actual,
            "1",
        );
        assert_eq!(status, SetVariableStatusEnumType::UnknownVariable);
    }

    #[test]
    fn set_unsupported_attribute_is_not_supported_attribute_type() {
        // The seed carries only `Actual`; writing `Target` addresses an attribute
        // the variable does not expose.
        let mut model = V201DeviceModel::with_standard_profile();
        let status = model.set(
            &component("OCPPCommCtrlr"),
            &variable("HeartbeatInterval"),
            AttributeEnumType::Target,
            "600",
        );
        assert_eq!(status, SetVariableStatusEnumType::NotSupportedAttributeType);
    }

    #[test]
    fn set_matches_component_and_variable_names_case_insensitively() {
        let mut model = V201DeviceModel::with_standard_profile();
        let status = model.set(
            &component("ocppcommctrlr"),
            &variable("heartbeatinterval"),
            AttributeEnumType::Actual,
            "600",
        );
        assert_eq!(status, SetVariableStatusEnumType::Accepted);
    }

    /// Find the single report entry for a component/variable name pair.
    fn find<'a>(
        report: &'a [ReportDataType],
        component: &str,
        variable: &str,
    ) -> Option<&'a ReportDataType> {
        report
            .iter()
            .find(|d| d.component.name == component && d.variable.name == variable)
    }

    #[test]
    fn report_full_inventory_lists_seeded_variables_in_display_casing() {
        let model = V201DeviceModel::with_standard_profile();
        let report = model.report(ReportBaseEnumType::FullInventory);

        // The report reproduces the CSMS-visible casing, not the normalized key.
        let heartbeat = find(&report, "OCPPCommCtrlr", "HeartbeatInterval")
            .expect("FullInventory includes the heartbeat interval in its display casing");
        assert_eq!(heartbeat.variable_attribute.len(), 1);
        let attr = &heartbeat.variable_attribute[0];
        assert_eq!(attr.kind, Some(AttributeEnumType::Actual));
        assert_eq!(attr.value.as_deref(), Some("300"));
        assert_eq!(attr.mutability, Some(MutabilityEnumType::ReadWrite));
        // No typed characteristics modeled yet.
        assert!(heartbeat.variable_characteristics.is_none());

        // FullInventory includes read-only capability constants too.
        assert!(
            find(&report, "SecurityCtrlr", "MaxCertificateChainSize").is_some(),
            "FullInventory includes read-only variables"
        );
    }

    #[test]
    fn report_read_only_variable_reports_read_only_mutability() {
        let model = V201DeviceModel::with_standard_profile();
        let report = model.report(ReportBaseEnumType::FullInventory);
        let max_chain = find(&report, "SecurityCtrlr", "MaxCertificateChainSize")
            .expect("read-only constant is in the full inventory");
        assert_eq!(
            max_chain.variable_attribute[0].mutability,
            Some(MutabilityEnumType::ReadOnly),
            "a read-only variable reports ReadOnly mutability"
        );
    }

    #[test]
    fn report_configuration_inventory_excludes_read_only_variables() {
        let model = V201DeviceModel::with_standard_profile();
        let full = model.report(ReportBaseEnumType::FullInventory);
        let config = model.report(ReportBaseEnumType::ConfigurationInventory);

        // The read-only capability constant is the only non-writable seed entry,
        // so configuration is exactly the full inventory minus it.
        assert!(
            find(&config, "SecurityCtrlr", "MaxCertificateChainSize").is_none(),
            "ConfigurationInventory omits the read-only MaxCertificateChainSize"
        );
        assert!(
            find(&config, "OCPPCommCtrlr", "HeartbeatInterval").is_some(),
            "ConfigurationInventory keeps writable variables"
        );
        assert_eq!(
            config.len(),
            full.len() - 1,
            "configuration is the full inventory minus the one read-only variable"
        );
    }

    #[test]
    fn report_configuration_includes_reboot_required_variable() {
        // A reboot-required variable is still writable, so it belongs to the
        // configuration inventory (only `read_only` variables are excluded).
        let model = V201DeviceModel::with_standard_profile();
        let config = model.report(ReportBaseEnumType::ConfigurationInventory);
        let net = find(&config, "OCPPCommCtrlr", "NetworkConfigurationPriority")
            .expect("reboot-required variable is writable, so it is in the configuration report");
        assert_eq!(
            net.variable_attribute[0].mutability,
            Some(MutabilityEnumType::ReadWrite),
        );
    }

    #[test]
    fn report_summary_inventory_is_empty() {
        let model = V201DeviceModel::with_standard_profile();
        assert!(
            model
                .report(ReportBaseEnumType::SummaryInventory)
                .is_empty(),
            "a freshly-booted simulator has nothing noteworthy to summarize"
        );
    }

    #[test]
    fn report_is_deterministically_ordered() {
        let model = V201DeviceModel::with_standard_profile();
        let first = model.report(ReportBaseEnumType::FullInventory);
        let second = model.report(ReportBaseEnumType::FullInventory);
        assert_eq!(first, second, "report ordering is stable across calls");

        // Sorted by component name then variable name.
        let keys: Vec<(&str, &str)> = first
            .iter()
            .map(|d| (d.component.name.as_str(), d.variable.name.as_str()))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted, "report is sorted by component then variable");
    }

    #[test]
    fn report_reflects_a_written_value() {
        // A SetVariables write is visible to a subsequent report.
        let mut model = V201DeviceModel::with_standard_profile();
        model.set(
            &component("OCPPCommCtrlr"),
            &variable("HeartbeatInterval"),
            AttributeEnumType::Actual,
            "600",
        );
        let report = model.report(ReportBaseEnumType::FullInventory);
        let heartbeat = find(&report, "OCPPCommCtrlr", "HeartbeatInterval").unwrap();
        assert_eq!(
            heartbeat.variable_attribute[0].value.as_deref(),
            Some("600")
        );
    }

    // --- report_filtered (GetReport selection) -----------------------------

    /// A `componentVariable` filter entry: a component, optionally narrowed to a
    /// single variable.
    fn component_variable(component: &str, variable: Option<&str>) -> ComponentVariableType {
        ComponentVariableType {
            component: ComponentType {
                name: component.to_string(),
                instance: None,
                evse: None,
                custom_data: None,
            },
            variable: variable.map(|v| VariableType {
                name: v.to_string(),
                instance: None,
                custom_data: None,
            }),
            custom_data: None,
        }
    }

    /// How many `FullInventory` rows belong to a given component — the expected
    /// size of a whole-component `componentVariable` narrowing.
    fn full_inventory_component_count(model: &V201DeviceModel, component: &str) -> usize {
        model
            .report(ReportBaseEnumType::FullInventory)
            .iter()
            .filter(|d| d.component.name == component)
            .count()
    }

    #[test]
    fn report_filtered_no_filters_matches_full_inventory() {
        // Both selectors absent = no narrowing on either axis = FullInventory.
        let model = V201DeviceModel::with_standard_profile();
        assert_eq!(
            model.report_filtered(None, None),
            model.report(ReportBaseEnumType::FullInventory),
            "an unfiltered GetReport reports the whole inventory, in the same order"
        );
    }

    #[test]
    fn report_filtered_empty_lists_do_not_narrow() {
        // Defensive: an empty (rather than absent) list is treated as "no such
        // filter" — the schema requires ≥1 item when the field is present.
        let model = V201DeviceModel::with_standard_profile();
        let full = model.report(ReportBaseEnumType::FullInventory);
        assert_eq!(model.report_filtered(Some(&[]), None), full);
        assert_eq!(model.report_filtered(None, Some(&[])), full);
    }

    #[test]
    fn report_filtered_narrows_to_a_named_component() {
        // A whole-component filter (no `variable`) returns exactly that
        // component's variables — every row, and only those.
        let model = V201DeviceModel::with_standard_profile();
        let filter = [component_variable("OCPPCommCtrlr", None)];
        let report = model.report_filtered(Some(&filter), None);

        assert!(!report.is_empty());
        assert!(
            report.iter().all(|d| d.component.name == "OCPPCommCtrlr"),
            "a component filter admits only that component's variables"
        );
        assert_eq!(
            report.len(),
            full_inventory_component_count(&model, "OCPPCommCtrlr"),
            "a whole-component filter returns every variable under it"
        );
        assert!(find(&report, "OCPPCommCtrlr", "HeartbeatInterval").is_some());
    }

    #[test]
    fn report_filtered_narrows_to_a_single_variable() {
        // A component+variable filter returns exactly that one row.
        let model = V201DeviceModel::with_standard_profile();
        let filter = [component_variable(
            "OCPPCommCtrlr",
            Some("HeartbeatInterval"),
        )];
        let report = model.report_filtered(Some(&filter), None);

        assert_eq!(
            report.len(),
            1,
            "a single component-variable selects one row"
        );
        assert_eq!(report[0].component.name, "OCPPCommCtrlr");
        assert_eq!(report[0].variable.name, "HeartbeatInterval");
    }

    #[test]
    fn report_filtered_matches_any_of_several_entries() {
        // Filter entries OR together: two components → both components' rows.
        let model = V201DeviceModel::with_standard_profile();
        let filter = [
            component_variable("OCPPCommCtrlr", Some("HeartbeatInterval")),
            component_variable("SecurityCtrlr", Some("OrganizationName")),
        ];
        let report = model.report_filtered(Some(&filter), None);

        assert_eq!(report.len(), 2);
        assert!(find(&report, "OCPPCommCtrlr", "HeartbeatInterval").is_some());
        assert!(find(&report, "SecurityCtrlr", "OrganizationName").is_some());
    }

    #[test]
    fn report_filtered_unknown_component_is_empty() {
        // A component the flat seed does not hold matches nothing → the handler
        // maps this to EmptyResultSet. (Trust boundary: arbitrary wire text
        // never panics, it simply misses.)
        let model = V201DeviceModel::with_standard_profile();
        let filter = [component_variable("NoSuchController", None)];
        assert!(model.report_filtered(Some(&filter), None).is_empty());
    }

    #[test]
    fn report_filtered_evse_scoped_component_misses_the_station_wide_seed() {
        // The seed is station-wide (no EVSE); an EVSE-scoped filter matches
        // nothing, because EVSE ids are compared exactly.
        let model = V201DeviceModel::with_standard_profile();
        let mut cv = component_variable("OCPPCommCtrlr", Some("HeartbeatInterval"));
        cv.component.evse = Some(EvseType {
            id: 1,
            connector_id: None,
            custom_data: None,
        });
        assert!(model.report_filtered(Some(&[cv]), None).is_empty());
    }

    #[test]
    fn report_filtered_matches_case_insensitively() {
        // Names/instances are case-insensitive (same normalization as
        // GetVariables), so lowercased identities still resolve.
        let model = V201DeviceModel::with_standard_profile();
        let filter = [component_variable(
            "ocppcommctrlr",
            Some("heartbeatinterval"),
        )];
        let report = model.report_filtered(Some(&filter), None);
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].variable.name, "HeartbeatInterval");
    }

    #[test]
    fn report_filtered_criteria_active_admits_all() {
        // On the healthy seed every reportable variable is Active, so [Active]
        // narrows nothing.
        let model = V201DeviceModel::with_standard_profile();
        assert_eq!(
            model.report_filtered(None, Some(&[ComponentCriterionEnumType::Active])),
            model.report(ReportBaseEnumType::FullInventory),
        );
    }

    #[test]
    fn report_filtered_criteria_problem_is_empty() {
        // No seeded variable is in a Problem state → [Problem] matches nothing.
        let model = V201DeviceModel::with_standard_profile();
        assert!(model
            .report_filtered(None, Some(&[ComponentCriterionEnumType::Problem]))
            .is_empty());
    }

    #[test]
    fn report_filtered_criteria_are_conjunctive() {
        // Criteria AND together: a list containing Problem fails even though
        // Active holds, so [Active, Problem] → empty on a healthy station.
        let model = V201DeviceModel::with_standard_profile();
        assert!(model
            .report_filtered(
                None,
                Some(&[
                    ComponentCriterionEnumType::Active,
                    ComponentCriterionEnumType::Problem,
                ]),
            )
            .is_empty());
        // Available + Enabled both hold → admits everything.
        assert_eq!(
            model.report_filtered(
                None,
                Some(&[
                    ComponentCriterionEnumType::Available,
                    ComponentCriterionEnumType::Enabled,
                ]),
            ),
            model.report(ReportBaseEnumType::FullInventory),
        );
    }

    #[test]
    fn report_filtered_combines_component_and_criteria() {
        // The two selectors AND: a component narrowing that still satisfies the
        // criteria returns that component's rows; the same narrowing with a
        // Problem criterion returns nothing.
        let model = V201DeviceModel::with_standard_profile();
        let filter = [component_variable("OCPPCommCtrlr", None)];

        let admitted =
            model.report_filtered(Some(&filter), Some(&[ComponentCriterionEnumType::Active]));
        assert_eq!(
            admitted.len(),
            full_inventory_component_count(&model, "OCPPCommCtrlr"),
        );
        assert!(admitted.iter().all(|d| d.component.name == "OCPPCommCtrlr"));

        assert!(
            model
                .report_filtered(Some(&filter), Some(&[ComponentCriterionEnumType::Problem]))
                .is_empty(),
            "a Problem criterion empties even a matching component narrowing"
        );
    }

    #[test]
    fn report_filtered_is_deterministically_ordered() {
        // A filtered report is stable and sorted, exactly like report().
        let model = V201DeviceModel::with_standard_profile();
        let filter = [
            component_variable("SecurityCtrlr", None),
            component_variable("OCPPCommCtrlr", None),
        ];
        let first = model.report_filtered(Some(&filter), None);
        let second = model.report_filtered(Some(&filter), None);
        assert_eq!(first, second, "filtered report ordering is stable");

        let keys: Vec<(&str, &str)> = first
            .iter()
            .map(|d| (d.component.name.as_str(), d.variable.name.as_str()))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(
            keys, sorted,
            "filtered report is sorted by component then variable"
        );
    }

    // --- install_monitors + monitoring_snapshot (SetVariableMonitoring /  ---
    // --- GetMonitoringReport, issue #494) ----------------------------------

    fn monitor_data(
        component_name: &str,
        variable_name: &str,
        kind: MonitorEnumType,
        value: f64,
        severity: i32,
    ) -> SetMonitoringDataType {
        SetMonitoringDataType {
            id: None,
            transaction: None,
            value,
            kind,
            severity,
            component: component(component_name),
            variable: variable(variable_name),
            custom_data: None,
        }
    }

    #[test]
    fn empty_snapshot_when_no_monitor_installed() {
        // A freshly-seeded model has no monitors, so the snapshot is empty
        // regardless of the filters — no filter, a component-variable filter, and
        // a criteria filter all report nothing. The empty snapshot drives the
        // `EmptyResultSet` status on the wire (no regression to #493's path).
        let model = V201DeviceModel::with_standard_profile();
        assert!(model.monitoring_snapshot(None, None).is_empty());
        let cv = [component_variable("OCPPCommCtrlr", None)];
        assert!(model.monitoring_snapshot(Some(&cv), None).is_empty());
        assert!(model
            .monitoring_snapshot(
                None,
                Some(&[MonitoringCriterionEnumType::ThresholdMonitoring])
            )
            .is_empty());
    }

    #[test]
    fn install_accepts_and_snapshot_reports_the_monitor() {
        let mut model = V201DeviceModel::with_standard_profile();
        let results = model.install_monitors(&[monitor_data(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
            MonitorEnumType::UpperThreshold,
            600.0,
            5,
        )]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, SetMonitoringStatusEnumType::Accepted);
        // A new monitor gets a station-assigned id (the first is 1).
        assert_eq!(results[0].id, Some(1));

        let snapshot = model.monitoring_snapshot(None, None);
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].component.name, "OCPPCommCtrlr");
        assert_eq!(snapshot[0].variable.name, "HeartbeatInterval");
        assert_eq!(snapshot[0].variable_monitoring.len(), 1);
        let m = &snapshot[0].variable_monitoring[0];
        assert_eq!(m.id, 1);
        assert_eq!(m.kind, MonitorEnumType::UpperThreshold);
        assert_eq!(m.value, 600.0);
        assert_eq!(m.severity, 5);
        assert!(!m.transaction);
    }

    #[test]
    fn install_rejects_unknown_component_and_variable() {
        let mut model = V201DeviceModel::with_standard_profile();
        let results = model.install_monitors(&[
            monitor_data(
                "NoSuchCtrlr",
                "HeartbeatInterval",
                MonitorEnumType::Delta,
                1.0,
                3,
            ),
            monitor_data(
                "OCPPCommCtrlr",
                "NoSuchVariable",
                MonitorEnumType::Delta,
                1.0,
                3,
            ),
        ]);
        assert_eq!(
            results[0].status,
            SetMonitoringStatusEnumType::UnknownComponent
        );
        assert_eq!(results[0].id, None);
        assert_eq!(
            results[1].status,
            SetMonitoringStatusEnumType::UnknownVariable
        );
        assert_eq!(results[1].id, None);
        // Nothing was installed.
        assert!(model.monitoring_snapshot(None, None).is_empty());
    }

    #[test]
    fn identical_reinstall_is_a_duplicate() {
        let mut model = V201DeviceModel::with_standard_profile();
        let data = monitor_data(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
            MonitorEnumType::UpperThreshold,
            600.0,
            5,
        );
        assert_eq!(
            model.install_monitors(std::slice::from_ref(&data))[0].status,
            SetMonitoringStatusEnumType::Accepted
        );
        let again = model.install_monitors(&[data]);
        assert_eq!(again[0].status, SetMonitoringStatusEnumType::Duplicate);
        assert_eq!(again[0].id, None);
        // Only the first monitor exists.
        assert_eq!(
            model.monitoring_snapshot(None, None)[0]
                .variable_monitoring
                .len(),
            1
        );
    }

    #[test]
    fn different_severity_on_same_variable_is_not_a_duplicate() {
        // Two threshold monitors on one variable at different severities (e.g. a
        // warning and a critical alarm) coexist — not a duplicate.
        let mut model = V201DeviceModel::with_standard_profile();
        model.install_monitors(&[monitor_data(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
            MonitorEnumType::UpperThreshold,
            600.0,
            5,
        )]);
        let second = model.install_monitors(&[monitor_data(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
            MonitorEnumType::UpperThreshold,
            600.0,
            1,
        )]);
        assert_eq!(second[0].status, SetMonitoringStatusEnumType::Accepted);
        assert_eq!(second[0].id, Some(2));
        let snapshot = model.monitoring_snapshot(None, None);
        // Both monitors group under the one component-variable, id-sorted.
        assert_eq!(snapshot.len(), 1);
        let ids: Vec<i32> = snapshot[0]
            .variable_monitoring
            .iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn replace_by_id_updates_in_place_and_keeps_the_id() {
        let mut model = V201DeviceModel::with_standard_profile();
        let id = model.install_monitors(&[monitor_data(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
            MonitorEnumType::UpperThreshold,
            600.0,
            5,
        )])[0]
            .id
            .unwrap();

        let mut replacement = monitor_data(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
            MonitorEnumType::UpperThreshold,
            900.0,
            2,
        );
        replacement.id = Some(id);
        let result = model.install_monitors(&[replacement]);
        assert_eq!(result[0].status, SetMonitoringStatusEnumType::Accepted);
        assert_eq!(result[0].id, Some(id));

        let snapshot = model.monitoring_snapshot(None, None);
        assert_eq!(
            snapshot[0].variable_monitoring.len(),
            1,
            "replaced, not added"
        );
        let m = &snapshot[0].variable_monitoring[0];
        assert_eq!(m.id, id);
        assert_eq!(m.value, 900.0);
        assert_eq!(m.severity, 2);
    }

    #[test]
    fn replace_of_unknown_id_is_rejected() {
        let mut model = V201DeviceModel::with_standard_profile();
        let mut data = monitor_data(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
            MonitorEnumType::Delta,
            1.0,
            3,
        );
        data.id = Some(999);
        let result = model.install_monitors(&[data]);
        assert_eq!(result[0].status, SetMonitoringStatusEnumType::Rejected);
        assert_eq!(result[0].id, None);
        assert!(model.monitoring_snapshot(None, None).is_empty());
    }

    #[test]
    fn snapshot_filters_by_component_variable() {
        let mut model = V201DeviceModel::with_standard_profile();
        model.install_monitors(&[
            monitor_data(
                "OCPPCommCtrlr",
                "HeartbeatInterval",
                MonitorEnumType::UpperThreshold,
                600.0,
                5,
            ),
            monitor_data(
                "AlignedDataCtrlr",
                "Interval",
                MonitorEnumType::Delta,
                10.0,
                5,
            ),
        ]);
        let filter = [component_variable("AlignedDataCtrlr", Some("Interval"))];
        let snapshot = model.monitoring_snapshot(Some(&filter), None);
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].component.name, "AlignedDataCtrlr");
        assert_eq!(snapshot[0].variable.name, "Interval");
    }

    #[test]
    fn snapshot_filters_by_monitoring_criteria() {
        let mut model = V201DeviceModel::with_standard_profile();
        model.install_monitors(&[
            monitor_data(
                "OCPPCommCtrlr",
                "HeartbeatInterval",
                MonitorEnumType::UpperThreshold,
                600.0,
                5,
            ),
            monitor_data(
                "AlignedDataCtrlr",
                "Interval",
                MonitorEnumType::Delta,
                10.0,
                5,
            ),
            monitor_data(
                "SampledDataCtrlr",
                "TxUpdatedInterval",
                MonitorEnumType::Periodic,
                30.0,
                5,
            ),
        ]);
        // ThresholdMonitoring keeps only the UpperThreshold monitor.
        let threshold = model.monitoring_snapshot(
            None,
            Some(&[MonitoringCriterionEnumType::ThresholdMonitoring]),
        );
        assert_eq!(threshold.len(), 1);
        assert_eq!(threshold[0].variable.name, "HeartbeatInterval");
        // DeltaMonitoring OR PeriodicMonitoring keeps the other two.
        let delta_or_periodic = model.monitoring_snapshot(
            None,
            Some(&[
                MonitoringCriterionEnumType::DeltaMonitoring,
                MonitoringCriterionEnumType::PeriodicMonitoring,
            ]),
        );
        assert_eq!(delta_or_periodic.len(), 2);
    }

    #[test]
    fn snapshot_is_deterministically_ordered() {
        let mut model = V201DeviceModel::with_standard_profile();
        model.install_monitors(&[
            monitor_data(
                "SampledDataCtrlr",
                "TxUpdatedInterval",
                MonitorEnumType::Periodic,
                30.0,
                5,
            ),
            monitor_data(
                "AlignedDataCtrlr",
                "Interval",
                MonitorEnumType::Delta,
                10.0,
                5,
            ),
        ]);
        let first = model.monitoring_snapshot(None, None);
        let second = model.monitoring_snapshot(None, None);
        assert_eq!(first, second, "snapshot ordering is stable");
        let names: Vec<&str> = first.iter().map(|d| d.component.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["AlignedDataCtrlr", "SampledDataCtrlr"],
            "sorted by component name"
        );
    }

    // --- clear_monitors (ClearVariableMonitoring, issue #497) --------------

    #[test]
    fn clear_removes_the_monitor_and_it_vanishes_from_the_snapshot() {
        let mut model = V201DeviceModel::with_standard_profile();
        let id = model.install_monitors(&[monitor_data(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
            MonitorEnumType::UpperThreshold,
            600.0,
            5,
        )])[0]
            .id
            .unwrap();
        assert_eq!(model.monitoring_snapshot(None, None).len(), 1);

        let results = model.clear_monitors(&[id]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, ClearMonitoringStatusEnumType::Accepted);
        assert_eq!(results[0].id, id);
        // The cleared monitor no longer shows up in a subsequent report.
        assert!(model.monitoring_snapshot(None, None).is_empty());
    }

    #[test]
    fn clear_unknown_id_is_not_found() {
        let mut model = V201DeviceModel::with_standard_profile();
        let results = model.clear_monitors(&[999]);
        assert_eq!(results[0].status, ClearMonitoringStatusEnumType::NotFound);
        assert_eq!(results[0].id, 999);
    }

    #[test]
    fn clear_mixed_batch_preserves_request_order() {
        let mut model = V201DeviceModel::with_standard_profile();
        let live = model.install_monitors(&[monitor_data(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
            MonitorEnumType::UpperThreshold,
            600.0,
            5,
        )])[0]
            .id
            .unwrap();

        // Unknown, then live, then unknown — the results echo each requested id
        // in the order asked, so the CSMS can correlate row-by-row.
        let results = model.clear_monitors(&[404, live, 405]);
        assert_eq!(
            results.iter().map(|r| (r.id, r.status)).collect::<Vec<_>>(),
            vec![
                (404, ClearMonitoringStatusEnumType::NotFound),
                (live, ClearMonitoringStatusEnumType::Accepted),
                (405, ClearMonitoringStatusEnumType::NotFound),
            ]
        );
        assert!(model.monitoring_snapshot(None, None).is_empty());
    }

    #[test]
    fn clear_duplicate_id_in_one_batch_is_accepted_then_not_found() {
        // The same id twice in one request: the first occurrence removes it,
        // the second finds nothing — idempotent, never a panic.
        let mut model = V201DeviceModel::with_standard_profile();
        let id = model.install_monitors(&[monitor_data(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
            MonitorEnumType::UpperThreshold,
            600.0,
            5,
        )])[0]
            .id
            .unwrap();
        let results = model.clear_monitors(&[id, id]);
        assert_eq!(results[0].status, ClearMonitoringStatusEnumType::Accepted);
        assert_eq!(results[1].status, ClearMonitoringStatusEnumType::NotFound);
    }

    #[test]
    fn cleared_id_is_not_reused_by_a_later_install() {
        // Clearing a monitor does not rewind the monotonic id counter, so the
        // next install gets a fresh id rather than reusing the cleared one.
        let mut model = V201DeviceModel::with_standard_profile();
        let first = model.install_monitors(&[monitor_data(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
            MonitorEnumType::UpperThreshold,
            600.0,
            5,
        )])[0]
            .id
            .unwrap();
        assert_eq!(
            model.clear_monitors(&[first])[0].status,
            ClearMonitoringStatusEnumType::Accepted
        );
        let second = model.install_monitors(&[monitor_data(
            "OCPPCommCtrlr",
            "HeartbeatInterval",
            MonitorEnumType::Delta,
            10.0,
            3,
        )])[0]
            .id
            .unwrap();
        assert_ne!(second, first, "the cleared id is never handed out again");
        assert!(second > first);
    }

    // --- SetMonitoringLevel: the reporting-level setter (#500) --------------

    #[test]
    fn monitoring_level_defaults_to_most_permissive() {
        // A fresh station suppresses nothing: the default is the most permissive
        // level (9, Debug), not the numeric zero a derived Default would give
        // (0 = Danger, which would hide every less-severe monitor).
        let model = V201DeviceModel::new();
        assert_eq!(model.active_monitoring_level(), DEFAULT_MONITORING_LEVEL);
        assert_eq!(model.active_monitoring_level(), 9);
        // The standard profile inherits the same default.
        assert_eq!(
            V201DeviceModel::with_standard_profile().active_monitoring_level(),
            9
        );
    }

    #[test]
    fn set_monitoring_level_accepts_in_range_and_stores_it() {
        let mut model = V201DeviceModel::new();
        // A representative mid-range level.
        assert!(model.set_monitoring_level(5));
        assert_eq!(model.active_monitoring_level(), 5);
        // Both inclusive bounds are accepted.
        assert!(model.set_monitoring_level(0));
        assert_eq!(model.active_monitoring_level(), 0);
        assert!(model.set_monitoring_level(9));
        assert_eq!(model.active_monitoring_level(), 9);
    }

    #[test]
    fn set_monitoring_level_replaces_the_previous_level() {
        let mut model = V201DeviceModel::new();
        assert!(model.set_monitoring_level(7));
        assert!(model.set_monitoring_level(2));
        assert_eq!(model.active_monitoring_level(), 2, "the last set wins");
    }

    #[test]
    fn set_monitoring_level_rejects_out_of_range_and_leaves_state_unchanged() {
        let mut model = V201DeviceModel::new();
        // Seed a known-good level so we can prove a rejected set does not clobber
        // it.
        assert!(model.set_monitoring_level(4));
        for bad in [-1, 10, 100, i32::MIN, i32::MAX] {
            assert!(
                !model.set_monitoring_level(bad),
                "severity {bad} is out of 0..=9 and must be rejected",
            );
            assert_eq!(
                model.active_monitoring_level(),
                4,
                "a rejected set leaves the stored level unchanged",
            );
        }
    }

    // --- SetMonitoringBase: the active-base selector (#501) -----------------

    #[test]
    fn monitoring_base_defaults_to_all() {
        // A fresh station has every monitor in scope: the default base is `All`,
        // not the first-declared variant a derive might imply.
        let model = V201DeviceModel::new();
        assert_eq!(model.active_monitoring_base(), DEFAULT_MONITORING_BASE);
        assert_eq!(model.active_monitoring_base(), MonitorBaseEnumType::All);
        // The standard profile inherits the same default.
        assert_eq!(
            V201DeviceModel::with_standard_profile().active_monitoring_base(),
            MonitorBaseEnumType::All,
        );
    }

    #[test]
    fn set_monitoring_base_all_and_factory_default_are_accepted_and_recorded() {
        let mut model = V201DeviceModel::new();
        // `FactoryDefault` is recorded and accepted.
        assert_eq!(
            model.set_monitoring_base(MonitorBaseEnumType::FactoryDefault),
            GenericDeviceModelStatusEnumType::Accepted,
        );
        assert_eq!(
            model.active_monitoring_base(),
            MonitorBaseEnumType::FactoryDefault,
        );
        // `All` is recorded and accepted, replacing the previous selection.
        assert_eq!(
            model.set_monitoring_base(MonitorBaseEnumType::All),
            GenericDeviceModelStatusEnumType::Accepted,
        );
        assert_eq!(model.active_monitoring_base(), MonitorBaseEnumType::All);
    }

    #[test]
    fn set_monitoring_base_hard_wired_only_is_not_supported_and_leaves_state_unchanged() {
        let mut model = V201DeviceModel::new();
        // Seed a known-good base so we can prove the modeled `HardWiredOnly` seam
        // does not clobber it.
        assert_eq!(
            model.set_monitoring_base(MonitorBaseEnumType::FactoryDefault),
            GenericDeviceModelStatusEnumType::Accepted,
        );
        assert_eq!(
            model.set_monitoring_base(MonitorBaseEnumType::HardWiredOnly),
            GenericDeviceModelStatusEnumType::NotSupported,
            "no hard-wired monitors are modeled, so HardWiredOnly is NotSupported",
        );
        assert_eq!(
            model.active_monitoring_base(),
            MonitorBaseEnumType::FactoryDefault,
            "a NotSupported base leaves the stored selection unchanged",
        );
    }
}
