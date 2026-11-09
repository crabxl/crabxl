//! Chart element ordering adapted from umya-spreadsheet 3.1.0.
//! Copyright (c) 2020 MathNya. MIT; see third_party/licenses/umya-spreadsheet-MIT.txt.
use super::{DrawingAnchor, XmlContent, XmlElement};
use crate::{Error, ErrorKind, Result};

/// Baseline Spreadsheet DrawingML chart families.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartKind {
    /// Two-dimensional bars or columns.
    Bar,
    /// Three-dimensional bars or columns.
    Bar3D,
    /// Line chart.
    Line,
    /// Three-dimensional line chart.
    Line3D,
    /// Area chart.
    Area,
    /// Three-dimensional area chart.
    Area3D,
    /// Pie chart.
    Pie,
    /// Three-dimensional pie chart.
    Pie3D,
    /// Doughnut chart.
    Doughnut,
    /// Pie-of-pie or bar-of-pie chart.
    OfPie,
    /// XY scatter chart.
    Scatter,
    /// Bubble chart.
    Bubble,
    /// Radar chart.
    Radar,
    /// Stock chart.
    Stock,
    /// Surface chart.
    Surface,
    /// Three-dimensional surface chart.
    Surface3D,
}
impl ChartKind {
    /// Local XML element name.
    pub const fn element_name(self) -> &'static str {
        match self {
            Self::Bar => "barChart",
            Self::Bar3D => "bar3DChart",
            Self::Line => "lineChart",
            Self::Line3D => "line3DChart",
            Self::Area => "areaChart",
            Self::Area3D => "area3DChart",
            Self::Pie => "pieChart",
            Self::Pie3D => "pie3DChart",
            Self::Doughnut => "doughnutChart",
            Self::OfPie => "ofPieChart",
            Self::Scatter => "scatterChart",
            Self::Bubble => "bubbleChart",
            Self::Radar => "radarChart",
            Self::Stock => "stockChart",
            Self::Surface => "surfaceChart",
            Self::Surface3D => "surface3DChart",
        }
    }
    /// Resolve a chart-family element.
    pub fn from_element_name(name: &str) -> Option<Self> {
        [
            Self::Bar,
            Self::Bar3D,
            Self::Line,
            Self::Line3D,
            Self::Area,
            Self::Area3D,
            Self::Pie,
            Self::Pie3D,
            Self::Doughnut,
            Self::OfPie,
            Self::Scatter,
            Self::Bubble,
            Self::Radar,
            Self::Stock,
            Self::Surface,
            Self::Surface3D,
        ]
        .into_iter()
        .find(|kind| kind.element_name() == name)
    }
    fn has_axes(self) -> bool {
        !matches!(self, Self::Pie | Self::Pie3D | Self::Doughnut | Self::OfPie)
    }
    fn has_series_axis(self) -> bool {
        matches!(
            self,
            Self::Line3D | Self::Area3D | Self::Surface | Self::Surface3D
        )
    }
}

/// A chart and its complete editable property tree, including combinations,
/// axes, series, layout, labels and unknown extension subtrees.
#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    /// Worksheet placement.
    pub anchor: DrawingAnchor,
    /// Complete `chartSpace` element. Namespace declarations belong to this root.
    pub space: XmlElement,
    /// Nonvisual drawing name.
    pub name: Box<str>,
    /// Nonvisual graphic-frame properties retained from a loaded drawing.
    pub frame: Option<XmlElement>,
    /// Graphic-frame hyperlinks and other drawing-level dependencies.
    pub drawing_parts: super::PartGraph,
    /// Anchor client flags retained from a loaded drawing.
    pub client_data: Option<XmlElement>,
    /// Drawing stacking order; equal values preserve insertion order.
    pub order: usize,
    /// Relationships referenced by chartSpace.
    pub relationships: Vec<super::PartRelationship>,
    /// Embedded workbooks, styles, colors and other relationship-resolved dependencies.
    pub parts: Vec<super::RelatedPart>,
    /// Original root identity, used only for graph edges pointing back to the chart.
    pub source_part: Option<Box<str>>,
}

