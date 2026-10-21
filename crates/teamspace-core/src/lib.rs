//! Teamspace core: storage and business rules for teams, roles, harnesses and tasks.
#![forbid(unsafe_code)]

pub mod acp;
pub mod api;
pub mod control;
pub mod dispatch;
pub mod error;
pub mod leader;
pub mod mock_agent;
pub mod model;
pub mod probe;
pub mod redact;
mod review_api;
pub mod role_folder;
pub mod run;
mod service;
mod store;
pub mod system_probe;
#[cfg(test)]
mod tests;
pub mod workspace_file;

pub use api::{AgentCommand, HostConfig, RUN_EVENT, Teamspace};
pub use error::{Error, ErrorKind, ErrorPayload, Result};
pub use probe::SystemProbe;
pub use run::RunSettings;
