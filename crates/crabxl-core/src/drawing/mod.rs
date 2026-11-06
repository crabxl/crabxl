//! Drawing anchors and image payloads shared by XLSX readers and writers.
mod chart;
mod edit_as;
mod element;
mod marker;
mod related;
pub use chart::{Chart, ChartKind};
pub use edit_as::DrawingEditAs;
pub use element::{XmlContent, XmlElement};
pub use marker::DrawingMarker;
pub use related::{PartGraph, PartRelationship, RelatedPart};

use crate::{Error, ErrorKind, Result};

/// Size in English Metric Units (914400 EMUs per inch).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawingExtent {
    /// Width.
    pub cx: i64,
    /// Height.
    pub cy: i64,
}
impl DrawingExtent {
    /// Convert pixels at 96 DPI to EMUs, as in umya's image constructor.
    pub fn pixels(width: u32, height: u32) -> Self {
        Self {
            cx: i64::from(width) * 9525,
            cy: i64::from(height) * 9525,
        }
    }
    /// Reject negative sizes.
    pub fn validate(&self) -> Result<()> {
        if self.cx < 0 || self.cy < 0 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Negative drawing extent",
            ));
        }
        Ok(())
    }
}

/// Absolute, one-cell or two-cell placement of a picture or chart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DrawingAnchor {
    /// Fixed position and size in EMUs.
    Absolute {
        /// Horizontal position.
        x: i64,
        /// Vertical position.
        y: i64,
        /// Drawing size.
        extent: DrawingExtent,
    },
    /// Cell-relative position and fixed size.
    OneCell {
        /// Top-left marker.
        from: DrawingMarker,
        /// Drawing size.
        extent: DrawingExtent,
    },
    /// Two cell-relative corners.
    TwoCell {
        /// Top-left marker.
        from: DrawingMarker,
        /// Bottom-right marker.
        to: DrawingMarker,
        /// Optional resizing policy; absence retains the format default.
        edit_as: Option<DrawingEditAs>,
    },
}
impl DrawingAnchor {
    /// Validate cell positions and sizes without changing the model.
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Absolute { extent, .. } => extent.validate(),
            Self::OneCell { from, extent } => {
                from.validate()?;
                extent.validate()
            }
            Self::TwoCell { from, to, .. } => {
                from.validate()?;
                to.validate()
            }
        }
    }
}

/// Image formats supported without decoding or re-encoding image bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    /// Portable Network Graphics.
    Png,
    /// JPEG.
    Jpeg,
    /// GIF.
    Gif,
    /// Windows bitmap.
    Bmp,
    /// TIFF.
    Tiff,
    /// Enhanced Metafile.
    Emf,
    /// Windows Metafile.
    Wmf,
    /// Scalable Vector Graphics.
    Svg,
}
impl ImageFormat {
    /// Conventional package extension.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Gif => "gif",
            Self::Bmp => "bmp",
            Self::Tiff => "tiff",
            Self::Emf => "emf",
            Self::Wmf => "wmf",
            Self::Svg => "svg",
        }
    }
    /// OPC content type.
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Bmp => "image/bmp",
            Self::Tiff => "image/tiff",
            Self::Emf => "image/x-emf",
            Self::Wmf => "image/x-wmf",
            Self::Svg => "image/svg+xml",
        }
    }
    /// Resolve a package media content type independently of its filename.
    pub fn from_content_type(value: &str) -> Result<Self> {
        [
            Self::Png,
            Self::Jpeg,
            Self::Gif,
            Self::Bmp,
            Self::Tiff,
            Self::Emf,
            Self::Wmf,
            Self::Svg,
        ]
        .into_iter()
        .find(|format| format.content_type() == value)
        .ok_or_else(|| Error::new(ErrorKind::Unsupported, "Unsupported image content type"))
    }
    /// Resolve a media filename extension.
    pub fn from_extension(extension: &str) -> Result<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "png" => Ok(Self::Png),
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            "gif" => Ok(Self::Gif),
            "bmp" => Ok(Self::Bmp),
            "tif" | "tiff" => Ok(Self::Tiff),
            "emf" => Ok(Self::Emf),
            "wmf" => Ok(Self::Wmf),
            "svg" => Ok(Self::Svg),
            _ => Err(Error::new(
                ErrorKind::Unsupported,
                "Unsupported image format",
            )),
        }
    }
}

