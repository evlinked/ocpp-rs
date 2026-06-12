//! OCPP configuration key-value store.
//!
//! `ConfigurationStore` holds the mutable OCPP configuration keys (e.g.
//! `HeartbeatInterval`, `MeterValueSampleInterval`) and enforces read-only
//! protection on certain keys.  It is shared between the `ChargePoint` and the
//! default `ChangeConfiguration` / `GetConfiguration` handlers registered in
//! `ActionDispatcher`.

use std::collections::HashMap;

use anyhow::Result;

use crate::error::ChargePointError;

/// Configuration key-value store
#[derive(Debug, Clone)]
pub struct ConfigurationStore {
    keys: HashMap<String, String>,
    readonly_keys: std::collections::HashSet<String>,
}

impl ConfigurationStore {
    /// Create new configuration store with OCPP 1.6J default values.
    pub fn new() -> Self {
        let mut keys = HashMap::new();
        let mut readonly_keys = std::collections::HashSet::new();

        keys.insert("AuthorizeRemoteTxRequests".to_string(), "true".to_string());
        keys.insert("ClockAlignedDataInterval".to_string(), "0".to_string());
        keys.insert("ConnectionTimeOut".to_string(), "60".to_string());
        keys.insert("GetConfigurationMaxKeys".to_string(), "100".to_string());
        keys.insert("HeartbeatInterval".to_string(), "86400".to_string());
        keys.insert("LocalAuthListEnabled".to_string(), "false".to_string());
        keys.insert("LocalAuthListMaxLength".to_string(), "0".to_string());
        keys.insert("LocalAuthorizeOffline".to_string(), "true".to_string());
        keys.insert("LocalPreAuthorize".to_string(), "false".to_string());
        keys.insert("MeterValuesAlignedData".to_string(), "".to_string());
        keys.insert(
            "MeterValuesSampledData".to_string(),
            "Energy.Active.Import.Register".to_string(),
        );
        keys.insert("MeterValueSampleInterval".to_string(), "60".to_string());
        keys.insert("NumberOfConnectors".to_string(), "2".to_string());
        keys.insert("ResetRetries".to_string(), "3".to_string());
        keys.insert(
            "ConnectorPhaseRotation".to_string(),
            "NotApplicable".to_string(),
        );
        keys.insert(
            "StopTransactionOnEVSideDisconnect".to_string(),
            "true".to_string(),
        );
        keys.insert("StopTransactionOnInvalidId".to_string(), "true".to_string());
        keys.insert("StopTxnAlignedData".to_string(), "".to_string());
        keys.insert("StopTxnSampledData".to_string(), "".to_string());
        keys.insert("SupportedFeatureProfiles".to_string(), "Core".to_string());
        keys.insert("TransactionMessageAttempts".to_string(), "3".to_string());
        keys.insert(
            "TransactionMessageRetryInterval".to_string(),
            "60".to_string(),
        );
        keys.insert(
            "UnlockConnectorOnEVSideDisconnect".to_string(),
            "true".to_string(),
        );

        readonly_keys.insert("NumberOfConnectors".to_string());
        readonly_keys.insert("SupportedFeatureProfiles".to_string());

        Self {
            keys,
            readonly_keys,
        }
    }

    /// Get configuration value.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.keys.get(key)
    }

    /// Set configuration value.  Returns `Err` if the key is read-only.
    pub fn set(&mut self, key: &str, value: String) -> Result<(), String> {
        if self.readonly_keys.contains(key) {
            return Err(format!("Key '{}' is read-only", key));
        }
        self.keys.insert(key.to_string(), value);
        Ok(())
    }

    /// Get all keys.
    pub fn keys(&self) -> &HashMap<String, String> {
        &self.keys
    }

    /// Check if key is read-only.
    pub fn is_readonly(&self, key: &str) -> bool {
        self.readonly_keys.contains(key)
    }

    /// Set configuration value (async-friendly wrapper returning `anyhow::Result`).
    pub fn set_value(&mut self, key: &str, value: String) -> Result<()> {
        self.set(key, value)
            .map_err(|e| ChargePointError::configuration(e).into())
    }
}

impl Default for ConfigurationStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configuration_store_defaults() {
        let store = ConfigurationStore::new();
        assert_eq!(store.get("HeartbeatInterval"), Some(&"86400".to_string()));
        assert!(store.get("NonExistentKey").is_none());
    }

    #[test]
    fn test_configuration_store_set() {
        let mut store = ConfigurationStore::new();
        assert!(store.set("CustomKey", "CustomValue".to_string()).is_ok());
        assert_eq!(store.get("CustomKey"), Some(&"CustomValue".to_string()));
    }

    #[test]
    fn test_configuration_store_readonly() {
        let mut store = ConfigurationStore::new();
        assert!(store.set("NumberOfConnectors", "5".to_string()).is_err());
        assert!(store.is_readonly("NumberOfConnectors"));
        assert!(!store.is_readonly("HeartbeatInterval"));
    }

    #[test]
    fn test_configuration_store_update_existing() {
        let mut store = ConfigurationStore::new();
        assert!(store.set("HeartbeatInterval", "300".to_string()).is_ok());
        assert_eq!(store.get("HeartbeatInterval"), Some(&"300".to_string()));
    }
}
