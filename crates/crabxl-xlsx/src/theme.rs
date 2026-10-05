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
            Event::Start(ref element) | Event::Empty(ref element) => {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ThemeNode {
    Root,
    Elements,
    Colors,
    Color(usize),
    ColorValue,
    Fonts,
    Major,
    Minor,
    Other,
}
fn theme_attribute(
    element: &quick_xml::events::BytesStart<'_>,
    key: &[u8],
) -> Result<Option<Box<str>>> {
    let mut result = None;
    crate::metadata::attributes(element, |name, value| {
        if name == key {
            result = Some(value.into());
        }
        Ok(())
    })?;
    Ok(result)
}
fn required_theme_attribute(
    element: &quick_xml::events::BytesStart<'_>,
    key: &[u8],
) -> Result<Box<str>> {
    theme_attribute(element, key)?
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Required theme attribute is absent"))
}
fn theme_rgb(value: &str) -> Result<crabxl_core::ArgbLiteral> {
    if value.len() != 6 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Theme RGB requires six hexadecimal digits",
        ));
    }
    crabxl_core::ArgbLiteral::parse(value)
}
fn theme_typeface(
    element: &quick_xml::events::BytesStart<'_>,
) -> Result<crabxl_core::ThemeTypeface> {
    let byte = |key| -> Result<Option<u8>> {
        theme_attribute(element, key)?
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| Error::new(ErrorKind::InvalidData, "Invalid theme typeface byte"))
            })
            .transpose()
    };
    Ok(crabxl_core::ThemeTypeface {
        typeface: required_theme_attribute(element, b"typeface")?,
        panose: theme_attribute(element, b"panose")?,
        pitch_family: byte(b"pitchFamily")?,
        charset: byte(b"charset")?,
    })
}

