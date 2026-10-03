//! Host-only harness for policy callbacks and initialized diagnostic handoff.
#![allow(dead_code)]
#[path = "../../firmware/src/startup_recovery/gate.rs"]
mod gate;
mod nrf {
    pub(crate) use crate::gate;
}
#[path = "../../firmware/src/startup_recovery/trace.rs"]
mod trace;
