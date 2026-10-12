#![forbid(unsafe_code)]

use std::fmt;

/// A validated SemVer 2.0.0 version string.
///
/// Under Adapter V0 and ADR-0002, leading zeros in core numbers (major, minor, patch)
/// and numeric pre-release identifiers are strictly prohibited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre_release: Option<String>,
    pub build_metadata: Option<String>,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemVerError {
    Empty,
    TooLong,
    InvalidFormat,
    LeadingZero,
    EmptyIdentifier,
    InvalidCharacter(char),
    Overflow,
}

impl fmt::Display for SemVerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "version string is empty"),
            Self::TooLong => write!(f, "version string exceeds maximum length of 128 characters"),
            Self::InvalidFormat => write!(f, "invalid SemVer 2.0.0 format"),
            Self::LeadingZero => write!(f, "leading zero in numerical identifier is forbidden"),
            Self::EmptyIdentifier => write!(f, "empty identifier in pre-release or build metadata"),
            Self::InvalidCharacter(c) => write!(f, "invalid character '{c}' in version string"),
            Self::Overflow => write!(f, "version number component overflows u64"),
        }
    }
}

impl std::error::Error for SemVerError {}

impl SemVer {
    /// Parses and strictly validates a SemVer 2.0.0 string.
    pub fn parse(s: &str) -> Result<Self, SemVerError> {
        if s.is_empty() {
            return Err(SemVerError::Empty);
        }
        if s.len() > 128 {
            return Err(SemVerError::TooLong);
        }

        // Split build metadata
        let (without_build, build_metadata) = match s.split_once('+') {
            Some((left, build)) => {
                validate_build_metadata(build)?;
                (left, Some(build.to_string()))
            }
            None => (s, None),
        };

        // Split pre-release
        let (core, pre_release) = match without_build.split_once('-') {
            Some((core_str, pre)) => {
                validate_pre_release(pre)?;
                (core_str, Some(pre.to_string()))
            }
            None => (without_build, None),
        };

        // Core must have major.minor.patch
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() != 3 {
            return Err(SemVerError::InvalidFormat);
        }

        let major = parse_numeric_component(parts[0])?;
        let minor = parse_numeric_component(parts[1])?;
        let patch = parse_numeric_component(parts[2])?;

        Ok(Self {
            major,
            minor,
            patch,
            pre_release,
            build_metadata,
            raw: s.to_string(),
        })
    }

    /// Checks if this protocol version is compatible with the given target protocol version.
    ///
    /// For Adapter V0 (current baseline: 0.1.0):
    /// Compatible within 0.1.x series (major == 0 && minor == 1).
    #[must_use]
    pub fn is_compatible_with(&self, target: &SemVer) -> bool {
        if target.major == 0 {
            // For major 0, minor increments indicate breaking changes
            self.major == 0 && self.minor == target.minor
        } else {
            self.major == target.major && self.minor >= target.minor
        }
    }

