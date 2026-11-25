//! Tables, data validation, conditional formats, charts, page setup, notes,
//! hyperlinks, defined names and images.
//!
//! Each type here is a plain value. Build it, then add it to a worksheet or
//! workbook. Reading returns the same types.

use crate::reference::{CellRef, IntoCellRef, IntoRange, RangeRef, Span};
use crate::style::PartialStyle;
use crate::value::Temporal;
use crate::{Error, ErrorKind, Result};
use crabxl_core::features as core;

/// A comparison for data validation and conditional formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Comparison {
    /// Between two values, inclusive.
    Between,
    /// Outside two values.
    NotBetween,
    /// Equal to a value.
    Equal,
    /// Not equal to a value.
    NotEqual,
    /// Greater than a value.
    GreaterThan,
    /// Greater than or equal to a value.
    GreaterThanOrEqual,
    /// Less than a value.
    LessThan,
    /// Less than or equal to a value.
    LessThanOrEqual,
}

impl Comparison {
    fn needs_two(self) -> bool {
        matches!(self, Self::Between | Self::NotBetween)
    }
    fn validation(self) -> core::ValidationOperator {
        use core::ValidationOperator as O;
        match self {
            Self::Between => O::Between,
            Self::NotBetween => O::NotBetween,
            Self::Equal => O::Equal,
            Self::NotEqual => O::NotEqual,
            Self::GreaterThan => O::GreaterThan,
            Self::GreaterThanOrEqual => O::GreaterThanOrEqual,
            Self::LessThan => O::LessThan,
            Self::LessThanOrEqual => O::LessThanOrEqual,
        }
    }
    fn from_validation(operator: core::ValidationOperator) -> Self {
        use core::ValidationOperator as O;
        match operator {
            O::Between => Self::Between,
            O::NotBetween => Self::NotBetween,
            O::Equal => Self::Equal,
            O::NotEqual => Self::NotEqual,
            O::GreaterThan => Self::GreaterThan,
            O::GreaterThanOrEqual => Self::GreaterThanOrEqual,
            O::LessThan => Self::LessThan,
            O::LessThanOrEqual => Self::LessThanOrEqual,
        }
    }
    fn from_conditional(operator: core::ConditionalOperator) -> Option<Self> {
        use core::ConditionalOperator as O;
        Some(match operator {
            O::Between => Self::Between,
            O::NotBetween => Self::NotBetween,
            O::Equal => Self::Equal,
            O::NotEqual => Self::NotEqual,
            O::GreaterThan => Self::GreaterThan,
            O::GreaterThanOrEqual => Self::GreaterThanOrEqual,
            O::LessThan => Self::LessThan,
            O::LessThanOrEqual => Self::LessThanOrEqual,
            _ => return None,
        })
    }
    fn conditional(self) -> core::ConditionalOperator {
        use core::ConditionalOperator as O;
        match self {
            Self::Between => O::Between,
            Self::NotBetween => O::NotBetween,
            Self::Equal => O::Equal,
            Self::NotEqual => O::NotEqual,
            Self::GreaterThan => O::GreaterThan,
            Self::GreaterThanOrEqual => O::GreaterThanOrEqual,
            Self::LessThan => O::LessThan,
            Self::LessThanOrEqual => O::LessThanOrEqual,
        }
    }
}

fn formula_text(value: impl ToString) -> Box<str> {
    let text = value.to_string();
    text.strip_prefix('=')
        .map_or_else(|| text.clone().into(), Into::into)
}

/// A table: a named range with a header row, optional totals row and style.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub(crate) name: String,
    pub(crate) range: RangeRef,
    pub(crate) style: Option<String>,
    pub(crate) totals_row: bool,
    pub(crate) banded_rows: bool,
}

