//! Tests the production watchdog recovery policy.
#[path = "../firmware/src/watchdog_recovery.rs"]
mod recovery;

// UICR compatibility is checked before Embassy is allowed to initialize.
#[path = "../firmware/src/startup.rs"]
mod startup;
