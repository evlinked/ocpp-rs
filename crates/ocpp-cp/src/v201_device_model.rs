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
    AttributeEnumType, ComponentType, GetVariableStatusEnumType, SetVariableStatusEnumType,
    VariableType,
};
use std::collections::{HashMap, HashSet};

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

/// One variable's stored state: its attribute values plus its write policy.
///
/// A variable exposes at most the four `AttributeEnumType`s, so a small `Vec`
/// (linear scan) is cheaper and simpler than a map — and `AttributeEnumType` is
/// not `Hash`.
#[derive(Debug, Default, Clone)]
struct VariableEntry {
    attributes: Vec<(AttributeEnumType, String)>,
    policy: WritePolicy,
}

/// The Charge Point simulator's OCPP 2.0.1 device model: the attribute values a
/// CSMS can read via `GetVariables` and write via `SetVariables`.
///
/// `components` is tracked separately from `variables` so a read/write can tell
/// an *unknown component* apart from a *known component with an unknown
/// variable* — the two map to distinct status outcomes.
#[derive(Debug, Default, Clone)]
pub struct V201DeviceModel {
    /// Every component identity the model knows about (even those whose
    /// requested variable is absent), for the `UnknownComponent` distinction.
    components: HashSet<ComponentKey>,
    /// Per-variable stored attribute values and write policy.
    variables: HashMap<VariableKey, VariableEntry>,
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
        let entry = self.variables.entry(variable_key).or_default();
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
}
