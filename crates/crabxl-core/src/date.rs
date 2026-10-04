// SPDX-License-Identifier: MIT
// Epoch/leapday conversion adapted from rust_xlsxwriter's chrono_date_to_excel,
// Copyright 2022-2026 John McNamara. See third_party/ports.json.
use crate::{Error, ErrorKind, Result};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, Timelike};

/// Workbook serial-date origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateEpoch {
    /// Excel's Windows origin, including its fictitious 1900 leap day.
    Windows1900,
    /// Excel's Mac origin, 1904-01-01.
    Mac1904,
}
/// Interpretation of a date/time serial value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateKind {
    /// Calendar date without a time component.
    Date,
    /// Calendar date and time without a timezone.
    DateTime,
    /// Time within one day.
    Time,
    /// Elapsed time, potentially negative or longer than one day.
    Duration,
}
/// Exact finite XLSX serial and its interpretation. No timezone is implied.
/// Serial 60 in the Windows epoch is retained as Excel's fictitious leap day;
/// converting that value to another calendar epoch is rejected.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExcelDateTime {
    serial: f64,
    epoch: DateEpoch,
    kind: DateKind,
    source: DateSource,
}

// Literal values keep their native precision/calendar identity; loaded serials
// retain their exact source and use the baseline conversion policy.
#[derive(Clone, Copy, Debug, PartialEq)]
enum DateSource {
    Serial,
    Calendar(NaiveDateTime),
    Date(NaiveDate),
    Clock(NaiveTime),
    Elapsed(TimeDelta),
}
impl ExcelDateTime {
    /// Construct a serial; clock values lie in [0, 1), date-only values use integral days.
    pub fn from_serial(serial: f64, epoch: DateEpoch, kind: DateKind) -> Result<Self> {
        if !serial.is_finite()
            || (kind == DateKind::Time && !(0.0..1.0).contains(&serial))
            || (kind == DateKind::Date && serial.fract() != 0.0)
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid date/time serial",
            ));
        }
        Ok(Self {
            serial,
            epoch,
            kind,
            source: DateSource::Serial,
        })
    }
    /// Construct a Gregorian calendar date and millisecond-resolution time.
    /// Invalid dates, leap seconds and years outside 1..=9999 are rejected.
    pub fn from_ymd_hms_milli(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        millisecond: u32,
    ) -> Result<Self> {
        let microsecond = millisecond
            .checked_mul(1000)
            .filter(|_| millisecond < 1000)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid millisecond field"))?;
        Self::from_ymd_hms_micro(year, month, day, hour, minute, second, microsecond)
    }
    /// Construct a literal Gregorian datetime preserving microsecond precision
    /// and original calendar identity, including early ambiguous serial dates.
    pub fn from_ymd_hms_micro(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        microsecond: u32,
    ) -> Result<Self> {
        let date = NaiveDate::from_ymd_opt(year, month, day)
            .filter(|_| (1..=9999).contains(&year))
            .and_then(|date| date.and_hms_micro_opt(hour, minute, second, microsecond))
            .filter(|date| date.nanosecond() < 1_000_000_000)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid calendar date/time"))?;
        Ok(Self {
            serial: calendar_serial(date, DateEpoch::Windows1900)?,
            epoch: DateEpoch::Windows1900,
            kind: DateKind::DateTime,
            source: DateSource::Calendar(date),
        })
    }
    /// Construct a literal Gregorian date without a clock component.
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Result<Self> {
        let date = NaiveDate::from_ymd_opt(year, month, day)
            .filter(|_| (1..=9999).contains(&year))
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid calendar date"))?;
        let midnight = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid midnight"))?;
        Ok(Self {
            serial: calendar_serial(midnight, DateEpoch::Windows1900)?,
            epoch: DateEpoch::Windows1900,
            kind: DateKind::Date,
            source: DateSource::Date(date),
        })
    }
    /// Construct a literal clock time preserving microsecond precision.
    pub fn from_hms_micro(hour: u32, minute: u32, second: u32, microsecond: u32) -> Result<Self> {
        let time = NaiveTime::from_hms_micro_opt(hour, minute, second, microsecond)
            .filter(|time| time.nanosecond() < 1_000_000_000)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid clock time"))?;
        let serial = (f64::from(time.num_seconds_from_midnight())
            + f64::from(microsecond) / 1_000_000.0)
            / 86400.0;
        Ok(Self {
            serial,
            epoch: DateEpoch::Windows1900,
            kind: DateKind::Time,
            source: DateSource::Clock(time),
        })
    }
    /// Construct a literal elapsed duration from normalized Python-compatible
    /// days, seconds within a day, and microseconds within a second.
    pub fn from_duration_parts(days: i64, seconds: u32, microseconds: u32) -> Result<Self> {
        if !(-999_999_999..=999_999_999).contains(&days)
            || seconds >= 86400
            || microseconds >= 1_000_000
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid elapsed duration components",
            ));
        }
        let duration = TimeDelta::try_days(days)
            .and_then(|value| value.checked_add(&TimeDelta::seconds(i64::from(seconds))))
            .and_then(|value| value.checked_add(&TimeDelta::microseconds(i64::from(microseconds))))
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Elapsed duration overflows"))?;
        // Match public timedelta.total_seconds/day conversion while retaining
        // exact literal components for language adapters and subsequent edits.
        let serial = (duration.num_seconds() as f64
            + f64::from(duration.subsec_micros()) / 1_000_000.0)
            / 86400.0;
        Ok(Self {
            serial,
            epoch: DateEpoch::Windows1900,
            kind: DateKind::Duration,
            source: DateSource::Elapsed(duration),
        })
    }
    /// The retained source serial.
    pub const fn serial(self) -> f64 {
        self.serial
    }
    /// The retained source epoch.
    pub const fn epoch(self) -> DateEpoch {
        self.epoch
    }
    /// The serial's interpretation.
    pub const fn kind(self) -> DateKind {
        self.kind
    }
    /// Convert the calendar serial to a destination workbook epoch. Times and
    /// durations are epoch-independent. Conversion preserves fractional days.
    pub fn serial_in(self, epoch: DateEpoch) -> Result<f64> {
        if let DateSource::Calendar(date) = self.source {
            return calendar_serial(date, epoch);
        }
        if let DateSource::Date(date) = self.source {
            let midnight = date
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid midnight"))?;
            return calendar_serial(midnight, epoch);
        }
        if self.epoch == epoch || !matches!(self.kind, DateKind::Date | DateKind::DateTime) {
            return Ok(self.serial);
        }
        if self.epoch == DateEpoch::Windows1900 && self.serial.floor() == 60.0 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Fictitious 1900-02-29 cannot change epoch",
            ));
        }
        Ok(match self.epoch {
            DateEpoch::Windows1900 => {
                self.serial - 1462.0
                    + if self.serial > 0.0 && self.serial < 60.0 {
                        1.0
                    } else {
                        0.0
                    }
            }
            DateEpoch::Mac1904 => {
                let days = self.serial + 1462.0;
                days - if days.floor() > 0.0 && days.floor() <= 60.0 {
                    1.0
                } else {
                    0.0
                }
            }
        })
    }
    /// Convert to a calendar datetime rounded to milliseconds, matching the
    /// pinned public baseline. Windows serial 60 maps to 1900-02-28.
    /// Time and duration kinds have no calendar datetime.
    pub fn to_datetime(self) -> Result<NaiveDateTime> {
        if !matches!(self.kind, DateKind::Date | DateKind::DateTime) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Value is not a calendar datetime",
            ));
        }
        if let DateSource::Calendar(value) = self.source {
            return Ok(value);
        }
        if let DateSource::Date(value) = self.source {
            return value
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid midnight"));
        }
        let epoch = match self.epoch {
            DateEpoch::Windows1900 => NaiveDate::from_ymd_opt(1899, 12, 30),
            DateEpoch::Mac1904 => NaiveDate::from_ymd_opt(1904, 1, 1),
        }
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid date epoch"))?;
        let day = self.serial.floor();
        if day < i64::MIN as f64 || day >= i64::MAX as f64 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Calendar serial out of range",
            ));
        }
        let offset =
            if self.epoch == DateEpoch::Windows1900 && self.serial > 0.0 && self.serial < 60.0 {
                1
            } else {
                0
            };
        let days = (day as i64)
            .checked_add(offset)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Calendar day overflows"))?;
        let millis = self.fraction_milliseconds();
        chrono::Duration::try_days(days)
            .and_then(|duration| epoch.checked_add_signed(duration))
            .and_then(|date| date.checked_add_signed(chrono::Duration::milliseconds(millis)))
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Calendar serial out of range"))
    }
    /// Convert a date/date-time value to its Gregorian calendar day.
    pub fn to_date(self) -> Result<NaiveDate> {
        self.to_datetime().map(|value| value.date())
    }
    /// Format calendar/clock values using baseline ISO milliseconds. Literal
    /// sub-millisecond fractions are truncated in this representation.
    pub fn to_iso8601(self) -> Result<String> {
        let (mut output, microseconds) = match self.kind {
            DateKind::Date => return Ok(self.to_date()?.format("%Y-%m-%d").to_string()),
            DateKind::DateTime => {
                let value = self.to_datetime()?;
                (
                    value.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    value.nanosecond() / 1000,
                )
            }
            DateKind::Time => {
                let value = self.to_time()?;
                (
                    value.format("%H:%M:%S").to_string(),
                    value.nanosecond() / 1000,
                )
            }
            DateKind::Duration => {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Elapsed durations use numeric XLSX storage",
                ));
            }
        };
        if microseconds != 0 {
            use std::fmt::Write;
            write!(&mut output, ".{:03}", microseconds / 1000).map_err(|e| {
                Error::caused_by(ErrorKind::InvalidData, "Cannot format ISO fraction", e)
            })?;
        }
        Ok(output)
    }
    /// Fractional-day milliseconds with baseline ties-to-even rounding.
    /// Gregorian day arithmetic stays separate to avoid losing sub-millisecond precision.
    pub fn fraction_milliseconds(self) -> i64 {
        (((self.serial - self.serial.floor()) * 86400.0) * 1000.0).round_ties_even() as i64
    }
    /// Convert a time-only value to millisecond-resolution clock time.
    pub fn to_time(self) -> Result<chrono::NaiveTime> {
        if self.kind != DateKind::Time {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Value is not a clock time",
            ));
        }
        if let DateSource::Clock(value) = self.source {
            return Ok(value);
        }
        let millis = self.fraction_milliseconds();
        if !(0..86_400_000).contains(&millis) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Clock rounding crosses a day boundary",
            ));
        }
        chrono::NaiveTime::from_num_seconds_from_midnight_opt(
            (millis / 1000) as u32,
            ((millis % 1000) * 1_000_000) as u32,
        )
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid clock time"))
    }
    /// Convert elapsed days to millisecond-resolution duration without applying an epoch.
    pub fn to_duration(self) -> Result<chrono::Duration> {
        if self.kind != DateKind::Duration {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Value is not an elapsed duration",
            ));
        }
        if let DateSource::Elapsed(value) = self.source {
            return Ok(value);
        }
        let millis = (self.serial * 86_400_000.0).round_ties_even();
        if millis < i64::MIN as f64 || millis >= i64::MAX as f64 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Duration serial out of range",
            ));
        }
        chrono::Duration::try_milliseconds(millis as i64)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Duration serial out of range"))
    }
    /// Whether conversion fits the pinned reference's calendar/duration value range.
    /// Raw serial retention remains available independently of this compatibility check.
    pub fn is_reference_representable(self) -> bool {
        match self.kind {
            DateKind::Date | DateKind::DateTime => self
                .to_datetime()
                .is_ok_and(|date| (1..=9999).contains(&chrono::Datelike::year(&date))),
            DateKind::Time => self.to_time().is_ok(),
            DateKind::Duration => self.to_duration().is_ok_and(|duration| {
                (-999_999_999..=999_999_999)
                    .contains(&duration.num_milliseconds().div_euclid(86_400_000))
            }),
        }
    }
}

fn calendar_serial(date: NaiveDateTime, epoch: DateEpoch) -> Result<f64> {
    let origin = match epoch {
        DateEpoch::Windows1900 => NaiveDate::from_ymd_opt(1899, 12, 30),
        DateEpoch::Mac1904 => NaiveDate::from_ymd_opt(1904, 1, 1),
    }
    .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid date epoch"))?;
    let mut days = (date.date() - origin).num_days();
    if epoch == DateEpoch::Windows1900 && days > 0 && days <= 60 {
        days -= 1;
    }
    let fraction = (f64::from(date.num_seconds_from_midnight())
        + f64::from(date.nanosecond()) / 1_000_000_000.0)
        / 86400.0;
    Ok(days as f64 + fraction)
}
