//! Bounded public-baseline ISO date/clock/duration prefix semantics.
//! Original implementation from public API observations, not reference source.
use crate::{Error, ErrorKind, ExcelDateTime, Result};

fn invalid() -> Error {
    Error::new(ErrorKind::InvalidData, "Invalid ISO date/time value")
}
fn digits(value: &[u8], start: usize, length: usize) -> Option<u32> {
    let bytes = value.get(start..start.checked_add(length)?)?;
    bytes.iter().try_fold(0u32, |v, b| {
        b.is_ascii_digit()
            .then_some(v * 10 + u32::from(b.saturating_sub(b'0')))
    })
}
fn clock(value: &[u8]) -> Result<Option<(u32, u32, u32, u32)>> {
    let Some((hour, minute)) = digits(value, 0, 2)
        .zip(digits(value, 3, 2))
        .filter(|_| value.get(2) == Some(&b':'))
    else {
        return Ok(None);
    };
    let seconds = if value.get(5) == Some(&b':') {
        digits(value, 6, 2).unwrap_or(0)
    } else {
        0
    };
    let mut microseconds = 0;
    if value.get(5) == Some(&b':') && digits(value, 6, 2).is_some() && value.get(8) == Some(&b'.') {
        let mut length = 0;
        while length < 3 && value.get(9 + length).is_some_and(u8::is_ascii_digit) {
            length += 1;
        }
        if length > 0 {
            microseconds =
                digits(value, 9, length).ok_or_else(invalid)? * 10u32.pow(6 - length as u32);
        }
    }
    // Construction performs finite calendar/clock range validation.
    Ok(Some((hour, minute, seconds, microseconds)))
}
fn component(
    value: &[u8],
    start: usize,
    suffix: u8,
    fractional: bool,
) -> Result<Option<(u64, u32, usize)>> {
    let mut at = start;
    while value.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    if at == start {
        return Ok(None);
    }
    let integer_end = at;
    let mut microseconds = 0;
    if fractional && value.get(at) == Some(&b'.') {
        at += 1;
        let fraction_start = at;
        while value.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        let length = at - fraction_start;
        if !(1..=3).contains(&length) {
            return Ok(None);
        }
        microseconds = digits(value, fraction_start, length).ok_or_else(invalid)?
            * 10u32.pow(6 - length as u32);
    }
    if value.get(at) != Some(&suffix) {
        return Ok(None);
    }
    let integer = value[start..integer_end]
        .iter()
        .try_fold(0u64, |v, b| {
            v.checked_mul(10)?.checked_add(u64::from(b - b'0'))
        })
        .ok_or_else(invalid)?;
    Ok(Some((integer, microseconds, at + 1)))
}
/// Parse pinned public ISO prefix behavior without reading reference source.
/// Empty text is absent. Recognized calendar/clock prefixes ignore trailing
/// text, including zone suffixes, as the compatibility baseline does. Only
/// hour/minute/second elapsed durations are supported; no day/year notation.
pub fn parse_iso8601(value: &str) -> Result<Option<ExcelDateTime>> {
    if value.is_empty() {
        return Ok(None);
    }
    let b = value.as_bytes();
    if let Some(((year, month), day)) = digits(b, 0, 4)
        .zip(digits(b, 5, 2))
        .zip(digits(b, 8, 2))
        .filter(|_| b.get(4) == Some(&b'-') && b.get(7) == Some(&b'-'))
    {
        if b.get(10) == Some(&b'T')
            && let Some((h, m, s, u)) = clock(&b[11..])?
        {
            return ExcelDateTime::from_ymd_hms_micro(year as i32, month, day, h, m, s, u)
                .map(Some);
        }
        return ExcelDateTime::from_ymd(year as i32, month, day).map(Some);
    }
    if let Some((h, m, s, u)) = clock(b)? {
        return ExcelDateTime::from_hms_micro(h, m, s, u).map(Some);
    }
    if b.starts_with(b"PT") {
        let mut at = 2;
        let mut seconds = 0u64;
        let mut microseconds = 0;
        let mut found = false;
        for (suffix, multiplier, fractional) in
            [(b'H', 3600, false), (b'M', 60, false), (b'S', 1, true)]
        {
            if let Some((integer, micro, next)) = component(b, at, suffix, fractional)? {
                seconds = seconds
                    .checked_add(integer.checked_mul(multiplier).ok_or_else(invalid)?)
                    .ok_or_else(invalid)?;
                microseconds = micro;
                at = next;
                found = true;
            }
        }
        if found {
            let days = i64::try_from(seconds / 86400).map_err(|_| invalid())?;
            return ExcelDateTime::from_duration_parts(
                days,
                (seconds % 86400) as u32,
                microseconds,
            )
            .map(Some);
        }
    }
    Err(invalid())
}