/// One picture. Clones share immutable image bytes; replacing bytes isolates edits.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    /// Drawing placement.
    pub anchor: DrawingAnchor,
    /// Image encoding.
    pub format: ImageFormat,
    /// Encoded image payload.
    pub data: crate::BinaryPayload,
    /// Nonvisual drawing name.
    pub name: Box<str>,
    /// Accessible description.
    pub description: Option<Box<str>>,
    /// Original picture properties, retained when editing image bytes or placement.
    pub properties: Option<XmlElement>,
    /// Picture-level hyperlinks and other relationships beyond its image payload.
    pub drawing_parts: PartGraph,
    /// Drawing client flags retained from the anchor.
    pub client_data: Option<XmlElement>,
    /// Drawing stacking order; equal values preserve insertion order.
    pub order: usize,
}
impl Image {
    /// Create a picture with explicit dimensions; no image decoder is required.
    pub fn new(
        data: impl Into<crate::BinaryPayload>,
        format: ImageFormat,
        anchor: DrawingAnchor,
    ) -> Result<Self> {
        anchor.validate()?;
        let data = data.into();
        if data.is_empty() {
            return Err(Error::new(ErrorKind::InvalidData, "Empty image payload"));
        }
        Ok(Self {
            anchor,
            format,
            data,
            name: "Picture".into(),
            description: None,
            properties: None,
            drawing_parts: PartGraph::default(),
            client_data: None,
            order: 0,
        })
    }
    /// Retained payload size, including shared bytes once for this picture.
    pub fn heap_bytes(&self) -> usize {
        self.drawing_parts.heap_bytes()
            + self
                .client_data
                .as_ref()
                .map_or(0, XmlElement::heap_bytes)
                .saturating_add(self.properties.as_ref().map_or(0, XmlElement::heap_bytes))
                .saturating_add(self.data.memory_bytes())
                .saturating_add(self.name.len())
                .saturating_add(self.description.as_ref().map_or(0, |v| v.len()))
    }
}

/// A shape, connector, grouped object or other anchored DrawingML object.
/// Its complete XML and related parts stay editable without flattening unknown properties.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    /// Worksheet placement.
    pub anchor: DrawingAnchor,
    /// The anchored object element, including namespace declarations.
    pub properties: XmlElement,
    /// Anchor client flags.
    pub client_data: Option<XmlElement>,
    /// Drawing stacking order.
    pub order: usize,
    /// Relationships referenced by the object.
    pub relationships: Vec<PartRelationship>,
    /// Immutable object dependencies.
    pub parts: Vec<RelatedPart>,
    /// Original drawing part, for back references.
    pub source_part: Option<Box<str>>,
}
impl Shape {
    /// Create an anchored object. XML and dependency references are validated on output.
    pub fn new(anchor: DrawingAnchor, properties: XmlElement) -> Result<Self> {
        anchor.validate()?;
        Ok(Self {
            anchor,
            properties,
            client_data: None,
            order: 0,
            relationships: Vec::new(),
            parts: Vec::new(),
            source_part: None,
        })
    }
    /// Retained metadata and payload allocation.
    pub fn heap_bytes(&self) -> usize {
        self.properties.heap_bytes()
            + self.client_data.as_ref().map_or(0, XmlElement::heap_bytes)
            + self.source_part.as_ref().map_or(0, |part| part.len())
            + self
                .parts
                .iter()
                .map(RelatedPart::heap_bytes)
                .sum::<usize>()
            + self
                .relationships
                .iter()
                .map(|rel| {
                    size_of::<PartRelationship>() + rel.id.len() + rel.kind.len() + rel.target.len()
                })
                .sum::<usize>()
    }
}