    /// Returns the raw version string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

fn parse_numeric_component(s: &str) -> Result<u64, SemVerError> {
    if s.is_empty() {
        return Err(SemVerError::InvalidFormat);
    }
    // Check leading zeros: "0" is allowed, but "00", "01", etc. are forbidden
    if s.len() > 1 && s.starts_with('0') {
        return Err(SemVerError::LeadingZero);
    }
    for c in s.chars() {
        if !c.is_ascii_digit() {
            return Err(SemVerError::InvalidCharacter(c));
        }
    }
    s.parse::<u64>().map_err(|_| SemVerError::Overflow)
}

fn validate_pre_release(s: &str) -> Result<(), SemVerError> {
    if s.is_empty() {
        return Err(SemVerError::EmptyIdentifier);
    }
    for ident in s.split('.') {
        if ident.is_empty() {
            return Err(SemVerError::EmptyIdentifier);
        }
        let mut is_numeric = true;
        for c in ident.chars() {
            if c.is_ascii_digit() {
                // digit
            } else if c.is_ascii_alphabetic() || c == '-' {
                is_numeric = false;
            } else {
                return Err(SemVerError::InvalidCharacter(c));
            }
        }
        if is_numeric && ident.len() > 1 && ident.starts_with('0') {
            return Err(SemVerError::LeadingZero);
        }
    }
    Ok(())
}

fn validate_build_metadata(s: &str) -> Result<(), SemVerError> {
    if s.is_empty() {
        return Err(SemVerError::EmptyIdentifier);
    }
    for ident in s.split('.') {
        if ident.is_empty() {
            return Err(SemVerError::EmptyIdentifier);
        }
        for c in ident.chars() {
            if !c.is_ascii_alphanumeric() && c != '-' {
                return Err(SemVerError::InvalidCharacter(c));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_semver_versions() {
        let v1 = SemVer::parse("0.1.0").expect("valid 0.1.0");
        assert_eq!(v1.major, 0);
        assert_eq!(v1.minor, 1);
        assert_eq!(v1.patch, 0);
        assert_eq!(v1.pre_release, None);
        assert_eq!(v1.build_metadata, None);

        let v2 = SemVer::parse("1.2.3-alpha.1+build.5").expect("valid prerelease and build");
        assert_eq!(v2.major, 1);
        assert_eq!(v2.minor, 2);
        assert_eq!(v2.patch, 3);
        assert_eq!(v2.pre_release.as_deref(), Some("alpha.1"));
        assert_eq!(v2.build_metadata.as_deref(), Some("build.5"));

        let v3 = SemVer::parse("0.1.0-rc.1+contract.7").expect("valid contract version");
        assert_eq!(v3.pre_release.as_deref(), Some("rc.1"));

        // Alphanumeric identifier with leading zero is allowed
        let v4 = SemVer::parse("1.2.3-01a").expect("valid alphanumeric pre-release");
        assert_eq!(v4.pre_release.as_deref(), Some("01a"));
    }

    #[test]
    fn rejects_invalid_semver_leading_zeros() {
        // Leading zeros in major, minor, patch
        assert_eq!(SemVer::parse("00.1.0"), Err(SemVerError::LeadingZero));
        assert_eq!(SemVer::parse("01.1.0"), Err(SemVerError::LeadingZero));
        assert_eq!(SemVer::parse("0.01.0"), Err(SemVerError::LeadingZero));
        assert_eq!(SemVer::parse("0.1.00"), Err(SemVerError::LeadingZero));
        assert_eq!(SemVer::parse("0.1.01"), Err(SemVerError::LeadingZero));

        // Leading zeros in numeric pre-release identifier
        assert_eq!(SemVer::parse("1.2.3-01"), Err(SemVerError::LeadingZero));
        assert_eq!(
            SemVer::parse("1.2.3-alpha.01"),
            Err(SemVerError::LeadingZero)
        );
    }

    #[test]
    fn rejects_malformed_semver() {
        assert_eq!(SemVer::parse(""), Err(SemVerError::Empty));
        assert_eq!(SemVer::parse("0.1"), Err(SemVerError::InvalidFormat));
        assert_eq!(SemVer::parse("0.1.0.0"), Err(SemVerError::InvalidFormat));
        assert_eq!(SemVer::parse("0.1.0-"), Err(SemVerError::EmptyIdentifier));
        assert_eq!(SemVer::parse("0.1.0+"), Err(SemVerError::EmptyIdentifier));
        assert_eq!(
            SemVer::parse("0.1.0-alpha..1"),
            Err(SemVerError::EmptyIdentifier)
        );
        assert_eq!(
            SemVer::parse("0.1.0+build..1"),
            Err(SemVerError::EmptyIdentifier)
        );
        assert!(matches!(
            SemVer::parse("0.1.0-alpha@1"),
            Err(SemVerError::InvalidCharacter('@'))
        ));
    }

    #[test]
    fn compatibility_checking() {
        let v010 = SemVer::parse("0.1.0").expect("valid 0.1.0");
        let v015 = SemVer::parse("0.1.5").expect("valid 0.1.5");
        let v020 = SemVer::parse("0.2.0").expect("valid 0.2.0");
        let v100 = SemVer::parse("1.0.0").expect("valid 1.0.0");

        // 0.1.x is compatible with 0.1.0
        assert!(v010.is_compatible_with(&v010));
        assert!(v015.is_compatible_with(&v010));

        // 0.2.0 is NOT compatible with 0.1.0
        assert!(!v020.is_compatible_with(&v010));
        // 1.0.0 is NOT compatible with 0.1.0
        assert!(!v100.is_compatible_with(&v010));
    }
}