impl Table {
    /// A table over `range`. The first row of the range is the header row;
    /// its cells name the columns. Table names are unique in a workbook and
    /// must start with a letter or underscore.
    pub fn new(name: &str, range: impl IntoRange) -> Result<Self> {
        let valid = name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
            && name.len() <= 255;
        if !valid {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("'{name}' is not a valid table name"),
            ));
        }
        let range = range.into_range()?;
        if range.height() < 2 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "A table needs a header row and at least one data row",
            ));
        }
        Ok(Self {
            name: name.to_owned(),
            range,
            style: Some("TableStyleMedium2".into()),
            totals_row: false,
            banded_rows: true,
        })
    }
    /// Set the table style, such as `"TableStyleLight9"`, or `None` for no style.
    pub fn with_style(mut self, style: Option<&str>) -> Self {
        self.style = style.map(str::to_owned);
        self
    }
    /// Shade alternate rows (the default).
    pub fn with_banded_rows(mut self, banded: bool) -> Self {
        self.banded_rows = banded;
        self
    }
    /// Make the last row of the range a totals row. The range then needs a
    /// header row, at least one data row and the totals row, whose cells hold
    /// text, a formula or nothing.
    pub fn with_totals_row(mut self, totals: bool) -> Self {
        self.totals_row = totals;
        self
    }
    /// Whether alternate rows are shaded.
    pub fn banded_rows(&self) -> bool {
        self.banded_rows
    }
    /// The table name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The range, including the header row.
    pub fn range(&self) -> RangeRef {
        self.range
    }
    /// The table style name.
    pub fn style(&self) -> Option<&str> {
        self.style.as_deref()
    }
    /// Whether the last row is a totals row.
    pub fn has_totals_row(&self) -> bool {
        self.totals_row
    }
    pub(crate) fn from_core(table: &core::Table) -> Result<Self> {
        Ok(Self {
            name: table.display_name.to_string(),
            range: table.ref_range.parse()?,
            style: table
                .table_style_info
                .as_ref()
                .and_then(|info| info.name.as_deref().map(str::to_owned)),
            totals_row: table.totals_row_count.unwrap_or(0) > 0,
            banded_rows: table
                .table_style_info
                .as_ref()
                .and_then(|info| info.show_row_stripes)
                .unwrap_or(false),
        })
    }
    pub(crate) fn to_core(&self, id: u32, headers: Vec<String>) -> Result<core::Table> {
        // The filter covers the header and data rows, never the totals row.
        let filtered = if self.totals_row {
            RangeRef::new(
                self.range.start(),
                crate::CellRef::new(self.range.end().row() - 1, self.range.end().column())?,
            )?
        } else {
            self.range
        };
        Ok(core::Table {
            id,
            name: Some(self.name.clone().into()),
            display_name: self.name.clone().into(),
            ref_range: self.range.to_string().into(),
            totals_row_count: self.totals_row.then_some(1),
            auto_filter: Some(core::AutoFilter {
                ref_range: Some(filtered.to_string().into()),
                ..Default::default()
            }),
            table_columns: headers
                .into_iter()
                .enumerate()
                .map(|(index, name)| core::TableColumn {
                    id: index as u32 + 1,
                    name: name.into(),
                    ..Default::default()
                })
                .collect(),
            // Written even without a style name, so banding survives a reload.
            table_style_info: Some(core::TableStyleInfo {
                name: self.style.as_ref().map(|style| style.clone().into()),
                show_first_column: Some(false),
                show_last_column: Some(false),
                show_row_stripes: Some(self.banded_rows),
                show_column_stripes: Some(false),
            }),
            ..Default::default()
        })
    }
}

/// What a data validation rule checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ValidationKind {
    /// Any value; the rule only shows its prompt.
    Any,
    /// Whole numbers.
    WholeNumber,
    /// Decimal numbers.
    Decimal,
    /// A drop-down list, of fixed choices or of a range.
    List,
    /// Dates.
    Date,
    /// Times of day.
    Time,
    /// Text length.
    TextLength,
    /// A custom formula.
    Custom,
}

/// What a conditional format tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConditionalKind {
    /// The cell value compared with one or two values.
    CellValue,
    /// A formula that is true for the cell.
    Formula,
    /// Another rule type, such as a color scale or data bar. CrabXL keeps
    /// these rules but does not create them.
    Other,
}

/// A rule that limits what users can type into cells.
#[derive(Clone, Debug, PartialEq)]
pub struct DataValidation {
    pub(crate) inner: core::DataValidation,
}

