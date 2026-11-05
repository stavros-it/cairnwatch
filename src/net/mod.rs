//! Network clients: GitHub, GitLab, VirusTotal, download engine, bus.
//! All async functions here are consumed by the Task 5 bus and Task 6 UI;
//! dead-code allow is crate-scoped to this module only.
#![allow(dead_code)]

pub mod bus;
pub mod download;
pub mod gh;
pub mod gl;
pub mod ratelimits;
pub mod vt;
