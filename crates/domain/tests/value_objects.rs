use domain::{
    Actor, AdapterId, ExpectedVersion, HumanId, RoleId, RunId, TaskId, UtcTimestamp, UuidV7,
    ValueObjectError, Version,
};
use serde_json::{from_str, to_string};

fn uuid(timestamp: u64, tail: u8) -> UuidV7 {
    UuidV7::from_timestamp_and_random(
        timestamp,
        [0, 0, tail, tail, tail, tail, tail, tail, tail, tail],
    )
    .expect("test timestamp is valid")
}

#[test]
fn typed_ids_round_trip_between_protocol_and_database() {
    let value = uuid(1_726_300_123_456, 0xA5);
    let role = RoleId::from_uuid(value);
    let protocol = "0191ef81-7140-7000-a5a5-a5a5a5a5a5a5";

    assert_eq!(role.to_protocol(), protocol);
    assert_eq!(RoleId::parse_protocol(protocol), Ok(role));
    assert_eq!(RoleId::from_database_bytes(role.database_bytes()), Ok(role));
    assert_eq!(role.uuid().timestamp_millis(), 1_726_300_123_456);
}

#[test]
fn ids_sort_by_uuidv7_timestamp_then_randomness() {
    let earlier = TaskId::from_uuid(uuid(100, 0xFF));
    let later = TaskId::from_uuid(uuid(101, 0));
    let same_millisecond_lower_random = TaskId::from_uuid(uuid(101, 0));
    let same_millisecond_higher_random = TaskId::from_uuid(uuid(101, 1));

    assert!(earlier < later);
    assert!(same_millisecond_lower_random < same_millisecond_higher_random);
}

#[test]
fn id_types_cannot_be_substituted_without_an_explicit_domain_decision() {
    let id = uuid(100, 1);
    let role = RoleId::from_uuid(id);
    let task = TaskId::from_uuid(id);
    let run = RunId::from_uuid(id);

    assert_eq!(role.to_protocol(), task.to_protocol());
    assert_eq!(task.to_protocol(), run.to_protocol());
    // RoleId, TaskId, and RunId intentionally have no cross-type `From` impl.
    assert_eq!(std::mem::size_of_val(&role), std::mem::size_of_val(&task));
}

#[test]
fn uuidv7_rejects_noncanonical_or_wrong_version_and_variant_values() {
    assert_eq!(
        UuidV7::parse_protocol("0191B7DA-4BC0-7000-A5A5-A5A5A5A5A5A5"),
        Err(ValueObjectError::InvalidUuidV7)
    );
    assert_eq!(
        UuidV7::parse_protocol("0191b7da-4bc0-6000-a5a5-a5a5a5a5a5a5"),
        Err(ValueObjectError::InvalidUuidV7)
    );
    assert_eq!(
        UuidV7::parse_protocol("0191b7da-4bc0-7000-65a5-a5a5a5a5a5a5"),
        Err(ValueObjectError::InvalidUuidV7)
    );
    assert_eq!(
        UuidV7::from_database_bytes([0; 16]),
        Err(ValueObjectError::InvalidUuidV7)
    );
}

#[test]
fn utc_rfc3339_milliseconds_round_trip_and_sort() {
    let epoch =
        UtcTimestamp::parse_rfc3339_millis("1970-01-01T00:00:00.000Z").expect("valid epoch");
    let before = UtcTimestamp::parse_rfc3339_millis("1969-12-31T23:59:59.999Z")
        .expect("valid pre-epoch time");
    let leap = UtcTimestamp::parse_rfc3339_millis("2024-02-29T23:59:59.123Z")
        .expect("valid leap-day time");

    assert_eq!(epoch.unix_millis(), 0);
    assert_eq!(before.unix_millis(), -1);
    assert_eq!(leap.to_rfc3339_millis(), "2024-02-29T23:59:59.123Z");
    assert!(before < epoch && epoch < leap);
}

#[test]
fn utc_timestamp_rejects_non_utc_non_millisecond_and_invalid_calendar_values() {
    for invalid in [
        "2024-02-29T23:59:59Z",
        "2024-02-29T23:59:59.123+00:00",
        "2023-02-29T00:00:00.000Z",
        "2024-13-01T00:00:00.000Z",
        "2024-01-01T24:00:00.000Z",
    ] {
        assert_eq!(
            UtcTimestamp::parse_rfc3339_millis(invalid),
            Err(ValueObjectError::InvalidRfc3339Millis)
        );
    }
    assert_eq!(
        UtcTimestamp::from_unix_millis(i64::MAX),
        Err(ValueObjectError::TimestampOutOfRange)
    );
}

