#![forbid(unsafe_code)]

//! Frozen domain value objects shared by the control plane.
//!
//! These types deliberately model identity, time, optimistic-concurrency
//! versions, and actor attribution only. They do not grant permissions or
//! encode aggregate state transitions.

use std::fmt;
use std::num::NonZeroU64;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

mod sources;
/// Deterministic sources reserved for tests and local development fixtures.
pub mod test_fixtures;

pub use sources::{Clock, RandomSource, UuidV7Generator, UuidV7GeneratorError, UuidV7Source};

/// A parse or validation error for a domain value object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueObjectError {
    /// The value is not a canonical UUIDv7 protocol string.
    InvalidUuidV7,
    /// The timestamp is outside the supported RFC 3339 four-digit-year range.
    TimestampOutOfRange,
    /// The timestamp is not canonical UTC RFC 3339 with millisecond precision.
    InvalidRfc3339Millis,
    /// A version cannot be zero.
    ZeroVersion,
    /// Incrementing the version would overflow.
    VersionOverflow,
}

impl fmt::Display for ValueObjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidUuidV7 => "expected a canonical UUIDv7",
            Self::TimestampOutOfRange => "timestamp is outside the RFC 3339 range",
            Self::InvalidRfc3339Millis => "expected UTC RFC 3339 with milliseconds",
            Self::ZeroVersion => "version must start at 1",
            Self::VersionOverflow => "version cannot be incremented further",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ValueObjectError {}

/// A UUIDv7 stored as its canonical 16-byte database representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UuidV7([u8; 16]);

impl UuidV7 {
    /// Creates a UUIDv7 from a UTC Unix timestamp in milliseconds and 74 bits
    /// of caller-provided randomness. The leading two bits of `random[1]` are
    /// ignored because UUIDv7 reserves them for the RFC 4122 variant.
    pub fn from_timestamp_and_random(
        timestamp_millis: u64,
        random: [u8; 10],
    ) -> Result<Self, ValueObjectError> {
        if timestamp_millis > 0xFFFF_FFFF_FFFF {
            return Err(ValueObjectError::TimestampOutOfRange);
        }

        let timestamp = timestamp_millis.to_be_bytes();
        let mut bytes = [0_u8; 16];
        bytes[..6].copy_from_slice(&timestamp[2..]);
        bytes[6] = 0x70 | (random[0] & 0x0F);
        bytes[7] = random[1];
        bytes[8] = 0x80 | (random[2] & 0x3F);
        bytes[9..].copy_from_slice(&random[3..]);
        Ok(Self(bytes))
    }

    /// Parses the canonical lower-case, hyphenated UUIDv7 protocol format.
    pub fn parse_protocol(value: &str) -> Result<Self, ValueObjectError> {
        if value.len() != 36 {
            return Err(ValueObjectError::InvalidUuidV7);
        }

        let raw = value.as_bytes();
        if [8, 13, 18, 23].into_iter().any(|index| raw[index] != b'-') {
            return Err(ValueObjectError::InvalidUuidV7);
        }

        let mut bytes = [0_u8; 16];
        let mut byte_index = 0;
        let mut index = 0;
        while index < raw.len() {
            if matches!(index, 8 | 13 | 18 | 23) {
                index += 1;
                continue;
            }
            let high = hex_value(raw[index]).ok_or(ValueObjectError::InvalidUuidV7)?;
            let low = hex_value(raw[index + 1]).ok_or(ValueObjectError::InvalidUuidV7)?;
            bytes[byte_index] = (high << 4) | low;
            byte_index += 1;
            index += 2;
        }

        if bytes[6] >> 4 != 7 || bytes[8] >> 6 != 2 || value != Self(bytes).to_protocol() {
            return Err(ValueObjectError::InvalidUuidV7);
        }
        Ok(Self(bytes))
    }

    /// Restores a UUIDv7 from its 16-byte database representation.
    pub fn from_database_bytes(bytes: [u8; 16]) -> Result<Self, ValueObjectError> {
        if bytes[6] >> 4 != 7 || bytes[8] >> 6 != 2 {
            return Err(ValueObjectError::InvalidUuidV7);
        }
        Ok(Self(bytes))
    }

    /// Returns the exact 16-byte database representation.
    pub const fn database_bytes(self) -> [u8; 16] {
        self.0
    }

    /// Returns the canonical lower-case, hyphenated UUID protocol string.
    pub fn to_protocol(self) -> String {
        let mut output = String::with_capacity(36);
        for (index, byte) in self.0.into_iter().enumerate() {
            if matches!(index, 4 | 6 | 8 | 10) {
                output.push('-');
            }
            output.push(hex_digit(byte >> 4));
            output.push(hex_digit(byte & 0x0F));
        }
        output
    }

