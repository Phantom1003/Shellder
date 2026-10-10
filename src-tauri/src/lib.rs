//! Shellder: the daemon that keeps ssh masters alive, the app around it and
//! the CLI, as a library so the integration tests can drive the pieces.
pub mod app;
pub mod cli;
pub mod core;
