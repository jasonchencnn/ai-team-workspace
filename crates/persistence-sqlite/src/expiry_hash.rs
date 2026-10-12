//! Byte-compatible message-expiry-v1 canonical hash material, shared by its
//! writer and reader. Other historical formats are deliberately not inferred.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use teamspace_domain::{EventId, MessageId, RequestId, TeamId, UtcTimestamp, Version};

pub(crate) const FORMAT: &str = "message-expiry-v1";
pub(crate) struct ExpiryHashFields<'a> {
    pub event_id: EventId,
    pub team_id: TeamId,
    pub message_id: MessageId,
    pub request_id: RequestId,
    pub version: Version,
    pub occurred_at: UtcTimestamp,
    pub previous_hash: Option<&'a str>,
    pub payload: &'a Value,
    pub metadata: &'a Value,
}
pub(crate) fn digest(fields: ExpiryHashFields<'_>) -> String {
    let protected=json!({
        "actor_binding_id":null,"actor_binding_version":null,"actor_harness_instance_id":null,
        "actor_id":null,"actor_kind":null,"actor_role_id":null,"actor_type":"SYSTEM",
        "event_id":fields.event_id.to_protocol(),"event_type":"MESSAGE_EXPIRED",
        "metadata":fields.metadata,"object_version":fields.version.get(),"occurred_at":fields.occurred_at.unix_millis(),
        "payload":fields.payload,"previous_hash":fields.previous_hash,"request_id":fields.request_id.to_protocol(),
        "subject_id":fields.message_id.to_protocol(),"subject_type":"MESSAGE","team_id":fields.team_id.to_protocol()
    }).to_string();
    format!("{:X}", Sha256::digest(protected.as_bytes()))
}