impl DataValidation {
    fn with_type(
        kind: core::ValidationType,
        comparison: Option<Comparison>,
        first: Box<str>,
        second: Option<Box<str>>,
    ) -> Self {
        Self {
            inner: core::DataValidation {
                validation_type: Some(kind),
                operator: comparison.map(Comparison::validation),
                formula1: Some(first),
                formula2: second,
                allow_blank: Some(true),
                show_error_message: Some(true),
                show_input_message: Some(true),
                ..Default::default()
            },
        }
    }
    /// A drop-down list of fixed choices. The choices cannot contain commas
    /// and together must stay within 255 characters.
    pub fn list<S: AsRef<str>>(choices: impl IntoIterator<Item = S>) -> Result<Self> {
        let choices: Vec<String> = choices.into_iter().map(|c| c.as_ref().to_owned()).collect();
        if choices
            .iter()
            .any(|choice| choice.contains(',') || choice.contains('"'))
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "List choices cannot contain commas or quotes",
            ));
        }
        let joined = format!("\"{}\"", choices.join(","));
        if joined.len() > 257 {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                "List choices exceed 255 characters",
            ));
        }
        Ok(Self::with_type(
            core::ValidationType::List,
            None,
            joined.into(),
            None,
        ))
    }
    /// A drop-down list of the values in a range, such as `"=$H$1:$H$10"`.
    pub fn list_from(source: &str) -> Self {
        Self::with_type(core::ValidationType::List, None, formula_text(source), None)
    }
    /// Whole numbers compared with one or two bounds (two for `Between`).
    pub fn whole_number(comparison: Comparison, first: i64, second: Option<i64>) -> Result<Self> {
        Self::numeric(
            core::ValidationType::Whole,
            comparison,
            first.to_string(),
            second.map(|v| v.to_string()),
        )
    }
    /// Decimal numbers compared with one or two bounds.
    pub fn decimal(comparison: Comparison, first: f64, second: Option<f64>) -> Result<Self> {
        Self::numeric(
            core::ValidationType::Decimal,
            comparison,
            first.to_string(),
            second.map(|v| v.to_string()),
        )
    }
    /// Dates compared with one or two bounds. The bounds are written as
    /// `DATE(year,month,day)` formulas, which work in every date system.
    pub fn date(comparison: Comparison, first: Temporal, second: Option<Temporal>) -> Result<Self> {
        let date = |value: Temporal| -> Result<String> {
            let (year, month, day, ..) = value.to_parts()?;
            Ok(format!("DATE({year},{month},{day})"))
        };
        Self::numeric(
            core::ValidationType::Date,
            comparison,
            date(first)?,
            second.map(date).transpose()?,
        )
    }
    /// Times of day compared with one or two bounds, written as
    /// `TIME(hour,minute,second)` formulas.
    pub fn time(comparison: Comparison, first: Temporal, second: Option<Temporal>) -> Result<Self> {
        let time = |value: Temporal| -> Result<String> {
            let (hour, minute, second, _) = value.to_time_of_day()?;
            Ok(format!("TIME({hour},{minute},{second})"))
        };
        Self::numeric(
            core::ValidationType::Time,
            comparison,
            time(first)?,
            second.map(time).transpose()?,
        )
    }
    /// Text whose length compares with one or two bounds.
    pub fn text_length(comparison: Comparison, first: u32, second: Option<u32>) -> Result<Self> {
        Self::numeric(
            core::ValidationType::TextLength,
            comparison,
            first.to_string(),
            second.map(|v| v.to_string()),
        )
    }
    fn numeric(
        kind: core::ValidationType,
        comparison: Comparison,
        first: String,
        second: Option<String>,
    ) -> Result<Self> {
        if comparison.needs_two() != second.is_some() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Between and NotBetween need two bounds; other comparisons need one",
            ));
        }
        Ok(Self::with_type(
            kind,
            Some(comparison),
            first.into(),
            second.map(Into::into),
        ))
    }
    /// Values for which a formula is true, such as `"=ISNUMBER(A1)"`.
    pub fn custom(formula: &str) -> Self {
        Self::with_type(
            core::ValidationType::Custom,
            None,
            formula_text(formula),
            None,
        )
    }
    /// Apply the rule to a range. Call again to add more ranges.
    pub fn with_range(mut self, range: impl IntoRange) -> Result<Self> {
        self.inner.ranges.push(range.into_range()?.0);
        Ok(self)
    }
    /// Allow empty cells (the default).
    pub fn with_allow_blank(mut self, allow: bool) -> Self {
        self.inner.allow_blank = Some(allow);
        self
    }
    /// Show a message when a user selects the cell.
    pub fn with_prompt(mut self, title: &str, message: &str) -> Self {
        self.inner.prompt_title = Some(title.into());
        self.inner.prompt = Some(message.into());
        self
    }
    /// Show a message when a user types a value the rule rejects.
    pub fn with_error(mut self, title: &str, message: &str) -> Self {
        self.inner.error_title = Some(title.into());
        self.inner.error = Some(message.into());
        self
    }
    /// The ranges the rule applies to.
    pub fn ranges(&self) -> Vec<RangeRef> {
        self.inner.ranges.iter().copied().map(RangeRef).collect()
    }
    /// The first formula or list.
    pub fn formula1(&self) -> Option<&str> {
        self.inner.formula1.as_deref()
    }
    /// The second formula, for `Between` and `NotBetween`.
    pub fn formula2(&self) -> Option<&str> {
        self.inner.formula2.as_deref()
    }
    /// What the rule checks.
    pub fn kind(&self) -> ValidationKind {
        use core::ValidationType as T;
        match self.inner.validation_type.unwrap_or(T::None) {
            T::None => ValidationKind::Any,
            T::Whole => ValidationKind::WholeNumber,
            T::Decimal => ValidationKind::Decimal,
            T::List => ValidationKind::List,
            T::Date => ValidationKind::Date,
            T::Time => ValidationKind::Time,
            T::TextLength => ValidationKind::TextLength,
            T::Custom => ValidationKind::Custom,
        }
    }
    /// The comparison of a number, date, time or text-length rule.
    pub fn comparison(&self) -> Option<Comparison> {
        let numeric = matches!(
            self.kind(),
            ValidationKind::WholeNumber
                | ValidationKind::Decimal
                | ValidationKind::Date
                | ValidationKind::Time
                | ValidationKind::TextLength
        );
        // Excel's default operator is between.
        numeric.then(|| {
            Comparison::from_validation(
                self.inner
                    .operator
                    .unwrap_or(core::ValidationOperator::Between),
            )
        })
    }
    /// Whether empty cells are allowed.
    pub fn allows_blank(&self) -> bool {
        self.inner.allow_blank.unwrap_or(false)
    }
    /// The prompt title and message, shown when a user selects the cell.
    pub fn prompt(&self) -> Option<(&str, &str)> {
        let message = self.inner.prompt.as_deref()?;
        Some((self.inner.prompt_title.as_deref().unwrap_or(""), message))
    }
    /// The error title and message, shown when the rule rejects a value.
    pub fn error(&self) -> Option<(&str, &str)> {
        let message = self.inner.error.as_deref()?;
        Some((self.inner.error_title.as_deref().unwrap_or(""), message))
    }
}

