#![forbid(unsafe_code)]

use std::fmt;

/// A lightweight, self-contained JSON Value.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    Integer(i64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonParseError {
    UnexpectedEnd,
    UnexpectedChar(char, usize),
    InvalidEscape(usize),
    InvalidNumber(usize),
    InvalidControlCharacter(usize),
    DuplicateKey,
}

impl fmt::Display for JsonParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd => write!(f, "unexpected end of JSON input"),
            Self::UnexpectedChar(c, pos) => write!(f, "unexpected character '{c}' at byte {pos}"),
            Self::InvalidEscape(pos) => write!(f, "invalid escape sequence at byte {pos}"),
            Self::InvalidNumber(pos) => write!(f, "invalid number format at byte {pos}"),
            Self::InvalidControlCharacter(pos) => {
                write!(f, "unescaped control character at byte {pos}")
            }
            Self::DuplicateKey => f.write_str("duplicate object key"),
        }
    }
}

impl std::error::Error for JsonParseError {}

impl JsonValue {
    pub fn parse(input: &str) -> Result<Self, JsonParseError> {
        let chars: Vec<char> = input.chars().collect();
        let mut index = 0;
        skip_whitespace(&chars, &mut index);
        let val = parse_value(&chars, &mut index)?;
        skip_whitespace(&chars, &mut index);
        if index < chars.len() {
            return Err(JsonParseError::UnexpectedChar(chars[index], index));
        }
        Ok(val)
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(n) => Some(*n),
            #[allow(clippy::cast_possible_truncation)]
            Self::Number(n) if n.fract() == 0.0 => Some(*n as i64),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            Self::Array(arr) => Some(arr),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_object(&self) -> Option<&[(String, JsonValue)]> {
        match self {
            Self::Object(entries) => Some(entries),
            _ => None,
        }
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            Self::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    #[must_use]
    pub fn to_json_string(&self) -> String {
        let mut buf = String::new();
        self.serialize_into(&mut buf);
        buf
    }

    fn serialize_into(&self, buf: &mut String) {
        match self {
            Self::Null => buf.push_str("null"),
            Self::Bool(true) => buf.push_str("true"),
            Self::Bool(false) => buf.push_str("false"),
            Self::Integer(i) => buf.push_str(&i.to_string()),
            Self::Number(n) => {
                if n.fract() == 0.0 {
                    buf.push_str(&format!("{n:.1}"));
                } else {
                    buf.push_str(&n.to_string());
                }
            }
            Self::String(s) => {
                buf.push('"');
                for c in s.chars() {
                    match c {
                        '"' => buf.push_str("\\\""),
                        '\\' => buf.push_str("\\\\"),
                        '\n' => buf.push_str("\\n"),
                        '\r' => buf.push_str("\\r"),
                        '\t' => buf.push_str("\\t"),
                        '\u{08}' => buf.push_str("\\b"),
                        '\u{0C}' => buf.push_str("\\f"),
                        c if c < ' ' => buf.push_str(&format!("\\u{:04x}", c as u32)),
                        c => buf.push(c),
                    }
                }
                buf.push('"');
            }
            Self::Array(items) => {
                buf.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        buf.push(',');
                    }
                    item.serialize_into(buf);
                }
                buf.push(']');
            }
            Self::Object(fields) => {
                buf.push('{');
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        buf.push(',');
                    }
                    JsonValue::String(k.clone()).serialize_into(buf);
                    buf.push(':');
                    v.serialize_into(buf);
                }
                buf.push('}');
            }
        }
    }
}

fn skip_whitespace(chars: &[char], index: &mut usize) {
    while *index < chars.len() && matches!(chars[*index], ' ' | '\t' | '\r' | '\n') {
        *index += 1;
    }
}

fn parse_value(chars: &[char], index: &mut usize) -> Result<JsonValue, JsonParseError> {
    skip_whitespace(chars, index);
    if *index >= chars.len() {
        return Err(JsonParseError::UnexpectedEnd);
    }
    match chars[*index] {
        '{' => parse_object(chars, index),
        '[' => parse_array(chars, index),
        '"' => parse_string(chars, index).map(JsonValue::String),
        't' | 'f' => parse_bool(chars, index),
        'n' => parse_null(chars, index),
        '-' | '0'..='9' => parse_number(chars, index),
        c => Err(JsonParseError::UnexpectedChar(c, *index)),
    }
}

