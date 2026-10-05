//! Bounded validation of opaque themes; no eager drawing graph materialization.
use crate::xml::{Scope, XmlStream, append_xml_text};
use crabxl_core::{Error, ErrorKind, ResourceLimits, Result, Theme};
use quick_xml::events::Event;
use std::io::Cursor;

/// Theme emission policy for newly created packages.
#[derive(Clone, Debug, Default)]
pub enum ThemeWritePolicy {
    /// Emit the public reference's default theme from static storage.
    #[default]
    ReferenceDefault,
    /// Preserve opaque caller bytes, matching the public loaded_theme property.
    /// Empty bytes select the reference default theme.
    Custom(Theme),
    /// Require a bounded DrawingML theme document before preserving custom bytes.
    Validated(Theme),
    /// Omit the theme explicitly; unresolved references keep their identities.
    Omit,
}
impl ThemeWritePolicy {
    pub(crate) fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::ReferenceDefault => Some(crate::default_theme::DEFAULT_THEME.as_bytes()),
            Self::Custom(theme) if theme.bytes().is_empty() => {
                Some(crate::default_theme::DEFAULT_THEME.as_bytes())
            }
            Self::Custom(theme) | Self::Validated(theme) => Some(theme.bytes()),
            Self::Omit => None,
        }
    }
    pub(crate) fn memory_bytes(&self) -> usize {
        match self {
            Self::Custom(theme) | Self::Validated(theme) => theme.memory_bytes(),
            _ => 0,
        }
    }
}
pub(crate) fn validate(bytes: &[u8], part: &str, limits: ResourceLimits) -> Result<()> {
    if bytes.len() > limits.max_theme_bytes || bytes.len() as u64 > limits.max_part_bytes {
        return Err(
            Error::new(ErrorKind::LimitExceeded, "Theme byte allowance exceeded").with_part(part),
        );
    }
    let mut xml = XmlStream::new(
        Cursor::new(bytes),
        part.to_owned(),
        bytes.len() as u64,
        limits,
    );
    let mut root = false;
    let mut text = String::new();
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(ref element) => {
                if !root {
                    if frame.scope != Scope::Drawing
                        || element.local_name().as_ref().as_bytes() != b"theme"
                    {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            "Theme root or namespace is invalid",
                        )
                        .with_part(part));
                    }
                    root = true;
                }
                for attribute in element.attributes() {
                    attribute
                        .map_err(|error| {
                            Error::caused_by(ErrorKind::Xml, "Invalid theme attribute", error)
                                .with_part(part)
                        })?
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|error| {
                            Error::caused_by(ErrorKind::Xml, "Invalid theme attribute value", error)
                                .with_part(part)
                        })?;
                }
            }
            event @ (Event::Text(_) | Event::CData(_) | Event::GeneralRef(_)) => {
                text.clear();
                append_xml_text(&mut text, &event, limits.max_xml_event_bytes)
                    .map_err(|error| error.with_part(part))?;
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}