    /// Extracts the UUIDv7 UTC Unix timestamp in milliseconds.
    pub fn timestamp_millis(self) -> u64 {
        let mut timestamp = [0_u8; 8];
        timestamp[2..].copy_from_slice(&self.0[..6]);
        u64::from_be_bytes(timestamp)
    }
}

impl fmt::Display for UuidV7 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_protocol())
    }
}

impl FromStr for UuidV7 {
    type Err = ValueObjectError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse_protocol(value)
    }
}

impl Serialize for UuidV7 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_protocol())
    }
}

impl<'de> Deserialize<'de> for UuidV7 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse_protocol(&value).map_err(serde::de::Error::custom)
    }
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

const fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'a' + (value - 10)) as char,
    }
}

macro_rules! uuid_v7_id {
    ($name:ident) => {
        #[doc = concat!("A strongly typed ", stringify!($name), " UUIDv7 identity.")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(UuidV7);

        impl $name {
            /// Creates this typed ID from a validated UUIDv7 value.
            pub const fn from_uuid(uuid: UuidV7) -> Self {
                Self(uuid)
            }

            /// Parses this typed ID from its protocol representation.
            pub fn parse_protocol(value: &str) -> Result<Self, ValueObjectError> {
                UuidV7::parse_protocol(value).map(Self)
            }

            /// Restores this typed ID from its 16-byte database representation.
            pub fn from_database_bytes(bytes: [u8; 16]) -> Result<Self, ValueObjectError> {
                UuidV7::from_database_bytes(bytes).map(Self)
            }

            /// Returns this ID's canonical UUID protocol representation.
            pub fn to_protocol(self) -> String {
                self.0.to_protocol()
            }

            /// Returns this ID's 16-byte database representation.
            pub const fn database_bytes(self) -> [u8; 16] {
                self.0.database_bytes()
            }

            /// Returns the underlying UUIDv7 without converting to another typed ID.
            pub const fn uuid(self) -> UuidV7 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = ValueObjectError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse_protocol(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                self.0.serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                UuidV7::deserialize(deserializer).map(Self)
            }
        }
    };
}

uuid_v7_id!(TeamId);
uuid_v7_id!(HumanId);
uuid_v7_id!(RoleId);
uuid_v7_id!(HarnessInstanceId);
uuid_v7_id!(AdapterId);
uuid_v7_id!(BindingId);
uuid_v7_id!(WorkspaceId);
uuid_v7_id!(TaskId);
uuid_v7_id!(RunId);
uuid_v7_id!(ArtifactId);
uuid_v7_id!(EventId);
uuid_v7_id!(ThreadId);
uuid_v7_id!(MessageId);
uuid_v7_id!(NotificationId);
uuid_v7_id!(ApprovalId);
uuid_v7_id!(RequestId);

/// A UTC timestamp represented as integer Unix milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtcTimestamp(i64);

impl UtcTimestamp {
    /// Creates a timestamp after ensuring it can be rendered as RFC 3339 UTC
    /// with a four-digit year and exactly millisecond precision.
    pub fn from_unix_millis(milliseconds: i64) -> Result<Self, ValueObjectError> {
        if !(MIN_UNIX_MILLIS..=MAX_UNIX_MILLIS).contains(&milliseconds) {
            return Err(ValueObjectError::TimestampOutOfRange);
        }
        Ok(Self(milliseconds))
    }

