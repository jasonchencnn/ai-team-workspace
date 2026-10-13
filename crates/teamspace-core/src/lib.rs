//! Teamspace core: storage and business rules for teams, roles, harnesses and tasks.
#![forbid(unsafe_code)]

pub mod api;
pub mod error;
pub mod model;
pub mod probe;
pub mod role_folder;
mod service;
mod store;
pub mod system_probe;
#[cfg(test)]
mod tests;

pub use api::Teamspace;
pub use error::{Error, ErrorKind, ErrorPayload, Result};
pub use probe::SystemProbe;
