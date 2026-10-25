//! Core, extended and custom document properties.
//!
//! Modelled after openpyxl `packaging/core.py`, `extended.py` and `custom.py`
//! (openpyxl, MIT). Dates are kept as W3CDTF text (`2024-01-02T03:04:05Z`).

/// Value of one custom document property.
#[derive(Clone, Debug, PartialEq)]
pub enum CustomPropertyValue {
    /// `vt:lpwstr` text.
    String(Box<str>),
    /// `vt:i4` / `vt:i8` integer.
    Int(i64),
    /// `vt:r8` floating point number.
    Float(f64),
    /// `vt:bool`.
    Bool(bool),
    /// `vt:filetime` as ISO-8601 text.
    DateTime(Box<str>),
    /// Property linked to a defined name or range (`linkTarget`).
    Link(Box<str>),
}

impl Default for CustomPropertyValue {
    fn default() -> Self {
        Self::String("".into())
    }
}

/// One custom document property.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CustomProperty {
    /// Property name.
    pub name: Box<str>,
    /// Typed value.
    pub value: CustomPropertyValue,
}

impl CustomProperty {
    /// Create a property.
    pub fn new(name: &str, value: CustomPropertyValue) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

/// Core, extended and custom properties of a workbook. `default()` means
/// "nothing set".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentProperties {
    /// `dc:creator`.
    pub creator: Option<Box<str>>,
    /// `dc:title`.
    pub title: Option<Box<str>>,
    /// `dc:description`.
    pub description: Option<Box<str>>,
    /// `dc:subject`.
    pub subject: Option<Box<str>>,
    /// `dc:identifier`.
    pub identifier: Option<Box<str>>,
    /// `dc:language`.
    pub language: Option<Box<str>>,
    /// `dcterms:created` (W3CDTF text).
    pub created: Option<Box<str>>,
    /// `dcterms:modified` (W3CDTF text).
    pub modified: Option<Box<str>>,
    /// `cp:lastModifiedBy`.
    pub last_modified_by: Option<Box<str>>,
    /// `cp:category`.
    pub category: Option<Box<str>>,
    /// `cp:contentStatus`.
    pub content_status: Option<Box<str>>,
    /// `cp:version`.
    pub version: Option<Box<str>>,
    /// `cp:revision`.
    pub revision: Option<Box<str>>,
    /// `cp:keywords`.
    pub keywords: Option<Box<str>>,
    /// `cp:lastPrinted` (W3CDTF text).
    pub last_printed: Option<Box<str>>,
    /// Extended `Application`.
    pub application: Option<Box<str>>,
    /// Extended `AppVersion`.
    pub app_version: Option<Box<str>>,
    /// Extended `Company`.
    pub company: Option<Box<str>>,
    /// Extended `Manager`.
    pub manager: Option<Box<str>>,
    /// Extended `Template`.
    pub template: Option<Box<str>>,
    /// Extended `PresentationFormat`.
    pub presentation_format: Option<Box<str>>,
    /// Extended `HyperlinkBase`.
    pub hyperlink_base: Option<Box<str>>,
    /// Extended `DocSecurity`.
    pub doc_security: Option<u32>,
    /// Extended `ScaleCrop`.
    pub scale_crop: Option<bool>,
    /// Extended `LinksUpToDate`.
    pub links_up_to_date: Option<bool>,
    /// Extended `SharedDoc`.
    pub shared_doc: Option<bool>,
    /// Extended `HyperlinksChanged`.
    pub hyperlinks_changed: Option<bool>,
    /// Extended `Pages`.
    pub pages: Option<u32>,
    /// Extended `Words`.
    pub words: Option<u32>,
    /// Extended `Characters`.
    pub characters: Option<u32>,
    /// Extended `CharactersWithSpaces`.
    pub characters_with_spaces: Option<u32>,
    /// Extended `Lines`.
    pub lines: Option<u32>,
    /// Extended `Paragraphs`.
    pub paragraphs: Option<u32>,
    /// Extended `Slides`.
    pub slides: Option<u32>,
    /// Extended `Notes`.
    pub notes: Option<u32>,
    /// Extended `TotalTime`.
    pub total_time: Option<u32>,
    /// Extended `HiddenSlides`.
    pub hidden_slides: Option<u32>,
    /// Extended `MMClips`.
    pub mm_clips: Option<u32>,
    /// Custom properties in document order.
    pub custom: Vec<CustomProperty>,
}

impl DocumentProperties {
    /// The properties openpyxl 3.1.5 writes for a fresh workbook: creator
    /// `openpyxl`, `created`/`modified` set to `now_iso` and the openpyxl
    /// application name and version.
    pub fn with_openpyxl_defaults(now_iso: &str) -> Self {
        Self {
            creator: Some("openpyxl".into()),
            created: Some(now_iso.into()),
            modified: Some(now_iso.into()),
            application: Some("Microsoft Excel Compatible / Openpyxl 3.1.5".into()),
            app_version: Some("3.1".into()),
            ..Self::default()
        }
    }

    /// True when nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Find a custom property by name.
    pub fn custom_property(&self, name: &str) -> Option<&CustomProperty> {
        self.custom.iter().find(|p| &*p.name == name)
    }

    /// Set or replace a custom property.
    pub fn set_custom_property(&mut self, name: &str, value: CustomPropertyValue) {
        match self.custom.iter_mut().find(|p| &*p.name == name) {
            Some(existing) => existing.value = value,
            None => self.custom.push(CustomProperty::new(name, value)),
        }
    }

    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        let texts = [
            &self.creator,
            &self.title,
            &self.description,
            &self.subject,
            &self.identifier,
            &self.language,
            &self.created,
            &self.modified,
            &self.last_modified_by,
            &self.category,
            &self.content_status,
            &self.version,
            &self.revision,
            &self.keywords,
            &self.last_printed,
            &self.application,
            &self.app_version,
            &self.company,
            &self.manager,
            &self.template,
            &self.presentation_format,
            &self.hyperlink_base,
        ];
        texts
            .into_iter()
            .map(|v| v.as_ref().map_or(0, |s| s.len()))
            .sum::<usize>()
            + self.custom.capacity() * std::mem::size_of::<CustomProperty>()
            + self
                .custom
                .iter()
                .map(|p| {
                    p.name.len()
                        + match &p.value {
                            CustomPropertyValue::String(s)
                            | CustomPropertyValue::DateTime(s)
                            | CustomPropertyValue::Link(s) => s.len(),
                            _ => 0,
                        }
                })
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn default_is_empty_and_defaults_are_openpyxl_like() {
        assert!(DocumentProperties::default().is_empty());
        let d = DocumentProperties::with_openpyxl_defaults("2024-01-02T03:04:05Z");
        assert_eq!(d.creator.as_deref(), Some("openpyxl"));
        assert_eq!(d.created, d.modified);
        assert!(d.heap_bytes() > 0);
    }

    #[test]
    fn custom_set_replaces() {
        let mut d = DocumentProperties::default();
        d.set_custom_property("a", CustomPropertyValue::Int(1));
        d.set_custom_property("a", CustomPropertyValue::Bool(true));
        assert_eq!(d.custom.len(), 1);
        assert_eq!(
            d.custom_property("a").map(|p| &p.value),
            Some(&CustomPropertyValue::Bool(true))
        );
    }
}
