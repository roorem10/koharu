//! Koharu's process entrypoint and diagnostics.

mod booharu; // Booharu: its glue, which keeps Sentry off without a DSN compiled in.
pub mod panic;
pub mod sentry;
