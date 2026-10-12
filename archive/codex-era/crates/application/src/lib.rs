#![forbid(unsafe_code)]

//! Application transaction orchestration. Infrastructure implements the ports.
/// Fixed internal audit projection and at-least-once consumption.
pub mod audit_consumer;
pub mod message_expiry;
/// Trusted installation Owner identity and shared recipient selection.
pub mod owner_identity;