/// A rule that formats cells when a condition holds.
#[derive(Clone, Debug, PartialEq)]
pub struct ConditionalFormat {
    pub(crate) ranges: Vec<RangeRef>,
    pub(crate) rule: core::ConditionalRule,
    pub(crate) style: Option<PartialStyle>,
}

impl ConditionalFormat {
    /// Format cells whose value compares with one or two values or formulas,
    /// such as `ConditionalFormat::cell("B2:B100", Comparison::GreaterThan, "1000", None, style)`.
    pub fn cell(
        range: impl IntoRange,
        comparison: Comparison,
        first: impl ToString,
        second: Option<&str>,
        style: PartialStyle,
    ) -> Result<Self> {
        if comparison.needs_two() != second.is_some() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Between and NotBetween need two values; other comparisons need one",
            ));
        }
        let mut formulas = vec![formula_text(first.to_string())];
        formulas.extend(second.map(formula_text));
        Ok(Self {
            ranges: vec![range.into_range()?],
            rule: core::ConditionalRule {
                rule_type: Some(core::ConditionalRuleType::CellIs),
                operator: Some(comparison.conditional()),
                formulas,
                ..Default::default()
            },
            style: Some(style),
        })
    }
    /// Format cells for which a formula is true. Relative references in the
    /// formula refer to the top-left cell of the range.
    pub fn formula(range: impl IntoRange, formula: &str, style: PartialStyle) -> Result<Self> {
        Ok(Self {
            ranges: vec![range.into_range()?],
            rule: core::ConditionalRule {
                rule_type: Some(core::ConditionalRuleType::Expression),
                formulas: vec![formula_text(formula)],
                ..Default::default()
            },
            style: Some(style),
        })
    }
    /// Stop evaluating lower-priority rules when this one applies.
    pub fn with_stop_if_true(mut self, stop: bool) -> Self {
        self.rule.stop_if_true = Some(stop);
        self
    }
    /// Apply the rule to one more range.
    pub fn with_range(mut self, range: impl IntoRange) -> Result<Self> {
        self.ranges.push(range.into_range()?);
        Ok(self)
    }
    /// The ranges the rule applies to.
    pub fn ranges(&self) -> &[RangeRef] {
        &self.ranges
    }
    /// The rule formulas, without leading `=`.
    pub fn formulas(&self) -> impl Iterator<Item = &str> {
        self.rule.formulas.iter().map(AsRef::as_ref)
    }
    /// What the rule tests.
    pub fn kind(&self) -> ConditionalKind {
        match self.rule.rule_type {
            Some(core::ConditionalRuleType::CellIs) => ConditionalKind::CellValue,
            Some(core::ConditionalRuleType::Expression) => ConditionalKind::Formula,
            _ => ConditionalKind::Other,
        }
    }
    /// The comparison of a cell-value rule.
    pub fn comparison(&self) -> Option<Comparison> {
        if self.kind() != ConditionalKind::CellValue {
            return None;
        }
        self.rule.operator.and_then(Comparison::from_conditional)
    }
    /// The formatting the rule applies.
    pub fn style(&self) -> Option<&PartialStyle> {
        self.style.as_ref()
    }
    /// Whether lower-priority rules stop when this one applies.
    pub fn stops_if_true(&self) -> bool {
        self.rule.stop_if_true.unwrap_or(false)
    }
}

/// A note (a cell comment).
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub(crate) text: String,
    pub(crate) author: String,
}

impl Note {
    /// A note with its text and author.
    pub fn new(text: &str, author: &str) -> Self {
        Self {
            text: text.to_owned(),
            author: author.to_owned(),
        }
    }
    /// The note text.
    pub fn text(&self) -> &str {
        &self.text
    }
    /// The author.
    pub fn author(&self) -> &str {
        &self.author
    }
}

/// A link from a cell to a web address, a file or a place in the workbook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hyperlink {
    /// A web address, mail address or file path.
    Url {
        /// The address.
        target: String,
        /// Text shown when the pointer rests on the cell.
        tooltip: Option<String>,
    },
    /// A cell or defined name in this workbook, such as `"Summary!A1"`.
    Location {
        /// The reference.
        location: String,
        /// Text shown when the pointer rests on the cell.
        tooltip: Option<String>,
    },
}

