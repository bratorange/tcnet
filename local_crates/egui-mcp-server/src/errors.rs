//! Error types for the MCP server
//!
//! This module provides structured error types for better error handling
//! and more informative error messages.

use atspi::zbus;
use thiserror::Error;

/// Errors that can occur during AT-SPI operations
#[derive(Debug, Error)]
#[allow(dead_code)] // Some variants reserved for future use
pub enum AtspiError {
    /// Failed to connect to AT-SPI
    #[error("Failed to connect to AT-SPI: {0}")]
    Connection(#[source] atspi::AtspiError),

    /// Application not found
    #[error("Application '{app_name}' not found via AT-SPI")]
    AppNotFound { app_name: String },

    /// Element not found
    #[error("Element with id {id} not found in application '{app_name}'")]
    ElementNotFound { id: u64, app_name: String },

    /// AT-SPI interface not available on the element
    #[error("AT-SPI {interface} interface not available on element {id}")]
    InterfaceNotAvailable { interface: &'static str, id: u64 },

    /// D-Bus communication error
    #[error("D-Bus error: {0}")]
    DBus(#[from] zbus::Error),

    /// AT-SPI library error
    #[error("AT-SPI error: {0}")]
    Atspi(#[from] atspi::AtspiError),
}

impl AtspiError {
    /// Create an ElementNotFound error
    pub fn element_not_found(id: u64, app_name: impl Into<String>) -> Self {
        Self::ElementNotFound {
            id,
            app_name: app_name.into(),
        }
    }

    /// Create an InterfaceNotAvailable error (for future use)
    #[allow(dead_code)]
    pub fn interface_not_available(interface: &'static str, id: u64) -> Self {
        Self::InterfaceNotAvailable { interface, id }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_not_found_error_message() {
        let err = AtspiError::element_not_found(42, "my-app");
        assert_eq!(
            err.to_string(),
            "Element with id 42 not found in application 'my-app'"
        );
    }

    #[test]
    fn test_element_not_found_with_string() {
        let app_name = String::from("test-app");
        let err = AtspiError::element_not_found(123, app_name);
        assert!(err.to_string().contains("123"));
        assert!(err.to_string().contains("test-app"));
    }

    #[test]
    fn test_interface_not_available_error_message() {
        let err = AtspiError::interface_not_available("Text", 99);
        assert_eq!(
            err.to_string(),
            "AT-SPI Text interface not available on element 99"
        );
    }

    #[test]
    fn test_app_not_found_error_message() {
        let err = AtspiError::AppNotFound {
            app_name: "missing-app".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Application 'missing-app' not found via AT-SPI"
        );
    }

    #[test]
    fn test_error_postconditions() {
        // 事後条件: ElementNotFound のメッセージには id と app_name が含まれる
        let err = AtspiError::element_not_found(999, "test-app");
        let msg = err.to_string();
        assert!(msg.contains("999"), "Error message should contain id");
        assert!(
            msg.contains("test-app"),
            "Error message should contain app_name"
        );

        // 事後条件: InterfaceNotAvailable のメッセージには interface と id が含まれる
        let err = AtspiError::interface_not_available("Selection", 42);
        let msg = err.to_string();
        assert!(
            msg.contains("Selection"),
            "Error message should contain interface"
        );
        assert!(msg.contains("42"), "Error message should contain id");

        // 事後条件: AppNotFound のメッセージには app_name が含まれる
        let err = AtspiError::AppNotFound {
            app_name: "my-app".to_string(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("my-app"),
            "Error message should contain app_name"
        );
    }
}