/// Decode palette and font identities on explicit request, preserving opaque
/// unknown sections separately. Unsupported selected color transforms fail
/// rather than returning an incorrect resolved RGB value.
pub(crate) fn catalog(
    bytes: &[u8],
    part: &str,
    limits: ResourceLimits,
    maximum: usize,
) -> Result<crabxl_core::ThemeCatalog> {
    validate(bytes, part, limits)?;
    let work = || -> Result<crabxl_core::ThemeCatalog> {
        use crabxl_core::{ThemeCatalog, ThemeColor, ThemeScriptFont};
        let mut result = ThemeCatalog::default();
        let mut xml = XmlStream::new(
            Cursor::new(bytes),
            part.to_owned(),
            bytes.len() as u64,
            limits,
        );
        let mut stack = Vec::new();
        let mut root = false;
        let mut elements = false;
        let mut colors = false;
        let mut fonts = false;
        let mut major = false;
        let mut minor = false;
        let mut seen_colors = [false; 12];
        let mut charged = size_of::<ThemeCatalog>();
        loop {
            let frame = xml.next()?;
            match frame.event {
                Event::Start(ref element) | Event::Empty(ref element) => {
                    let parent = stack.last().copied().unwrap_or(ThemeNode::Other);
                    let local = element.local_name();
                    let name = local.as_ref().as_bytes();
                    let mut node = ThemeNode::Other;
                    let modeled = !root
                        || frame.scope == Scope::Drawing
                            && (matches!(
                                (parent, name),
                                (ThemeNode::Elements, b"clrScheme" | b"fontScheme")
                                    | (ThemeNode::Color(_), b"srgbClr" | b"sysClr")
                                    | (
                                        ThemeNode::Major | ThemeNode::Minor,
                                        b"latin" | b"ea" | b"cs" | b"font"
                                    )
                            ));
                    if modeled {
                        // Attribute lexical bytes conservatively cover decoded owned
                        // strings, avoiding a quadratic full-catalog scan per record.
                        charged = charged.saturating_add(element.len());
                        if charged > maximum {
                            return Err(Error::new(
                                ErrorKind::MemoryBudgetExceeded,
                                "Typed theme catalog exceeds allowance",
                            ));
                        }
                    }

                    if !root {
                        // validate() has already checked the non-empty root;
                        // explicitly permit an empty valid root as an empty catalog.
                        if frame.scope != Scope::Drawing || name != b"theme" {
                            return Err(Error::new(
                                ErrorKind::InvalidData,
                                "Theme root or namespace is invalid",
                            ));
                        }
                        root = true;
                        node = ThemeNode::Root;
                        result.name = theme_attribute(element, b"name")?;
                    } else if frame.scope == Scope::Drawing {
                        match (parent, name) {
                            (ThemeNode::Root, b"themeElements") if !elements => {
                                elements = true;
                                node = ThemeNode::Elements;
                            }
                            (ThemeNode::Elements, b"clrScheme") if !colors => {
                                colors = true;
                                node = ThemeNode::Colors;
                                result.color_scheme_name = theme_attribute(element, b"name")?;
                            }
                            (ThemeNode::Elements, b"fontScheme") if !fonts => {
                                fonts = true;
                                node = ThemeNode::Fonts;
                                result.font_scheme_name = theme_attribute(element, b"name")?;
                            }
                            (ThemeNode::Fonts, b"majorFont") if !major => {
                                major = true;
                                node = ThemeNode::Major;
                            }
                            (ThemeNode::Fonts, b"minorFont") if !minor => {
                                minor = true;
                                node = ThemeNode::Minor;
                            }
                            (ThemeNode::Colors, _) => {
                                let names: [&[u8]; 12] = [
                                    b"lt1",
                                    b"dk1",
                                    b"lt2",
                                    b"dk2",
                                    b"accent1",
                                    b"accent2",
                                    b"accent3",
                                    b"accent4",
                                    b"accent5",
                                    b"accent6",
                                    b"hlink",
                                    b"folHlink",
                                ];
                                if let Some(index) =
                                    names.iter().position(|candidate| *candidate == name)
                                {
                                    if seen_colors[index] {
                                        return Err(Error::new(
                                            ErrorKind::InvalidData,
                                            "Duplicate theme color slot",
                                        ));
                                    }
                                    seen_colors[index] = true;
                                    node = ThemeNode::Color(index);
                                }
                            }
                            (ThemeNode::Color(index), b"srgbClr" | b"sysClr") => {
                                if result.colors[index].is_some() {
                                    return Err(Error::new(
                                        ErrorKind::InvalidData,
                                        "Duplicate theme color value",
                                    ));
                                }
                                let value = required_theme_attribute(element, b"val")?;
                                result.colors[index] = Some(if name == b"srgbClr" {
                                    ThemeColor::Rgb(theme_rgb(&value)?)
                                } else {
                                    ThemeColor::System {
                                        name: value,
                                        last_color: theme_attribute(element, b"lastClr")?
                                            .map(|value| theme_rgb(&value))
                                            .transpose()?,
                                    }
                                });
                                node = ThemeNode::ColorValue;
                            }
                            (ThemeNode::Color(_), _) | (ThemeNode::ColorValue, _) => {
                                return Err(Error::new(
                                    ErrorKind::Unsupported,
                                    "Theme color choice or transform is not implemented",
                                ));
                            }
                            (ThemeNode::Major | ThemeNode::Minor, _) => {
                                let collection = if parent == ThemeNode::Major {
                                    &mut result.major_fonts
                                } else {
                                    &mut result.minor_fonts
                                };
                                match name {
                                    b"latin" | b"ea" | b"cs" => {
                                        let slot = match name {
                                            b"latin" => &mut collection.latin,
                                            b"ea" => &mut collection.east_asian,
                                            _ => &mut collection.complex_script,
                                        };
                                        if slot.is_some() {
                                            return Err(Error::new(
                                                ErrorKind::InvalidData,
                                                "Duplicate theme typeface",
                                            ));
                                        }
                                        *slot = Some(theme_typeface(element)?);
                                    }
                                    b"font" => {
                                        if collection.supplemental.len() >= limits.max_style_records
                                        {
                                            return Err(Error::new(
                                                ErrorKind::LimitExceeded,
                                                "Theme font record limit exceeded",
                                            ));
                                        }
                                        if collection.supplemental.len()
                                            == collection.supplemental.capacity()
                                        {
                                            let capacity = collection.supplemental.capacity();
                                            let target = capacity
                                                .saturating_mul(2)
                                                .max(4)
                                                .min(limits.max_style_records);
                                            let slots = target.saturating_sub(capacity);
                                            if charged.saturating_add(
                                                slots.saturating_mul(size_of::<ThemeScriptFont>()),
                                            ) > maximum
                                            {
                                                return Err(Error::new(
                                                    ErrorKind::MemoryBudgetExceeded,
                                                    "Typed theme font storage exceeds allowance",
                                                ));
                                            }
                                            collection
                                                .supplemental
                                                .try_reserve_exact(slots)
                                                .map_err(|error| {
                                                    Error::caused_by(
                                                        ErrorKind::MemoryBudgetExceeded,
                                                        "Cannot reserve theme font mapping",
                                                        error,
                                                    )
                                                })?;
                                            charged = charged.saturating_add(
                                                (collection.supplemental.capacity() - capacity)
                                                    .saturating_mul(size_of::<ThemeScriptFont>()),
                                            );
                                        }
                                        collection.supplemental.push(ThemeScriptFont {
                                            script: required_theme_attribute(element, b"script")?,
                                            typeface: required_theme_attribute(
                                                element,
                                                b"typeface",
                                            )?,
                                        });
                                    }
                                    _ => {}
                                }
                            }
                            (ThemeNode::Root, b"themeElements")
                            | (ThemeNode::Elements, b"clrScheme" | b"fontScheme")
                            | (ThemeNode::Fonts, b"majorFont" | b"minorFont") => {
                                return Err(Error::new(
                                    ErrorKind::InvalidData,
                                    "Duplicate theme scheme",
                                ));
                            }
                            _ => {}
                        }
                    }
                    if charged > maximum {
                        return Err(Error::new(
                            ErrorKind::MemoryBudgetExceeded,
                            "Typed theme catalog exceeds allowance",
                        ));
                    }
                    if matches!(frame.event, Event::Start(_)) {
                        stack.push(node);
                    }
                }
                Event::End(_) => {
                    stack.pop();
                }
                Event::Eof => return Ok(result),
                _ => {}
            }
        }
    };
    work().map_err(|error| error.with_part(part))
}
