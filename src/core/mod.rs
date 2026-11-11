//! Platform-independent metric model. Nothing here reads the operating system;
//! collectors in this crate produce these values.

pub mod format;
pub mod metrics;
pub mod process;

/// How loud an alert is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}
