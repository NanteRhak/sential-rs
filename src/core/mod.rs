//! Platform-independent metric model. Nothing here reads the operating system;
//! collectors in this crate produce these values.

use serde::{Deserialize, Serialize};

pub mod format;
pub mod metrics;
pub mod process;

/// How loud an alert is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}