#[test]
fn utc_timestamp_accepts_the_rfc3339_supported_range_boundaries() {
    let earliest =
        UtcTimestamp::from_unix_millis(-62_167_219_200_000).expect("year zero starts in range");
    let latest =
        UtcTimestamp::from_unix_millis(253_402_300_799_999).expect("year 9999 ends in range");

    assert_eq!(earliest.to_rfc3339_millis(), "0000-01-01T00:00:00.000Z");
    assert_eq!(latest.to_rfc3339_millis(), "9999-12-31T23:59:59.999Z");
    assert_eq!(
        UtcTimestamp::parse_rfc3339_millis(&earliest.to_rfc3339_millis()),
        Ok(earliest)
    );
    assert_eq!(
        UtcTimestamp::parse_rfc3339_millis(&latest.to_rfc3339_millis()),
        Ok(latest)
    );
}

#[test]
fn versions_start_at_one_increment_monotonically_and_match_explicit_preconditions() {
    let initial = Version::initial();
    let next = initial.next().expect("initial version increments");
    let expected = ExpectedVersion::new(next);

    assert_eq!(initial.get(), 1);
    assert_eq!(next.get(), 2);
    assert!(expected.matches(next));
    assert!(!expected.matches(initial));
    assert_eq!(Version::new(0), Err(ValueObjectError::ZeroVersion));
    assert_eq!(
        Version::new(u64::MAX).and_then(Version::next),
        Err(ValueObjectError::VersionOverflow)
    );
}

#[test]
fn actor_preserves_human_role_system_and_adapter_distinctions() {
    let id = uuid(100, 1);
    let human = Actor::Human(HumanId::from_uuid(id));
    let role = Actor::Role(RoleId::from_uuid(id));
    let adapter = Actor::Adapter(AdapterId::from_uuid(id));

    assert_ne!(human, role);
    assert_ne!(role, Actor::System);
    assert_ne!(Actor::System, adapter);
}

#[test]
fn serde_round_trips_typed_ids_as_canonical_uuidv7_protocol_strings() {
    let role = RoleId::from_uuid(uuid(1_726_300_123_456, 0xA5));
    let encoded = to_string(&role).expect("ID serializes");

    assert_eq!(encoded, "\"0191ef81-7140-7000-a5a5-a5a5a5a5a5a5\"");
    assert_eq!(from_str::<RoleId>(&encoded).expect("ID deserializes"), role);
    assert!(from_str::<RoleId>("\"0191EF81-7140-7000-A5A5-A5A5A5A5A5A5\"").is_err());
    assert!(from_str::<RoleId>("\"0191ef81-7140-6000-a5a5-a5a5a5a5a5a5\"").is_err());
}

#[test]
fn serde_round_trips_utc_timestamps_as_canonical_millisecond_rfc3339_strings() {
    let timestamp =
        UtcTimestamp::parse_rfc3339_millis("2024-02-29T23:59:59.123Z").expect("valid timestamp");
    let encoded = to_string(&timestamp).expect("timestamp serializes");

    assert_eq!(encoded, "\"2024-02-29T23:59:59.123Z\"");
    assert_eq!(
        from_str::<UtcTimestamp>(&encoded).expect("timestamp deserializes"),
        timestamp
    );
    assert!(from_str::<UtcTimestamp>("\"2024-02-29T23:59:59.123+00:00\"").is_err());
    assert!(from_str::<UtcTimestamp>("\"2024-02-29T23:59:59Z\"").is_err());
}

#[test]
fn serde_round_trips_versions_as_positive_integers() {
    let version = Version::new(42).expect("positive version");
    let expected = ExpectedVersion::new(version);

    assert_eq!(to_string(&version).expect("version serializes"), "42");
    assert_eq!(
        from_str::<Version>("42").expect("version deserializes"),
        version
    );
    assert_eq!(
        to_string(&expected).expect("expected version serializes"),
        "42"
    );
    assert_eq!(
        from_str::<ExpectedVersion>("42").expect("expected version deserializes"),
        expected
    );
    assert!(from_str::<Version>("0").is_err());
    assert!(from_str::<ExpectedVersion>("-1").is_err());
    assert!(from_str::<Version>("\"42\"").is_err());
}
