//! Admin audit log for tracking configuration changes.
//!
//! This module provides audit logging for all admin configuration changes,
//! including endpoint updates, service configuration, and other administrative operations.

use soroban_sdk::{contracttype, Address, Env, String};

/// Represents a single admin configuration change event.
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminConfigChangeEvent {
    /// Unique identifier for this audit entry
    pub entry_id: u64,
    /// Admin address that made the change
    pub admin: Address,
    /// Type of configuration change (e.g., "endpoint_update", "service_config", "rate_limit_update")
    pub change_type: String,
    /// Target of the change (e.g., attestor address, service name)
    pub target: String,
    /// Previous value (empty string if not applicable)
    pub old_value: String,
    /// New value (empty string if not applicable)
    pub new_value: String,
    /// Timestamp of the change
    pub timestamp: u64,
    /// Status of the change ("success" or "failed")
    pub status: String,
    /// Optional error message if status is "failed"
    pub error_message: String,
}

/// Represents the admin audit log configuration
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminAuditLogConfig {
    /// Whether admin audit logging is enabled
    pub enabled: bool,
    /// Maximum number of entries to retain (0 = unlimited)
    pub max_entries: u32,
    /// TTL for audit entries in seconds
    pub ttl_seconds: u64,
}

impl AdminAuditLogConfig {
    /// Create a default admin audit log configuration
    pub fn default() -> Self {
        AdminAuditLogConfig {
            enabled: true,
            max_entries: 10000,
            ttl_seconds: 31_536_000, // 1 year
        }
    }
}

/// Admin audit log manager
pub struct AdminAuditLog;

impl AdminAuditLog {
    /// Log an admin configuration change
    pub fn log_change(
        env: &Env,
        admin: &Address,
        change_type: &str,
        target: &str,
        old_value: &str,
        new_value: &str,
    ) {
        Self::log_change_with_status(
            env,
            admin,
            change_type,
            target,
            old_value,
            new_value,
            "success",
            "",
        );
    }

    /// Log an admin configuration change with status
    pub fn log_change_with_status(
        env: &Env,
        admin: &Address,
        change_type: &str,
        target: &str,
        old_value: &str,
        new_value: &str,
        status: &str,
        error_message: &str,
    ) {
        Self::write_event(
            env,
            admin,
            String::from_str(env, change_type),
            String::from_str(env, target),
            String::from_str(env, old_value),
            String::from_str(env, new_value),
            String::from_str(env, status),
            String::from_str(env, error_message),
        );
    }

    /// Log an admin configuration change where `target` is an on-chain value
    /// already rendered as a Soroban [`String`] (for example `addr.to_string()`).
    ///
    /// This mirrors [`Self::log_change`] but avoids forcing callers inside the
    /// `no_std` contract to materialise a `&str` for an [`Address`] target. It
    /// is the entry point used by `contract.rs` to record admin operations.
    pub fn log_action(
        env: &Env,
        admin: &Address,
        change_type: &str,
        target: String,
        old_value: &str,
        new_value: &str,
    ) {
        Self::write_event(
            env,
            admin,
            String::from_str(env, change_type),
            target,
            String::from_str(env, old_value),
            String::from_str(env, new_value),
            String::from_str(env, "success"),
            String::from_str(env, ""),
        );
    }

    /// Core audit-entry writer shared by the public logging helpers.
    ///
    /// All string fields are already materialised as Soroban [`String`]s so this
    /// function never has to assume the target is a `&str`.
    fn write_event(
        env: &Env,
        admin: &Address,
        change_type: String,
        target: String,
        old_value: String,
        new_value: String,
        status: String,
        error_message: String,
    ) {
        // Check if audit logging is enabled
        let config = Self::get_config(env);

        if !config.enabled {
            return;
        }

        // Get next entry ID
        let counter_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT_CNT");
        let entry_id: u64 = env
            .storage()
            .instance()
            .get(&counter_key)
            .unwrap_or(0u64);

        // Check if we've exceeded max entries
        if config.max_entries > 0 && entry_id >= config.max_entries as u64 {
            // Optionally: delete oldest entry or stop logging
            // For now, we'll continue logging (circular buffer behavior)
        }

        // Create the audit event
        let event = AdminConfigChangeEvent {
            entry_id,
            admin: admin.clone(),
            change_type,
            target,
            old_value,
            new_value,
            timestamp: env.ledger().timestamp(),
            status,
            error_message,
        };

        // Store the event using entry_id as part of the key
        let entry_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT");
        env.storage().instance().set(&(entry_key, entry_id), &event);
        env.storage()
            .instance()
            .extend_ttl(config.ttl_seconds as u32, config.ttl_seconds as u32);

        // Increment counter
        env.storage()
            .instance()
            .set(&counter_key, &(entry_id + 1));
        env.storage()
            .instance()
            .extend_ttl(config.ttl_seconds as u32, config.ttl_seconds as u32);

        // Publish event
        env.events().publish(
            (
                soroban_sdk::symbol_short!("admin"),
                soroban_sdk::symbol_short!("audit"),
                entry_id,
            ),
            event,
        );
    }

    /// Get an admin audit log entry by ID
    pub fn get_entry(env: &Env, entry_id: u64) -> Option<AdminConfigChangeEvent> {
        let entry_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT");
        env.storage().instance().get(&(entry_key, entry_id))
    }

    /// Get the total number of audit entries
    pub fn get_entry_count(env: &Env) -> u64 {
        let counter_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT_CNT");
        env.storage().instance().get(&counter_key).unwrap_or(0u64)
    }

    /// Get the admin audit log configuration
    pub fn get_config(env: &Env) -> AdminAuditLogConfig {
        let config_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT_CFG");
        env.storage()
            .instance()
            .get(&config_key)
            .unwrap_or_else(|| AdminAuditLogConfig::default())
    }

    /// Update the admin audit log configuration
    pub fn set_config(env: &Env, config: &AdminAuditLogConfig) {
        let config_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT_CFG");
        env.storage().instance().set(&config_key, config);
        env.storage()
            .instance()
            .extend_ttl(config.ttl_seconds as u32, config.ttl_seconds as u32);
    }

    /// Clear all audit entries (admin only)
    pub fn clear_entries(env: &Env) {
        // Note: In a real implementation, this would require admin authorization
        // and would iterate through all entries to delete them
        let counter_key = soroban_sdk::Symbol::new(env, "ADMIN_AUDIT_CNT");
        env.storage().instance().set(&counter_key, &0u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_admin_audit_log_config_default() {
        let config = AdminAuditLogConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_entries, 10000);
        assert_eq!(config.ttl_seconds, 31_536_000);
    }
}
