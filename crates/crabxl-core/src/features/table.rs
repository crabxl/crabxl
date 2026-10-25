//! Worksheet tables.
//!
//! Mirrors `openpyxl/worksheet/table.py` (attributes in snake_case).
use super::auto_filter::{AutoFilter, SortState};

macro_rules! xml_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($(#[$vmeta])* $variant,)+
        }
        impl $name {
            /// Parse the XML attribute value.
            pub fn from_xml(value: &str) -> Option<Self> {
                match value {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
            /// The XML attribute value.
            pub fn as_xml(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)+
                }
            }
        }
    };
}

xml_enum! {
    /// Table source kind (`tableType`).
    TableType {
        /// Worksheet range.
        Worksheet = "worksheet",
        /// XML mapped.
        Xml = "xml",
        /// Query table.
        QueryTable = "queryTable",
    }
}

xml_enum! {
    /// Totals row function (`totalsRowFunction`).
    TotalsRowFunction {
        /// Sum.
        Sum = "sum",
        /// Minimum.
        Min = "min",
        /// Maximum.
        Max = "max",
        /// Average.
        Average = "average",
        /// Count.
        Count = "count",
        /// Count numbers.
        CountNums = "countNums",
        /// Standard deviation.
        StdDev = "stdDev",
        /// Variance.
        Var = "var",
        /// Custom formula.
        Custom = "custom",
    }
}

fn opt_text(value: &Option<Box<str>>) -> usize {
    value.as_deref().map_or(0, str::len)
}

/// Table style options (`<tableStyleInfo>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableStyleInfo {
    /// Style name, e.g. `TableStyleMedium9`.
    pub name: Option<Box<str>>,
    /// Emphasize first column (`showFirstColumn`).
    pub show_first_column: Option<bool>,
    /// Emphasize last column (`showLastColumn`).
    pub show_last_column: Option<bool>,
    /// Banded rows (`showRowStripes`).
    pub show_row_stripes: Option<bool>,
    /// Banded columns (`showColumnStripes`).
    pub show_column_stripes: Option<bool>,
}
impl TableStyleInfo {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        opt_text(&self.name)
    }
}

/// XML column mapping (`<xmlColumnPr>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct XmlColumnProps {
    /// Map id (`mapId`).
    pub map_id: u32,
    /// XPath (`xpath`).
    pub xpath: Box<str>,
    /// Denormalized (`denormalized`).
    pub denormalized: Option<bool>,
    /// XML data type (`xmlDataType`).
    pub xml_data_type: Box<str>,
}
impl XmlColumnProps {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        self.xpath.len() + self.xml_data_type.len()
    }
}

/// Formula element of a table column; the formula is the text content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableFormula {
    /// Array formula (`array`).
    pub array: Option<bool>,
    /// Formula text.
    pub text: Box<str>,
}

/// Table column (`<tableColumn>`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableColumn {
    /// Column id (`id`).
    pub id: u32,
    /// Unique name (`uniqueName`).
    pub unique_name: Option<Box<str>>,
    /// Column name (`name`).
    pub name: Box<str>,
    /// Totals row function (`totalsRowFunction`).
    pub totals_row_function: Option<TotalsRowFunction>,
    /// Totals row label (`totalsRowLabel`).
    pub totals_row_label: Option<Box<str>>,
    /// Query table field id (`queryTableFieldId`).
    pub query_table_field_id: Option<u32>,
    /// Header row dxf id (`headerRowDxfId`).
    pub header_row_dxf_id: Option<u32>,
    /// Data dxf id (`dataDxfId`).
    pub data_dxf_id: Option<u32>,
    /// Totals row dxf id (`totalsRowDxfId`).
    pub totals_row_dxf_id: Option<u32>,
    /// Header row cell style (`headerRowCellStyle`).
    pub header_row_cell_style: Option<Box<str>>,
    /// Data cell style (`dataCellStyle`).
    pub data_cell_style: Option<Box<str>>,
    /// Totals row cell style (`totalsRowCellStyle`).
    pub totals_row_cell_style: Option<Box<str>>,
    /// Calculated column formula.
    pub calculated_column_formula: Option<TableFormula>,
    /// Totals row formula.
    pub totals_row_formula: Option<TableFormula>,
    /// XML column properties.
    pub xml_column_pr: Option<XmlColumnProps>,
}
impl TableColumn {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        self.name.len()
            + opt_text(&self.unique_name)
            + opt_text(&self.totals_row_label)
            + opt_text(&self.header_row_cell_style)
            + opt_text(&self.data_cell_style)
            + opt_text(&self.totals_row_cell_style)
            + self
                .calculated_column_formula
                .as_ref()
                .map_or(0, |f| f.text.len())
            + self.totals_row_formula.as_ref().map_or(0, |f| f.text.len())
            + self
                .xml_column_pr
                .as_ref()
                .map_or(0, XmlColumnProps::heap_bytes)
    }
}

