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
//! This module is the **read** seam consumed by the `GetVariables` handler; the
//! `SetVariables` write seam is a follow-up (it will reuse this store).
//!
//! Ports the data the CSMS reads via
//! [`ocpp.v201.call.GetVariables`](https://github.com/mobilityhouse/ocpp/blob/master/ocpp/v201/call.py)
//! — the message types and their FINAL JSON Schema already live in
//! `ocpp-messages`; this is the Charging Station's answering behavior.
//!
//! **Case-insensitivity.** Per the 2.0.1 spec, component and variable *names*
//! (and instances) are case-insensitive, so lookups normalize them to
//! lowercase. EVSE ids are numeric and matched exactly. The store keeps only
//! normalized keys; the `GetVariables` handler echoes the CSMS's original
//! (un-normalized) `component` / `variable` back on each result, as the schema
//! requires.

use ocpp_types::v201::{AttributeEnumType, ComponentType, GetVariableStatusEnumType, VariableType};
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

/// The Charge Point simulator's OCPP 2.0.1 device model: the attribute values a
/// CSMS can read via `GetVariables`.
///
/// `components` is tracked separately from `variables` so a read can tell an
/// *unknown component* apart from a *known component with an unknown variable*
/// — the two map to distinct [`GetVariableStatusEnumType`] outcomes.
#[derive(Debug, Default, Clone)]
pub struct V201DeviceModel {
    /// Every component identity the model knows about (even those whose
    /// requested variable is absent), for the `UnknownComponent` distinction.
    components: HashSet<ComponentKey>,
    /// Attribute values per variable. A variable exposes at most the four
    /// `AttributeEnumType`s, so a small `Vec` (linear scan) is cheaper and
    /// simpler than a map — and `AttributeEnumType` is not `Hash`.
    variables: HashMap<VariableKey, Vec<(AttributeEnumType, String)>>,
}

impl V201DeviceModel {
    /// An empty device model.
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a station-wide (no instance, no EVSE) variable attribute. Used by
    /// [`with_standard_profile`](Self::with_standard_profile) and available for
    /// tests / future wiring.
    pub fn set_station_variable(
        &mut self,
        component: &str,
        variable: &str,
        attribute: AttributeEnumType,
        value: impl Into<String>,
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
        let attributes = self.variables.entry(variable_key).or_default();
        let value = value.into();
        match attributes.iter_mut().find(|(a, _)| *a == attribute) {
            Some(slot) => slot.1 = value,
            None => attributes.push((attribute, value)),
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
        let Some(attributes) = self.variables.get(&variable_key) else {
            return (GetVariableStatusEnumType::UnknownVariable, None);
        };
        match attributes.iter().find(|(a, _)| *a == attribute) {
            Some((_, value)) => (GetVariableStatusEnumType::Accepted, Some(value.clone())),
            None => (GetVariableStatusEnumType::NotSupportedAttributeType, None),
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
}
