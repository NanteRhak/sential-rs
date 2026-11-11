//! Rust System Sentinel: a lightweight Linux system monitor.
//!
//! This crate is the platform-independent model and logic. Operating-system
//! collection is kept out of [`core`]; the binary in `main.rs` wires it up.

pub mod action;
pub mod alert;
pub mod cli;
pub mod collect;
pub mod commands;
pub mod config;
pub mod core;
pub mod exits;
pub mod history;
pub mod interrupt;
pub mod oom;
pub mod policy;
pub mod pressure;
pub mod report;
pub mod sample;
pub mod store;
pub mod tui;
pub mod view;