fn parse_object(chars: &[char], index: &mut usize) -> Result<JsonValue, JsonParseError> {
    *index += 1; // skip '{'
    skip_whitespace(chars, index);
    let mut entries = Vec::new();
    if *index < chars.len() && chars[*index] == '}' {
        *index += 1;
        return Ok(JsonValue::Object(entries));
    }

    loop {
        skip_whitespace(chars, index);
        if *index >= chars.len() {
            return Err(JsonParseError::UnexpectedEnd);
        }
        if chars[*index] != '"' {
            return Err(JsonParseError::UnexpectedChar(chars[*index], *index));
        }
        let key = parse_string(chars, index)?;
        skip_whitespace(chars, index);
        if *index >= chars.len() || chars[*index] != ':' {
            return Err(JsonParseError::UnexpectedChar(
                chars.get(*index).copied().unwrap_or('\0'),
                *index,
            ));
        }
        *index += 1; // skip ':'
        let val = parse_value(chars, index)?;
        if entries.iter().any(|(existing_key, _)| existing_key == &key) {
            return Err(JsonParseError::DuplicateKey);
        }
        entries.push((key, val));

        skip_whitespace(chars, index);
        if *index >= chars.len() {
            return Err(JsonParseError::UnexpectedEnd);
        }
        if chars[*index] == '}' {
            *index += 1;
            break;
        }
        if chars[*index] == ',' {
            *index += 1;
            continue;
        }
        return Err(JsonParseError::UnexpectedChar(chars[*index], *index));
    }
    Ok(JsonValue::Object(entries))
}

fn parse_array(chars: &[char], index: &mut usize) -> Result<JsonValue, JsonParseError> {
    *index += 1; // skip '['
    skip_whitespace(chars, index);
    let mut items = Vec::new();
    if *index < chars.len() && chars[*index] == ']' {
        *index += 1;
        return Ok(JsonValue::Array(items));
    }

    loop {
        let val = parse_value(chars, index)?;
        items.push(val);
        skip_whitespace(chars, index);
        if *index >= chars.len() {
            return Err(JsonParseError::UnexpectedEnd);
        }
        if chars[*index] == ']' {
            *index += 1;
            break;
        }
        if chars[*index] == ',' {
            *index += 1;
            continue;
        }
        return Err(JsonParseError::UnexpectedChar(chars[*index], *index));
    }
    Ok(JsonValue::Array(items))
}

fn parse_string(chars: &[char], index: &mut usize) -> Result<String, JsonParseError> {
    *index += 1; // skip opening quote
    let mut s = String::new();
    while *index < chars.len() {
        let c = chars[*index];
        *index += 1;
        match c {
            '"' => return Ok(s),
            '\\' => {
                if *index >= chars.len() {
                    return Err(JsonParseError::UnexpectedEnd);
                }
                let esc = chars[*index];
                *index += 1;
                match esc {
                    '"' => s.push('"'),
                    '\\' => s.push('\\'),
                    '/' => s.push('/'),
                    'b' => s.push('\u{08}'),
                    'f' => s.push('\u{0C}'),
                    'n' => s.push('\n'),
                    'r' => s.push('\r'),
                    't' => s.push('\t'),
                    'u' => {
                        if *index + 4 > chars.len() {
                            return Err(JsonParseError::UnexpectedEnd);
                        }
                        let hex_str: String = chars[*index..*index + 4].iter().collect();
                        *index += 4;
                        let code = u32::from_str_radix(&hex_str, 16)
                            .map_err(|_| JsonParseError::InvalidEscape(*index - 4))?;
                        match code {
                            0xD800..=0xDBFF => {
                                if *index + 6 > chars.len() {
                                    return Err(JsonParseError::UnexpectedEnd);
                                }
                                if chars[*index] != '\\' || chars[*index + 1] != 'u' {
                                    return Err(JsonParseError::InvalidEscape(*index));
                                }
                                *index += 2;
                                let low_hex: String = chars[*index..*index + 4].iter().collect();
                                *index += 4;
                                let low = u32::from_str_radix(&low_hex, 16)
                                    .map_err(|_| JsonParseError::InvalidEscape(*index - 4))?;
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err(JsonParseError::InvalidEscape(*index - 4));
                                }
                                let scalar = 0x1_0000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                                let ch = char::from_u32(scalar)
                                    .ok_or(JsonParseError::InvalidEscape(*index - 4))?;
                                s.push(ch);
                            }
                            0xDC00..=0xDFFF => {
                                return Err(JsonParseError::InvalidEscape(*index - 4));
                            }
                            _ => {
                                let ch = char::from_u32(code)
                                    .ok_or(JsonParseError::InvalidEscape(*index - 4))?;
                                s.push(ch);
                            }
                        }
                    }
                    _ => return Err(JsonParseError::InvalidEscape(*index - 1)),
                }
            }
            c if c <= '\u{001f}' => {
                return Err(JsonParseError::InvalidControlCharacter(*index - 1));
            }
            c => s.push(c),
        }
    }
    Err(JsonParseError::UnexpectedEnd)
}

