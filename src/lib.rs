//! Rust System Sentinel: a lightweight Linux system monitor.
//!
//! This crate is the platform-independent model and logic. Operating-system
//! collection is kept out of [`core`]; the binary in `main.rs` wires it up.

pub mod config;
pub mod core;