impl Hyperlink {
    /// A link to a web address, mail address or file path.
    pub fn url(target: &str) -> Result<Self> {
        if target.trim().is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "A hyperlink needs a target",
            ));
        }
        Ok(Self::Url {
            target: target.to_owned(),
            tooltip: None,
        })
    }
    /// A link to a cell or defined name in this workbook.
    pub fn location(location: &str) -> Result<Self> {
        if location.trim().is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "A hyperlink needs a location",
            ));
        }
        Ok(Self::Location {
            location: location.to_owned(),
            tooltip: None,
        })
    }
    /// Set the tooltip.
    pub fn with_tooltip(mut self, text: &str) -> Self {
        match &mut self {
            Self::Url { tooltip, .. } | Self::Location { tooltip, .. } => {
                *tooltip = Some(text.to_owned())
            }
        }
        self
    }
    pub(crate) fn to_core(&self) -> crabxl_core::Hyperlink {
        match self {
            Self::Url { target, tooltip } => crabxl_core::Hyperlink {
                target: Some(target.clone().into()),
                tooltip: tooltip.clone().map(Into::into),
                external: true,
                ..Default::default()
            },
            Self::Location { location, tooltip } => crabxl_core::Hyperlink {
                location: Some(location.clone().into()),
                tooltip: tooltip.clone().map(Into::into),
                ..Default::default()
            },
        }
    }
    pub(crate) fn from_core(link: &crabxl_core::Hyperlink) -> Option<Self> {
        let tooltip = link.tooltip.as_deref().map(str::to_owned);
        if let Some(target) = &link.target {
            return Some(Self::Url {
                target: target.to_string(),
                tooltip,
            });
        }
        link.location.as_ref().map(|location| Self::Location {
            location: location.to_string(),
            tooltip,
        })
    }
}

/// A defined name: a named formula, range or constant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinedName {
    pub(crate) name: String,
    pub(crate) formula: String,
    pub(crate) sheet: Option<String>,
}

impl DefinedName {
    /// A workbook-wide name. The formula is usually a sheet-qualified
    /// absolute reference such as `"Settings!$B$1"`.
    pub fn new(name: &str, formula: &str) -> Result<Self> {
        let valid = name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '\\')
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\'))
            && name.parse::<RangeRef>().is_err();
        if !valid {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("'{name}' is not a valid defined name"),
            ));
        }
        Ok(Self {
            name: name.to_owned(),
            formula: formula_text(formula).into(),
            sheet: None,
        })
    }
    /// Limit the name to one sheet.
    pub fn with_sheet_scope(mut self, sheet: &str) -> Self {
        self.sheet = Some(sheet.to_owned());
        self
    }
    /// The name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The formula, without a leading `=`.
    pub fn formula(&self) -> &str {
        &self.formula
    }
    /// The sheet the name belongs to, or `None` for a workbook-wide name.
    pub fn sheet(&self) -> Option<&str> {
        self.sheet.as_deref()
    }
}

/// A picture to place on a worksheet.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub(crate) data: Vec<u8>,
    pub(crate) format: crabxl_core::ImageFormat,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl Image {
    /// A PNG, JPEG or GIF image from its bytes. The size in pixels comes from
    /// the image header.
    pub fn from_bytes(data: impl Into<Vec<u8>>) -> Result<Self> {
        let data = data.into();
        let (format, width, height) = image_header(&data)
            .ok_or_else(|| Error::new(ErrorKind::Unsupported, "Use a PNG, JPEG or GIF image"))?;
        Ok(Self {
            data,
            format,
            width,
            height,
        })
    }
    /// A PNG, JPEG or GIF image from a file.
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let data = std::fs::read(path).map_err(|error| {
            Error::caused_by(ErrorKind::Io, "Cannot read the image file", error)
        })?;
        Self::from_bytes(data)
    }
    /// The encoded bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    /// Width and height in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub(crate) fn to_core(&self, at: impl IntoCellRef) -> Result<crabxl_core::Image> {
        let at = at.into_cell_ref()?;
        // 9525 EMU per pixel at 96 DPI.
        let anchor = crabxl_core::DrawingAnchor::OneCell {
            from: crabxl_core::DrawingMarker::new(at.row(), at.column())?,
            extent: crabxl_core::DrawingExtent {
                cx: i64::from(self.width) * 9525,
                cy: i64::from(self.height) * 9525,
            },
        };
        crabxl_core::Image::new(self.data.clone(), self.format, anchor)
    }
}

fn image_header(data: &[u8]) -> Option<(crabxl_core::ImageFormat, u32, u32)> {
    use crabxl_core::ImageFormat;
    let be32 = |at: usize| {
        data.get(at..at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let le16 = |at: usize| {
        data.get(at..at + 2)
            .map(|b| u32::from(u16::from_le_bytes([b[0], b[1]])))
    };
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((ImageFormat::Png, be32(16)?, be32(20)?));
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return Some((ImageFormat::Gif, le16(6)?, le16(8)?));
    }
    if data.starts_with(&[0xFF, 0xD8]) {
        let mut at = 2;
        while at + 9 < data.len() {
            if data[at] != 0xFF {
                return None;
            }
            let marker = data[at + 1];
            let length = usize::from(u16::from_be_bytes([data[at + 2], data[at + 3]]));
            if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                let height = u32::from(u16::from_be_bytes([data[at + 5], data[at + 6]]));
                let width = u32::from(u16::from_be_bytes([data[at + 7], data[at + 8]]));
                return Some((ImageFormat::Jpeg, width, height));
            }
            at += 2 + length;
        }
    }
    None
}