fn parse_bool(chars: &[char], index: &mut usize) -> Result<JsonValue, JsonParseError> {
    if chars[*index..].starts_with(&['t', 'r', 'u', 'e']) {
        *index += 4;
        Ok(JsonValue::Bool(true))
    } else if chars[*index..].starts_with(&['f', 'a', 'l', 's', 'e']) {
        *index += 5;
        Ok(JsonValue::Bool(false))
    } else {
        Err(JsonParseError::UnexpectedChar(chars[*index], *index))
    }
}

fn parse_null(chars: &[char], index: &mut usize) -> Result<JsonValue, JsonParseError> {
    if chars[*index..].starts_with(&['n', 'u', 'l', 'l']) {
        *index += 4;
        Ok(JsonValue::Null)
    } else {
        Err(JsonParseError::UnexpectedChar(chars[*index], *index))
    }
}

fn parse_number(chars: &[char], index: &mut usize) -> Result<JsonValue, JsonParseError> {
    let start = *index;
    if chars[*index] == '-' {
        *index += 1;
    }
    if *index >= chars.len() {
        return Err(JsonParseError::InvalidNumber(start));
    }

    match chars[*index] {
        '0' => {
            *index += 1;
            if *index < chars.len() && chars[*index].is_ascii_digit() {
                return Err(JsonParseError::InvalidNumber(start));
            }
        }
        '1'..='9' => {
            *index += 1;
            while *index < chars.len() && chars[*index].is_ascii_digit() {
                *index += 1;
            }
        }
        _ => return Err(JsonParseError::InvalidNumber(start)),
    }

    let mut is_float = false;
    if *index < chars.len() && chars[*index] == '.' {
        is_float = true;
        *index += 1;
        let fraction_start = *index;
        while *index < chars.len() && chars[*index].is_ascii_digit() {
            *index += 1;
        }
        if *index == fraction_start {
            return Err(JsonParseError::InvalidNumber(start));
        }
    }

    if *index < chars.len() && matches!(chars[*index], 'e' | 'E') {
        is_float = true;
        *index += 1;
        if *index < chars.len() && matches!(chars[*index], '+' | '-') {
            *index += 1;
        }
        let exponent_start = *index;
        while *index < chars.len() && chars[*index].is_ascii_digit() {
            *index += 1;
        }
        if *index == exponent_start {
            return Err(JsonParseError::InvalidNumber(start));
        }
    }
    let num_str: String = chars[start..*index].iter().collect();
    if is_float {
        let value = num_str
            .parse::<f64>()
            .map_err(|_| JsonParseError::InvalidNumber(start))?;
        if !value.is_finite() {
            return Err(JsonParseError::InvalidNumber(start));
        }
        Ok(JsonValue::Number(value))
    } else {
        num_str
            .parse::<i64>()
            .map(JsonValue::Integer)
            .map_err(|_| JsonParseError::InvalidNumber(start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_serialize_json() {
        let text =
            r#"{"name":"test","count":42,"flags":[true,false,null],"nested":{"key":"val\"ue\n"}}"#;
        let val = JsonValue::parse(text).expect("valid json");

        assert_eq!(val.get("name").and_then(JsonValue::as_str), Some("test"));
        assert_eq!(val.get("count").and_then(JsonValue::as_i64), Some(42));

        let flags = val
            .get("flags")
            .and_then(JsonValue::as_array)
            .expect("array");
        assert_eq!(flags.len(), 3);
        assert_eq!(flags[0].as_bool(), Some(true));
        assert_eq!(flags[1].as_bool(), Some(false));
        assert_eq!(flags[2], JsonValue::Null);

        let nested = val.get("nested").expect("nested object");
        assert_eq!(
            nested.get("key").and_then(JsonValue::as_str),
            Some("val\"ue\n")
        );

        let serialized = val.to_json_string();
        let reparsed = JsonValue::parse(&serialized).expect("reparsed valid json");
        assert_eq!(val, reparsed);
    }

    #[test]
    fn parse_escaped_unicode() {
        let text = r#"{"chinese":"\u4e16\u754c"}"#;
        let val = JsonValue::parse(text).expect("valid json");
        assert_eq!(val.get("chinese").and_then(JsonValue::as_str), Some("世界"));
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(JsonValue::parse("").is_err());
        assert!(JsonValue::parse("{").is_err());
        assert!(JsonValue::parse(r#"{"key":}"#).is_err());
        assert!(JsonValue::parse(r#"{"key": "unclosed}"#).is_err());
        assert!(JsonValue::parse(r#"[1, 2,]"#).is_err());
        assert!(JsonValue::parse(r#"{"a": 1} trailing"#).is_err());
    }

    #[test]
    fn rejects_duplicate_object_keys_at_any_nesting_level() {
        assert!(JsonValue::parse(r#"{"key":1,"key":2}"#).is_err());
        assert!(JsonValue::parse(r#"{"outer":{"key":1,"key":2}}"#).is_err());
        assert!(JsonValue::parse(r#"{"key":1,"\u006bey":2}"#).is_err());
    }

    #[test]
    fn accepts_only_json_whitespace() {
        for whitespace in [" ", "\t", "\r", "\n"] {
            assert!(JsonValue::parse(&format!("{whitespace}1{whitespace}")).is_ok());
        }
        for whitespace in ['\u{000b}', '\u{000c}', '\u{0085}', '\u{00a0}', '\u{2003}'] {
            assert!(JsonValue::parse(&format!("{whitespace}1{whitespace}")).is_err());
        }
    }

    #[test]
    fn rejects_non_standard_numbers() {
        for number in ["01", "-01", "00.1", "1.", "1e", "1e+", "1e-"] {
            assert!(
                JsonValue::parse(number).is_err(),
                "accepted invalid number {number}"
            );
        }
        assert!(JsonValue::parse("+1").is_err());
        for number in ["0", "-0", "10", "-10", "0.1", "1.0", "1e10", "1E-2"] {
            assert!(
                JsonValue::parse(number).is_ok(),
                "rejected valid number {number}"
            );
        }
    }

    #[test]
    fn rejects_numbers_that_cannot_be_represented_as_finite_values() {
        assert!(JsonValue::parse("1e999").is_err());
        assert!(JsonValue::parse("-1e999").is_err());
    }

    #[test]
    fn rejects_raw_control_characters_in_strings() {
        for code in 0..=0x1f {
            let control = char::from_u32(code).expect("control character");
            let input = format!("{{\"key\":\"{control}\"}}");
            assert!(
                JsonValue::parse(&input).is_err(),
                "accepted raw U+{code:04X}"
            );
        }
    }

    #[test]
    fn parses_unicode_surrogate_pairs() {
        let value = JsonValue::parse(r#"{"emoji":"\uD83D\uDE00"}"#).expect("valid surrogate pair");
        assert_eq!(value.get("emoji").and_then(JsonValue::as_str), Some("😀"));
    }

    #[test]
    fn rejects_unpaired_unicode_surrogates() {
        assert!(JsonValue::parse(r#"{"emoji":"\uD83D"}"#).is_err());
        assert!(JsonValue::parse(r#"{"emoji":"\uDE00"}"#).is_err());
    }
}