pub(super) fn node(name: &str, children: Vec<XmlElement>) -> XmlElement {
    XmlElement {
        name: name.into(),
        attributes: Vec::new(),
        children: children.into_iter().map(XmlContent::Element).collect(),
    }
}
pub(super) fn val(name: &str, value: impl ToString) -> XmlElement {
    let mut element = XmlElement::new(name);
    element.set_attribute("val", value.to_string());
    element
}
fn text(name: &str, value: impl Into<Box<str>>) -> XmlElement {
    XmlElement {
        name: name.into(),
        attributes: Vec::new(),
        children: vec![XmlContent::Text(value.into())],
    }
}
fn axis(kind: &str, id: u32, cross: u32, position: &str) -> XmlElement {
    let mut axis = node(
        kind,
        vec![
            val("c:axId", id),
            node("c:scaling", vec![val("c:orientation", "minMax")]),
            val("c:axPos", position),
            val("c:crossAx", cross),
            val("c:crosses", "autoZero"),
        ],
    );
    if kind == "c:catAx" {
        axis.children
            .push(XmlContent::Element(val("c:lblOffset", 100)));
    }
    axis
}
impl Chart {
    /// Create an empty chart with default axes. Add series before saving.
    pub fn new(kind: ChartKind, anchor: DrawingAnchor) -> Result<Self> {
        anchor.validate()?;
        let mut family = XmlElement::new(format!("c:{}", kind.element_name()));
        let mut add = |element| family.children.push(XmlContent::Element(element));
        match kind {
            ChartKind::Bar | ChartKind::Bar3D => {
                add(val("c:barDir", "col"));
                add(val("c:grouping", "clustered"));
            }
            ChartKind::Line | ChartKind::Line3D | ChartKind::Area | ChartKind::Area3D => {
                add(val("c:grouping", "standard"))
            }
            ChartKind::Scatter => add(val("c:scatterStyle", "lineMarker")),
            ChartKind::Radar => add(val("c:radarStyle", "standard")),
            ChartKind::OfPie => add(val("c:ofPieType", "pie")),
            ChartKind::Doughnut => add(val("c:holeSize", 10)),
            ChartKind::Surface | ChartKind::Surface3D => add(val("c:wireframe", 0)),
            _ => {}
        }
        if kind.has_axes() {
            add(val("c:axId", 10));
            add(val("c:axId", 100));
            if kind.has_series_axis() {
                add(val("c:axId", 1000));
            }
        }
        let mut plot = node("c:plotArea", vec![XmlElement::new("c:layout"), family]);
        if kind.has_axes() {
            plot.children.push(XmlContent::Element(axis(
                if matches!(kind, ChartKind::Scatter | ChartKind::Bubble) {
                    "c:valAx"
                } else {
                    "c:catAx"
                },
                10,
                100,
                "b",
            )));
            let mut value_axis = axis("c:valAx", 100, 10, "l");
            if matches!(kind, ChartKind::Area | ChartKind::Area3D) {
                value_axis
                    .children
                    .insert(3, XmlContent::Element(XmlElement::new("c:majorGridlines")));
            }
            plot.children.push(XmlContent::Element(value_axis));
            if kind.has_series_axis() {
                plot.children
                    .push(XmlContent::Element(axis("c:serAx", 1000, 10, "b")));
            }
        }
        let mut body = Vec::new();
        if matches!(
            kind,
            ChartKind::Bar3D | ChartKind::Surface | ChartKind::Surface3D
        ) {
            let view = if kind == ChartKind::Surface {
                vec![
                    val("c:rotX", 90),
                    val("c:rotY", 0),
                    val("c:rAngAx", 0),
                    val("c:perspective", 0),
                ]
            } else {
                vec![val("c:rotX", 15), val("c:rotY", 20), val("c:rAngAx", 1)]
            };
            body.push(node("c:view3D", view));
        }
        body.extend([plot, val("c:plotVisOnly", 1), val("c:dispBlanksAs", "gap")]);
        let mut space = node("c:chartSpace", vec![node("c:chart", body)]);
        space.set_attribute(
            "xmlns:c",
            "http://schemas.openxmlformats.org/drawingml/2006/chart",
        );
        space.set_attribute(
            "xmlns:a",
            "http://schemas.openxmlformats.org/drawingml/2006/main",
        );
        space.set_attribute(
            "xmlns:r",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        );
        Ok(Self {
            anchor,
            space,
            name: "Chart".into(),
            frame: None,
            drawing_parts: Default::default(),
            client_data: None,
            order: 0,
            relationships: Vec::new(),
            parts: Vec::new(),
            source_part: None,
        })
    }
    /// Chart families in source order, including combined charts.
    pub fn kinds(&self) -> impl Iterator<Item = ChartKind> + '_ {
        self.space
            .child("chart")
            .and_then(|chart| chart.child("plotArea"))
            .into_iter()
            .flat_map(|plot| plot.children.iter())
            .filter_map(|child| match child {
                XmlContent::Element(element) => ChartKind::from_element_name(element.local_name()),
                _ => None,
            })
    }
    /// Add an independently editable series to a chart family.
    /// Category, value and bubble-size references remain formulas; this does not calculate caches.
    pub fn add_series(
        &mut self,
        family: usize,
        values: &str,
        categories: Option<&str>,
        title: Option<&str>,
        bubble_sizes: Option<&str>,
    ) -> Result<()> {
        let plot = self
            .space
            .child_mut("chart")
            .and_then(|chart| chart.child_mut("plotArea"))
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Chart has no plot area"))?;
        let mut index = 0u32;
        for family in &plot.children {
            if let XmlContent::Element(family) = family {
                for series in &family.children {
                    if let XmlContent::Element(series) = series
                        && series.local_name() == "ser"
                    {
                        let existing: u32 = series
                            .child("idx")
                            .and_then(|value| value.attribute("val"))
                            .ok_or_else(|| {
                                Error::new(ErrorKind::InvalidData, "Chart series has no identity")
                            })?
                            .parse()
                            .map_err(|_| {
                                Error::new(ErrorKind::InvalidData, "Invalid chart series identity")
                            })?;
                        index = index.max(existing.checked_add(1).ok_or_else(|| {
                            Error::new(
                                ErrorKind::LimitExceeded,
                                "Chart series identities exhausted",
                            )
                        })?);
                    }
                }
            }
        }
        let family = plot
            .children
            .iter_mut()
            .filter_map(|child| match child {
                XmlContent::Element(element)
                    if ChartKind::from_element_name(element.local_name()).is_some() =>
                {
                    Some(element)
                }
                _ => None,
            })
            .nth(family)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown chart family index"))?;
        let kind = ChartKind::from_element_name(family.local_name())
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown chart family"))?;
        if values.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Empty chart value reference",
            ));
        }
        if kind == ChartKind::Bubble && bubble_sizes.is_none() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Bubble chart requires size values",
            ));
        }
        if matches!(kind, ChartKind::Scatter | ChartKind::Bubble) && categories.is_none()
            || kind != ChartKind::Bubble && bubble_sizes.is_some()
            || categories.is_some_and(str::is_empty)
            || bubble_sizes.is_some_and(str::is_empty)
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid chart category or bubble-size reference",
            ));
        }
        let mut series = node("c:ser", vec![val("c:idx", index), val("c:order", index)]);
        if let Some(title) = title {
            series
                .children
                .push(XmlContent::Element(node("c:tx", vec![text("c:v", title)])));
        }
        if matches!(
            kind,
            ChartKind::Line | ChartKind::Scatter | ChartKind::Radar | ChartKind::Stock
        ) {
            series.children.push(XmlContent::Element(node(
                "c:marker",
                vec![val("c:symbol", "none")],
            )));
        }
        let reference = |name: &str, formula: &str, numeric: bool| {
            node(
                name,
                vec![node(
                    if numeric { "c:numRef" } else { "c:strRef" },
                    vec![text("c:f", formula.trim_start_matches('='))],
                )],
            )
        };
        let xy = matches!(kind, ChartKind::Scatter | ChartKind::Bubble);
        if let Some(categories) = categories {
            series.children.push(XmlContent::Element(reference(
                if xy { "c:xVal" } else { "c:cat" },
                categories,
                xy,
            )));
        }
        series.children.push(XmlContent::Element(reference(
            if xy { "c:yVal" } else { "c:val" },
            values,
            true,
        )));
        if let Some(sizes) = bubble_sizes {
            series
                .children
                .push(XmlContent::Element(reference("c:bubbleSize", sizes, true)));
        }
        let before = family.children.iter().position(|child| matches!(child, XmlContent::Element(element) if !matches!(element.local_name(), "barDir" | "grouping" | "scatterStyle" | "radarStyle" | "ofPieType" | "wireframe" | "varyColors" | "ser"))).unwrap_or(family.children.len());
        family.children.insert(before, XmlContent::Element(series));
        Ok(())
    }
    /// Retained property and drawing payload size.
    pub fn heap_bytes(&self) -> usize {
        self.drawing_parts.heap_bytes()
            + self
                .parts
                .iter()
                .map(super::RelatedPart::heap_bytes)
                .sum::<usize>()
            + self
                .relationships
                .iter()
                .map(|rel| {
                    size_of::<super::PartRelationship>()
                        + rel.id.len()
                        + rel.kind.len()
                        + rel.target.len()
                })
                .sum::<usize>()
            + self.space.heap_bytes()
            + self.name.len()
            + self.frame.as_ref().map_or(0, XmlElement::heap_bytes)
            + self.client_data.as_ref().map_or(0, XmlElement::heap_bytes)
    }
}
