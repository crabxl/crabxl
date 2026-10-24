//! Verify public ISO observations without a language-runtime implementation.
use crabxl::{DateKind, parse_iso8601};
use std::io::{self, BufRead};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        match parse_iso8601(&line?) {
            Ok(None) => println!("NoneType|None"),
            Ok(Some(value)) => match value.kind() {
                DateKind::Date => println!("date|{}", value.to_iso8601()?),
                DateKind::DateTime => println!("datetime|{}", value.to_iso8601()?),
                DateKind::Time => println!("time|{}", value.to_iso8601()?),
                DateKind::Duration => {
                    println!("timedelta|{}", value.to_duration()?.num_milliseconds())
                }
            },
            Err(_) => println!("error"),
        }
    }
    Ok(())
}
