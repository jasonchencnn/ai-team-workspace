//! Deterministic sources for tests and local development fixtures.
//!
//! These types are intentionally isolated from domain business concepts. They
//! provide repeatable source values only, and never read the system clock,
//! user directories, or the network.

use std::cell::Cell;
use std::collections::VecDeque;

use crate::{Clock, RandomSource, UtcTimestamp, UuidV7, UuidV7Source};

/// A stable failure returned when a deterministic fixture has no next value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureSourceError {
    /// The sequence was explicitly exhausted or all its values were consumed.
    Exhausted,
    /// The next configured random value has a different length than requested.
    ByteCountMismatch {
        /// Number of bytes requested by the consumer.
        expected: usize,
        /// Number of bytes supplied by the fixture sequence.
        actual: usize,
    },
}

/// A clock that returns a fixed timestamp until it is reset by a test.
pub struct FixedClock {
    current: Cell<UtcTimestamp>,
}

impl FixedClock {
    /// Creates a clock fixed at `timestamp`.
    pub const fn new(timestamp: UtcTimestamp) -> Self {
        Self {
            current: Cell::new(timestamp),
        }
    }

    /// Replaces the fixed timestamp without consulting system time.
    pub fn reset(&self, timestamp: UtcTimestamp) {
        self.current.set(timestamp);
    }
}

impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        self.current.get()
    }
}

/// A finite sequence of deterministic random byte values.
pub struct DeterministicRandomSource {
    initial: Vec<Vec<u8>>,
    remaining: VecDeque<Vec<u8>>,
}

impl DeterministicRandomSource {
    /// Creates a source that yields each configured byte value exactly once.
    pub fn new(sequence: Vec<Vec<u8>>) -> Self {
        Self {
            remaining: VecDeque::from(sequence.clone()),
            initial: sequence,
        }
    }

    /// Restores the sequence to its initial values.
    pub fn reset(&mut self) {
        self.remaining = VecDeque::from(self.initial.clone());
    }

    /// Clears all remaining values so the next request fails predictably.
    pub fn exhaust(&mut self) {
        self.remaining.clear();
    }
}

impl RandomSource for DeterministicRandomSource {
    type Error = FixtureSourceError;

    fn fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), Self::Error> {
        let next = self
            .remaining
            .pop_front()
            .ok_or(FixtureSourceError::Exhausted)?;
        if next.len() != destination.len() {
            return Err(FixtureSourceError::ByteCountMismatch {
                expected: destination.len(),
                actual: next.len(),
            });
        }
        destination.copy_from_slice(&next);
        Ok(())
    }
}

/// A finite sequence of already-validated UUIDv7 values for test setup.
pub struct DeterministicUuidV7Source {
    initial: Vec<UuidV7>,
    remaining: VecDeque<UuidV7>,
}

impl DeterministicUuidV7Source {
    /// Creates a source that yields each configured UUIDv7 value exactly once.
    pub fn new(sequence: Vec<UuidV7>) -> Self {
        Self {
            remaining: VecDeque::from(sequence.clone()),
            initial: sequence,
        }
    }

    /// Restores the UUIDv7 sequence to its initial values.
    pub fn reset(&mut self) {
        self.remaining = VecDeque::from(self.initial.clone());
    }

    /// Clears all remaining values so the next ID request fails predictably.
    pub fn exhaust(&mut self) {
        self.remaining.clear();
    }
}

impl UuidV7Source for DeterministicUuidV7Source {
    type Error = FixtureSourceError;

    fn next_uuid_v7(&mut self) -> Result<UuidV7, Self::Error> {
        self.remaining
            .pop_front()
            .ok_or(FixtureSourceError::Exhausted)
    }
}
