//! Workflow 4.01 keeps driver policy outside the agent. Compile it here as a
//! separate crate so it cannot reach the harness's private flow constructors.

#[path = "../dev_ste_driver/mod.rs"]
mod ste_conversion;