/// Page orientation for printing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Orientation {
    /// Taller than wide.
    Portrait,
    /// Wider than tall.
    Landscape,
}

/// Page margins in inches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Margins {
    /// Left margin.
    pub left: f64,
    /// Right margin.
    pub right: f64,
    /// Top margin.
    pub top: f64,
    /// Bottom margin.
    pub bottom: f64,
    /// Distance from the top edge to the header.
    pub header: f64,
    /// Distance from the bottom edge to the footer.
    pub footer: f64,
}

impl Default for Margins {
    /// 0.75 inches left and right, 1 inch top and bottom, 0.5 inches for the
    /// header and footer.
    fn default() -> Self {
        Self {
            left: 0.75,
            right: 0.75,
            top: 1.0,
            bottom: 1.0,
            header: 0.5,
            footer: 0.5,
        }
    }
}

/// How a worksheet prints. `None` and `false` fields keep Excel's defaults.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageSetup {
    /// Portrait or landscape.
    pub orientation: Option<Orientation>,
    /// Excel's paper code, such as 1 for Letter or 9 for A4.
    pub paper_size: Option<u32>,
    /// Print scale in percent, from 10 to 400.
    pub scale: Option<u32>,
    /// Fit to this many pages wide and tall; 0 leaves that direction free.
    pub fit_to: Option<(u32, u32)>,
    /// Page margins.
    pub margins: Margins,
    /// Center the printout between the left and right margins.
    pub center_horizontally: bool,
    /// Center the printout between the top and bottom margins.
    pub center_vertically: bool,
    /// Print the cell grid lines.
    pub grid_lines: bool,
    /// The range to print; `None` prints the used range.
    pub print_area: Option<RangeRef>,
    /// Rows to repeat at the top of each page.
    pub title_rows: Option<Span>,
    /// Columns to repeat at the left of each page.
    pub title_columns: Option<Span>,
}

impl PageSetup {
    pub(crate) fn validate(&self) -> Result<()> {
        let invalid = |message: &str| Err(Error::new(ErrorKind::InvalidData, message));
        if self.scale.is_some_and(|scale| !(10..=400).contains(&scale)) {
            return invalid("A print scale is between 10 and 400");
        }
        if self
            .fit_to
            .is_some_and(|(wide, tall)| wide > 32767 || tall > 32767)
        {
            return invalid("Fit to at most 32767 pages");
        }
        let m = self.margins;
        if [m.left, m.right, m.top, m.bottom, m.header, m.footer]
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return invalid("Margins are finite, non-negative inches");
        }
        Ok(())
    }
}

/// The kind of a chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ChartKind {
    /// Vertical bars.
    Column,
    /// Horizontal bars.
    Bar,
    /// Lines.
    Line,
    /// Filled areas.
    Area,
    /// A pie.
    Pie,
    /// A ring.
    Doughnut,
    /// Points at x and y values. Each series needs categories: the x values.
    Scatter,
    /// Another kind, such as a 3-D, radar or stock chart. CrabXL keeps these
    /// charts but does not create them.
    Other,
}

/// One data series of a chart: references such as `"Data!$B$2:$B$9"`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Series {
    values: String,
    categories: Option<String>,
    title: Option<String>,
}

impl Series {
    /// A series of the values in a range.
    pub fn new(values: &str) -> Result<Self> {
        let values = values.trim_start_matches('=');
        if values.is_empty() {
            return Err(Error::new(ErrorKind::InvalidData, "A series needs values"));
        }
        Ok(Self {
            values: values.to_owned(),
            categories: None,
            title: None,
        })
    }
    /// Set the category labels, or the x values of a scatter chart.
    pub fn with_categories(mut self, categories: &str) -> Self {
        self.categories = Some(categories.trim_start_matches('=').to_owned());
        self
    }
    /// Set the name shown in the legend.
    pub fn with_title(mut self, title: &str) -> Self {
        self.title = Some(title.to_owned());
        self
    }
    /// The value reference.
    pub fn values(&self) -> &str {
        &self.values
    }
    /// The category reference.
    pub fn categories(&self) -> Option<&str> {
        self.categories.as_deref()
    }
    /// The name shown in the legend.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
}

/// A chart that covers a range of cells.
#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    kind: ChartKind,
    range: Option<RangeRef>,
    title: Option<String>,
    series: Vec<Series>,
    legend: bool,
}

