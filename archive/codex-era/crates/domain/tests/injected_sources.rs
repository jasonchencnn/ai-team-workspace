use teamspace_domain::test_fixtures::{
    DeterministicRandomSource, DeterministicUuidV7Source, FixedClock, FixtureSourceError,
};
use teamspace_domain::{
    Clock, RandomSource, UtcTimestamp, UuidV7Generator, UuidV7GeneratorError, UuidV7Source,
    ValueObjectError,
};

fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse_rfc3339_millis(value).expect("test timestamp must be valid")
}

#[test]
fn injected_clock_and_random_source_generate_repeatable_uuidv7_values() {
    let clock = FixedClock::new(timestamp("2024-09-01T12:34:56.789Z"));
    let mut random = DeterministicRandomSource::new(vec![vec![
        0x0a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc,
    ]]);
    let mut ids = UuidV7Generator::new(&clock, &mut random);

    let id = ids.next_uuid_v7().expect("fixture provides random bytes");

    assert_eq!(id.timestamp_millis(), 1_725_194_096_789);
    assert_eq!(id.to_protocol(), "0191ad94-d095-7abc-9ef0-123456789abc");
}

#[test]
fn fixed_clock_can_be_reset_without_using_system_time() {
    let clock = FixedClock::new(timestamp("2024-01-01T00:00:00.000Z"));

    clock.reset(timestamp("2024-02-29T23:59:59.123Z"));

    assert_eq!(clock.now(), timestamp("2024-02-29T23:59:59.123Z"));
}

#[test]
fn deterministic_random_source_resets_and_fails_stably_when_explicitly_exhausted() {
    let mut random = DeterministicRandomSource::new(vec![vec![1, 2], vec![3, 4]]);
    let mut bytes = [0_u8; 2];

    random.fill_bytes(&mut bytes).expect("first sequence entry");
    assert_eq!(bytes, [1, 2]);
    random.exhaust();
    assert_eq!(
        random.fill_bytes(&mut bytes),
        Err(FixtureSourceError::Exhausted)
    );

    random.reset();
    random
        .fill_bytes(&mut bytes)
        .expect("reset restores first entry");
    assert_eq!(bytes, [1, 2]);
}

#[test]
fn deterministic_id_source_resets_and_fails_stably_when_explicitly_exhausted() {
    let first = teamspace_domain::UuidV7::from_timestamp_and_random(1, [0; 10])
        .expect("fixture ID is valid");
    let second = teamspace_domain::UuidV7::from_timestamp_and_random(2, [1; 10])
        .expect("fixture ID is valid");
    let mut ids = DeterministicUuidV7Source::new(vec![first, second]);

    assert_eq!(ids.next_uuid_v7(), Ok(first));
    ids.exhaust();
    assert_eq!(ids.next_uuid_v7(), Err(FixtureSourceError::Exhausted));

    ids.reset();
    assert_eq!(ids.next_uuid_v7(), Ok(first));
    assert_eq!(ids.next_uuid_v7(), Ok(second));
    assert_eq!(ids.next_uuid_v7(), Err(FixtureSourceError::Exhausted));
}

#[test]
fn uuidv7_generator_reports_injected_source_failures_without_a_system_fallback() {
    let clock = FixedClock::new(timestamp("2024-01-01T00:00:00.000Z"));
    let mut random = DeterministicRandomSource::new(vec![]);
    let mut ids = UuidV7Generator::new(&clock, &mut random);

    assert_eq!(
        ids.next_uuid_v7(),
        Err(UuidV7GeneratorError::Random(FixtureSourceError::Exhausted))
    );

    let pre_epoch_clock = FixedClock::new(timestamp("1969-12-31T23:59:59.999Z"));
    let mut available_random = DeterministicRandomSource::new(vec![vec![0; 10]]);
    let mut pre_epoch_ids = UuidV7Generator::new(&pre_epoch_clock, &mut available_random);

    assert_eq!(
        pre_epoch_ids.next_uuid_v7(),
        Err(UuidV7GeneratorError::Timestamp(
            ValueObjectError::TimestampOutOfRange
        ))
    );
}