    /// Parses canonical UTC RFC 3339 (`YYYY-MM-DDTHH:MM:SS.sssZ`).
    pub fn parse_rfc3339_millis(value: &str) -> Result<Self, ValueObjectError> {
        let bytes = value.as_bytes();
        if bytes.len() != 24
            || bytes[4] != b'-'
            || bytes[7] != b'-'
            || bytes[10] != b'T'
            || bytes[13] != b':'
            || bytes[16] != b':'
            || bytes[19] != b'.'
            || bytes[23] != b'Z'
        {
            return Err(ValueObjectError::InvalidRfc3339Millis);
        }
        let year = decimal(&bytes[0..4])? as i32;
        let month = decimal(&bytes[5..7])? as u32;
        let day = decimal(&bytes[8..10])? as u32;
        let hour = decimal(&bytes[11..13])?;
        let minute = decimal(&bytes[14..16])?;
        let second = decimal(&bytes[17..19])?;
        let millisecond = decimal(&bytes[20..23])?;
        if month == 0
            || month > 12
            || day == 0
            || day > days_in_month(year, month)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return Err(ValueObjectError::InvalidRfc3339Millis);
        }
        let days = days_from_civil(year, month, day);
        let milliseconds = days
            .checked_mul(MILLIS_PER_DAY)
            .and_then(|value| {
                value.checked_add(hour * 3_600_000 + minute * 60_000 + second * 1_000 + millisecond)
            })
            .ok_or(ValueObjectError::TimestampOutOfRange)?;
        let timestamp = Self::from_unix_millis(milliseconds)?;
        if timestamp.to_rfc3339_millis() != value {
            return Err(ValueObjectError::InvalidRfc3339Millis);
        }
        Ok(timestamp)
    }

    /// Returns the stored UTC Unix milliseconds for database use.
    pub const fn unix_millis(self) -> i64 {
        self.0
    }

    /// Returns canonical UTC RFC 3339 with exactly three fractional digits.
    pub fn to_rfc3339_millis(self) -> String {
        let days = self.0.div_euclid(MILLIS_PER_DAY);
        let day_millis = self.0.rem_euclid(MILLIS_PER_DAY);
        let (year, month, day) = civil_from_days(days);
        let hour = day_millis / 3_600_000;
        let minute = (day_millis % 3_600_000) / 60_000;
        let second = (day_millis % 60_000) / 1_000;
        let millisecond = day_millis % 1_000;
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millisecond:03}Z")
    }
}

impl fmt::Display for UtcTimestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_rfc3339_millis())
    }
}

impl FromStr for UtcTimestamp {
    type Err = ValueObjectError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse_rfc3339_millis(value)
    }
}

impl Serialize for UtcTimestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_rfc3339_millis())
    }
}

impl<'de> Deserialize<'de> for UtcTimestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse_rfc3339_millis(&value).map_err(serde::de::Error::custom)
    }
}

const MILLIS_PER_DAY: i64 = 86_400_000;
const MIN_UNIX_MILLIS: i64 = -62_167_219_200_000;
const MAX_UNIX_MILLIS: i64 = 253_402_300_799_999;

fn decimal(value: &[u8]) -> Result<i64, ValueObjectError> {
    value.iter().try_fold(0_i64, |number, digit| match digit {
        b'0'..=b'9' => Ok(number * 10 + i64::from(digit - b'0')),
        _ => Err(ValueObjectError::InvalidRfc3339Millis),
    })
}

const fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

const fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

// Howard Hinnant's public-domain civil calendar algorithms, using the Unix epoch.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let adjusted_year = year - if month <= 2 { 1 } else { 0 };
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let month_prime = month as i32 + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day as i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    (era * 146_097 + day_of_era - 719_468) as i64
}

const fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let adjusted_days = days + 719_468;
    let era = if adjusted_days >= 0 {
        adjusted_days
    } else {
        adjusted_days - 146_096
    } / 146_097;
    let day_of_era = adjusted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era as i32 + era as i32 * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    (year, month as u32, day as u32)
}

/// A non-zero, monotonically increasing aggregate version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(NonZeroU64);

impl Version {
    /// The required initial version of a newly created mutable aggregate.
    pub const fn initial() -> Self {
        Self(NonZeroU64::MIN)
    }

    /// Validates a persisted aggregate version.
    pub fn new(value: u64) -> Result<Self, ValueObjectError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(ValueObjectError::ZeroVersion)
    }

    /// Returns the next monotonically increasing version.
    pub fn next(self) -> Result<Self, ValueObjectError> {
        self.0
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(ValueObjectError::VersionOverflow)
    }

    /// Returns the integer representation used by persistence and protocols.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.get())
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// The client-observed version required by a non-create mutation command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExpectedVersion(Version);

impl ExpectedVersion {
    /// Creates an expected-version precondition from a previously read version.
    pub const fn new(version: Version) -> Self {
        Self(version)
    }

    /// Returns whether the current aggregate version satisfies this precondition.
    pub fn matches(self, current: Version) -> bool {
        self.0 == current
    }

    /// Returns the version supplied by the caller.
    pub const fn get(self) -> Version {
        self.0
    }
}

impl Serialize for ExpectedVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.get().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ExpectedVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Version::deserialize(deserializer).map(Self::new)
    }
}

/// The subject that caused an auditable command or event.
///
/// This is attribution only: authorization remains the responsibility of the
/// application layer and is intentionally not encoded here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Actor {
    /// A Human owner or controller.
    Human(HumanId),
    /// A stable Role acting through the control plane.
    Role(RoleId),
    /// The trusted local control plane acting autonomously.
    System,
    /// An external Adapter process reporting through its protocol boundary.
    Adapter(AdapterId),
}