impl Chart {
    /// An empty chart that covers `range`, such as `"E2:L18"`, with a legend.
    pub fn new(kind: ChartKind, range: impl IntoRange) -> Result<Self> {
        if kind == ChartKind::Other {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Choose a chart kind CrabXL can create",
            ));
        }
        Ok(Self {
            kind,
            range: Some(range.into_range()?),
            title: None,
            series: Vec::new(),
            legend: true,
        })
    }
    /// Add a data series.
    pub fn with_series(mut self, series: Series) -> Self {
        self.series.push(series);
        self
    }
    /// Set the chart title.
    pub fn with_title(mut self, title: &str) -> Self {
        self.title = Some(title.to_owned());
        self
    }
    /// Show or hide the legend.
    pub fn with_legend(mut self, legend: bool) -> Self {
        self.legend = legend;
        self
    }
    /// The kind of chart.
    pub fn kind(&self) -> ChartKind {
        self.kind
    }
    /// The cells the chart covers, or `None` for a chart placed by position.
    pub fn range(&self) -> Option<RangeRef> {
        self.range
    }
    /// The title.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
    /// The data series.
    pub fn series(&self) -> &[Series] {
        &self.series
    }
    /// Whether the legend shows.
    pub fn has_legend(&self) -> bool {
        self.legend
    }

    pub(crate) fn to_core(&self) -> Result<crabxl_core::drawing::Chart> {
        use crabxl_core::drawing::{ChartKind as K, XmlContent, XmlElement};
        let range = self
            .range
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "A chart needs a range"))?;
        if self.series.is_empty() {
            return Err(Error::new(ErrorKind::InvalidData, "A chart needs a series"));
        }
        let kind = match self.kind {
            ChartKind::Column | ChartKind::Bar => K::Bar,
            ChartKind::Line => K::Line,
            ChartKind::Area => K::Area,
            ChartKind::Pie => K::Pie,
            ChartKind::Doughnut => K::Doughnut,
            ChartKind::Scatter => K::Scatter,
            ChartKind::Other => {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Choose a chart kind CrabXL can create",
                ));
            }
        };
        let (start, end) = (range.start(), range.end());
        let anchor = crabxl_core::DrawingAnchor::TwoCell {
            from: crabxl_core::DrawingMarker::new(start.row(), start.column())?,
            to: crabxl_core::DrawingMarker::new(end.row() + 1, end.column() + 1)?,
            edit_as: None,
        };
        let mut chart = crabxl_core::drawing::Chart::new(kind, anchor)?;
        for series in &self.series {
            chart.add_series(
                0,
                &series.values,
                series.categories.as_deref(),
                series.title.as_deref(),
                None,
            )?;
        }
        let element = |name: &str, children: Vec<XmlElement>| XmlElement {
            name: name.into(),
            attributes: Vec::new(),
            children: children.into_iter().map(XmlContent::Element).collect(),
        };
        let value = |name: &str, value: &str| {
            let mut node = XmlElement::new(name);
            node.set_attribute("val", value);
            node
        };
        let body = chart
            .space
            .child_mut("chart")
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Chart has no body"))?;
        if self.kind == ChartKind::Bar
            && let Some(direction) = body
                .child_mut("plotArea")
                .and_then(|plot| plot.child_mut("barChart"))
                .and_then(|family| family.child_mut("barDir"))
        {
            direction.set_attribute("val", "bar");
        }
        let plot = body
            .children
            .iter()
            .position(
                |child| matches!(child, XmlContent::Element(e) if e.local_name() == "plotArea"),
            )
            .unwrap_or(0);
        if self.legend {
            body.children.insert(
                plot + 1,
                XmlContent::Element(element(
                    "c:legend",
                    vec![value("c:legendPos", "r"), value("c:overlay", "0")],
                )),
            );
        }
        // Excel names a one-series chart after its series unless told not to.
        let deleted = value(
            "c:autoTitleDeleted",
            if self.title.is_some() { "0" } else { "1" },
        );
        let mut head = Vec::new();
        if let Some(title) = &self.title {
            let mut text = XmlElement::new("a:t");
            text.children.push(XmlContent::Text(title.as_str().into()));
            head.push(element(
                "c:title",
                vec![
                    element(
                        "c:tx",
                        vec![element(
                            "c:rich",
                            vec![
                                XmlElement::new("a:bodyPr"),
                                element("a:p", vec![element("a:r", vec![text])]),
                            ],
                        )],
                    ),
                    value("c:overlay", "0"),
                ],
            ));
        }
        head.push(deleted);
        for (offset, node) in head.into_iter().enumerate() {
            body.children.insert(offset, XmlContent::Element(node));
        }
        Ok(chart)
    }

    pub(crate) fn from_core(chart: &crabxl_core::drawing::Chart) -> Self {
        use crabxl_core::drawing::{XmlContent, XmlElement};
        fn text(element: &XmlElement, out: &mut String) {
            for child in &element.children {
                match child {
                    XmlContent::Text(value) => out.push_str(value),
                    XmlContent::Element(inner) => text(inner, out),
                    _ => {}
                }
            }
        }
        fn elements(element: &XmlElement) -> impl Iterator<Item = &XmlElement> {
            element.children.iter().filter_map(|child| match child {
                XmlContent::Element(inner) => Some(inner),
                _ => None,
            })
        }
        let reference = |series: &XmlElement, names: [&str; 2]| {
            names
                .iter()
                .find_map(|name| series.child(name))
                .and_then(|node| node.child("numRef").or_else(|| node.child("strRef")))
                .and_then(|node| node.child("f"))
                .map(|formula| {
                    let mut out = String::new();
                    text(formula, &mut out);
                    out
                })
        };
        let body = chart.space.child("chart");
        let plot = body.and_then(|body| body.child("plotArea"));
        let families: Vec<&XmlElement> = plot
            .into_iter()
            .flat_map(elements)
            .filter(|e| e.local_name().ends_with("Chart"))
            .collect();
        let kind = match families.as_slice() {
            [family] => match family.local_name() {
                "barChart" => match family.child("barDir").and_then(|d| d.attribute("val")) {
                    Some("bar") => ChartKind::Bar,
                    _ => ChartKind::Column,
                },
                "lineChart" => ChartKind::Line,
                "areaChart" => ChartKind::Area,
                "pieChart" => ChartKind::Pie,
                "doughnutChart" => ChartKind::Doughnut,
                "scatterChart" => ChartKind::Scatter,
                _ => ChartKind::Other,
            },
            _ => ChartKind::Other,
        };
        let series = families
            .iter()
            .flat_map(|family| elements(family))
            .filter(|e| e.local_name() == "ser")
            .filter_map(|ser| {
                let values = reference(ser, ["val", "yVal"])?;
                let title = ser.child("tx").map(|tx| {
                    let mut out = String::new();
                    match tx.child("v") {
                        Some(v) => text(v, &mut out),
                        None => text(tx, &mut out),
                    }
                    out
                });
                Some(Series {
                    values,
                    categories: reference(ser, ["cat", "xVal"]),
                    title,
                })
            })
            .collect();
        let title = body.and_then(|body| body.child("title")).map(|title| {
            let mut out = String::new();
            if let Some(tx) = title.child("tx") {
                text(tx, &mut out);
            }
            out
        });
        let range = match &chart.anchor {
            crabxl_core::DrawingAnchor::TwoCell { from, to, .. } => {
                let start = CellRef::new(from.row, from.col).ok();
                let end = CellRef::new(
                    to.row.saturating_sub(1).max(from.row),
                    to.col.saturating_sub(1).max(from.col),
                )
                .ok();
                start.zip(end).and_then(|(a, b)| RangeRef::new(a, b).ok())
            }
            _ => None,
        };
        Self {
            kind,
            range,
            title,
            series,
            legend: body.is_some_and(|body| body.child("legend").is_some()),
        }
    }
}

