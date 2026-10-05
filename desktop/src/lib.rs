//! Companion's shared recovery, backup and guarded application-update core.
pub mod completion_gate;
pub mod diagnostics;
pub mod firmware_journey;
pub mod release;
pub mod runtime_recovery;
pub mod update;
pub mod update_image;

mod backup_flow;
pub mod device;
pub mod home;
pub mod journey;
pub mod session;

pub mod recovery_journey;

mod battery;
pub mod flow_presentation;
mod recovery;
pub mod ui;

mod status_strip;

pub mod factory_version;
pub mod firmware_version;

mod device_status;

mod status_cache;

pub mod dongle_pairing;

mod navigation;
mod operation;
mod return_flow;

mod install_journey;

pub mod factory_release;

mod firmware_preflight;

mod factory_source;

mod peripheral_journey;
pub mod scope;

mod scope_presence;