/// A worksheet table (`xl/tables/tableN.xml`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    /// Table id (`id`); must be unique in the workbook.
    pub id: u32,
    /// Name (`name`).
    pub name: Option<Box<str>>,
    /// Display name (`displayName`); no spaces.
    pub display_name: Box<str>,
    /// Comment (`comment`).
    pub comment: Option<Box<str>>,
    /// Table range (`ref`).
    pub ref_range: Box<str>,
    /// Table type (`tableType`).
    pub table_type: Option<TableType>,
    /// Header row count (`headerRowCount`, default 1).
    pub header_row_count: Option<u32>,
    /// Insert row shown (`insertRow`).
    pub insert_row: Option<bool>,
    /// Insert row shifts cells (`insertRowShift`).
    pub insert_row_shift: Option<bool>,
    /// Totals row count (`totalsRowCount`).
    pub totals_row_count: Option<u32>,
    /// Totals row shown (`totalsRowShown`).
    pub totals_row_shown: Option<bool>,
    /// Published (`published`).
    pub published: Option<bool>,
    /// Header row dxf id.
    pub header_row_dxf_id: Option<u32>,
    /// Data dxf id.
    pub data_dxf_id: Option<u32>,
    /// Totals row dxf id.
    pub totals_row_dxf_id: Option<u32>,
    /// Header row border dxf id.
    pub header_row_border_dxf_id: Option<u32>,
    /// Table border dxf id.
    pub table_border_dxf_id: Option<u32>,
    /// Totals row border dxf id.
    pub totals_row_border_dxf_id: Option<u32>,
    /// Header row cell style name.
    pub header_row_cell_style: Option<Box<str>>,
    /// Data cell style name.
    pub data_cell_style: Option<Box<str>>,
    /// Totals row cell style name.
    pub totals_row_cell_style: Option<Box<str>>,
    /// Connection id (`connectionId`).
    pub connection_id: Option<u32>,
    /// Table auto filter.
    pub auto_filter: Option<AutoFilter>,
    /// Table sort state.
    pub sort_state: Option<SortState>,
    /// Columns.
    pub table_columns: Vec<TableColumn>,
    /// Style options.
    pub table_style_info: Option<TableStyleInfo>,
}
impl Table {
    /// Approximate owned heap bytes.
    pub fn heap_bytes(&self) -> usize {
        self.display_name.len()
            + self.ref_range.len()
            + opt_text(&self.name)
            + opt_text(&self.comment)
            + opt_text(&self.header_row_cell_style)
            + opt_text(&self.data_cell_style)
            + opt_text(&self.totals_row_cell_style)
            + self.auto_filter.as_ref().map_or(0, AutoFilter::heap_bytes)
            + self.sort_state.as_ref().map_or(0, SortState::heap_bytes)
            + self.table_columns.capacity() * size_of::<TableColumn>()
            + self
                .table_columns
                .iter()
                .map(TableColumn::heap_bytes)
                .sum::<usize>()
            + self
                .table_style_info
                .as_ref()
                .map_or(0, TableStyleInfo::heap_bytes)
    }

    /// Column names in order.
    pub fn column_names(&self) -> impl Iterator<Item = &str> {
        self.table_columns.iter().map(|c| &*c.name)
    }
}