/// A picture on a worksheet and the cell its top-left corner is on.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedImage {
    /// The picture.
    pub image: Image,
    /// The cell of its top-left corner, or `None` for a picture placed by
    /// position.
    pub at: Option<CellRef>,
}

impl PlacedImage {
    pub(crate) fn from_core(image: &crabxl_core::Image) -> Result<Self> {
        let at = match &image.anchor {
            crabxl_core::DrawingAnchor::OneCell { from, .. }
            | crabxl_core::DrawingAnchor::TwoCell { from, .. } => {
                CellRef::new(from.row, from.col).ok()
            }
            crabxl_core::DrawingAnchor::Absolute { .. } => None,
        };
        let mut data = Vec::new();
        image
            .data
            .write_to(&mut data)
            .map_err(|error| Error::caused_by(ErrorKind::Io, "Cannot read the picture", error))?;
        let (width, height) = match image_header(&data) {
            Some((_, width, height)) => (width, height),
            None => (0, 0),
        };
        Ok(Self {
            image: Image {
                data,
                format: image.format,
                width,
                height,
            },
            at,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn builders_validate_their_input() {
        assert!(Table::new("1Bad", "A1:B3").is_err());
        assert!(Table::new("Sales", "A1:B1").is_err());
        assert_eq!(
            Table::new("Sales", "A1:B3").unwrap().range().to_string(),
            "A1:B3"
        );
        let list = DataValidation::list(["Yes", "No"])
            .unwrap()
            .with_range("C2:C9")
            .unwrap();
        assert_eq!(list.formula1(), Some("\"Yes,No\""));
        assert!(DataValidation::list(["a,b"]).is_err());
        assert!(DataValidation::whole_number(Comparison::Between, 1, None).is_err());
        assert!(DefinedName::new("A1", "Sheet!$A$1").is_err());
        assert_eq!(
            DefinedName::new("Rate", "=Sheet!$A$1").unwrap().formula(),
            "Sheet!$A$1"
        );
        let png = [
            b"\x89PNG\r\n\x1a\n".as_slice(),
            &[0, 0, 0, 13],
            b"IHDR",
            &[0, 0, 0, 7, 0, 0, 0, 5],
        ]
        .concat();
        assert_eq!(Image::from_bytes(png).unwrap().size(), (7, 5));
        assert!(Image::from_bytes(b"nope".to_vec()).is_err());
    }
}
