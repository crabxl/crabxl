//! Sparse, runtime-independent worksheet editing with explicit allocation allowances.
use crate::cell_store::CellStore;
use crate::{
    Cell, CellAddress, CellValue, Error, ErrorKind, MAX_COLUMNS, MAX_ROWS, Result, RowIndex,
    StyleId,
};
use std::collections::BTreeMap;

// Conservative node allowance, not a claim about std's private BTreeMap layout.
const ENTRY_BYTES: usize = 256;

/// Limits for one explicitly materialized editable sheet.
#[derive(Clone, Copy, Debug)]
pub struct EditLimits {
    /// Conservative managed node/payload allowance, also checked for transient
    /// structural work. Excludes allocator overhead and caller-retained copies.
    pub max_bytes: usize,
    /// Maximum physically present cells; missing coordinates cost no nodes.
    /// The default follows the worksheet coordinate space.
    pub max_cells: usize,
}
impl Default for EditLimits {
    fn default() -> Self {
        Self {
            max_bytes: 256 * 1024 * 1024,
            max_cells: (MAX_ROWS as usize).saturating_mul(MAX_COLUMNS as usize),
        }
    }
}
/// A validated finite rectangle, including both end coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRange {
    /// Top-left corner.
    pub start: CellAddress,
    /// Bottom-right corner.
    pub end: CellAddress,
}
impl CellRange {
    /// Construct an ordered rectangle; reversed bounds are rejected.
    pub fn new(start: CellAddress, end: CellAddress) -> Result<Self> {
        if start.row > end.row || start.column > end.column {
            return Err(invalid("Reversed cell range"));
        }
        Ok(Self { start, end })
    }
    /// Whether this finite rectangle contains an address.
    pub fn contains(self, address: CellAddress) -> bool {
        (self.start.row..=self.end.row).contains(&address.row)
            && (self.start.column..=self.end.column).contains(&address.column)
    }
}
impl std::str::FromStr for CellRange {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        let (start, end) = value.split_once(':').unwrap_or((value, value));
        Self::new(start.parse()?, end.parse()?)
    }
}
impl std::fmt::Display for CellRange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.start == self.end {
            write!(formatter, "{}", self.start)
        } else {
            write!(formatter, "{}:{}", self.start, self.end)
        }
    }
}
/// Workbook catalog visibility of a worksheet or opaque sheet placeholder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SheetVisibility {
    /// Shown in the workbook tab bar.
    #[default]
    Visible,
    /// Hidden but can be shown through the spreadsheet user interface.
    Hidden,
    /// Hidden from ordinary user-interface unhide controls.
    VeryHidden,
}
impl SheetVisibility {
    /// Canonical OOXML/openpyxl spelling of this state.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Hidden => "hidden",
            Self::VeryHidden => "veryHidden",
        }
    }
}
/// Sparse owned cell model. Structural edits move coordinates and preserve
/// formulas verbatim; reference translation and feature graphs are separate.
/// This is distinct from a lazy original-file editor and opaque preservation.
pub struct Worksheet {
    name: Box<str>,
    cells: CellStore,
    limits: EditLimits,
    charged: usize,
    append_cursor: u32,
    dirty: bool,
    views: Option<Box<crate::SheetViews>>,
    printing: Option<Box<crate::PrintSettings>>,
    visibility: SheetVisibility,
}
impl Worksheet {
    /// Create an empty sheet with explicit managed-data/work allowances.
    pub fn new(name: impl Into<Box<str>>, limits: EditLimits) -> Result<Self> {
        let name = name.into();
        if name.is_empty() {
            return Err(invalid("Worksheet name is empty"));
        }
        if limits.max_bytes == 0 || limits.max_cells == 0 || name.len() > limits.max_bytes {
            return Err(budget());
        }
        Ok(Self {
            charged: name.len(),
            name,
            cells: CellStore::new(),
            limits,
            append_cursor: 0,
            dirty: false,
            views: None,
            printing: None,
            visibility: SheetVisibility::Visible,
        })
    }
    /// Sheet display name. XLSX naming rules are checked by format writers.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Current workbook catalog visibility; this flag allocates no payload.
    pub const fn visibility(&self) -> SheetVisibility {
        self.visibility
    }
    /// Set catalog visibility. Hiding all sheets is permitted in memory but
    /// saveable workbook output requires at least one visible sheet.
    pub fn set_visibility(&mut self, visibility: SheetVisibility) {
        if self.visibility != visibility {
            self.visibility = visibility;
            self.dirty = true;
        }
    }
    /// Rename a model within its retained-data allowance. Format-specific
    /// name rules remain the responsibility of codecs and adapters.
    pub fn rename(&mut self, name: impl Into<Box<str>>) -> Result<()> {
        let name = name.into();
        if name.is_empty() {
            return Err(invalid("Worksheet name is empty"));
        }
        let charged = self
            .charged
            .saturating_sub(self.name.len())
            .saturating_add(name.len());
        self.check(charged, self.len())?;
        if self.name != name {
            self.name = name;
            self.charged = charged;
            self.dirty = true;
        }
        Ok(())
    }
    pub(crate) fn edit_limits(&self) -> EditLimits {
        self.limits
    }
    pub(crate) fn set_edit_limits(&mut self, limits: EditLimits) {
        self.limits = limits;
    }
    pub(crate) fn copy_named(&self, name: Box<str>, limits: EditLimits) -> Result<Self> {
        let bytes = self
            .charged
            .saturating_sub(self.name.len())
            .saturating_add(name.len());
        if bytes > limits.max_bytes || self.len() > limits.max_cells {
            return Err(budget());
        }
        let mut copy = Self::new(name, limits)?;
        for cell in self.cells.values() {
            copy.set(cell.clone())?;
        }
        copy.set_sheet_views(self.views.as_deref().cloned())?;
        copy.set_print_settings(self.printing.as_deref().cloned())?;
        copy.append_cursor = self.append_cursor;
        copy.dirty = true;
        Ok(copy)
    }
    /// Optional explicit display settings; absent settings allocate nothing.
    pub fn sheet_views(&self) -> Option<&crate::SheetViews> {
        self.views.as_deref()
    }
    /// Replace display settings atomically within the sheet's retained allowance.
    pub fn set_sheet_views(&mut self, views: Option<crate::SheetViews>) -> Result<()> {
        if let Some(views) = &views {
            views.validate()?;
        }
        let charged = self
            .charged
            .saturating_sub(self.view_bytes())
            .saturating_add(views.as_ref().map_or(0, crate::SheetViews::memory_bytes));
        self.check(charged, self.len())?;
        self.views = views.map(Box::new);
        self.charged = charged;
        self.dirty = true;
        Ok(())
    }
    fn view_bytes(&self) -> usize {
        self.views.as_ref().map_or(0, |views| views.memory_bytes())
    }
    /// Optional explicit printing metadata.
    pub fn print_settings(&self) -> Option<&crate::PrintSettings> {
        self.printing.as_deref()
    }
    /// Atomically replace printing settings within the retained-data allowance.
    pub fn set_print_settings(&mut self, settings: Option<crate::PrintSettings>) -> Result<()> {
        if let Some(settings) = &settings {
            settings.validate()?;
        }
        let charged = self
            .charged
            .saturating_sub(self.print_bytes())
            .saturating_add(
                settings
                    .as_ref()
                    .map_or(0, crate::PrintSettings::memory_bytes),
            );
        self.check(charged, self.len())?;
        self.printing = settings.map(Box::new);
        self.charged = charged;
        self.dirty = true;
        Ok(())
    }
    /// Update one printing component without copying unrelated break vectors.
    /// An unconfigured sheet installs canonical defaults only after admission.
    pub fn update_print_settings(&mut self, change: crate::PrintSettingsChange) -> Result<()> {
        let other = self.charged.saturating_sub(self.print_bytes());
        let maximum = self.limits.max_bytes.saturating_sub(other);
        if let Some(settings) = &mut self.printing {
            settings.update(change, maximum)?;
        } else {
            let mut settings = crate::PrintSettings::default();
            settings.update(change, maximum)?;
            self.printing = Some(Box::new(settings));
        }
        self.charged = other.saturating_add(self.print_bytes());
        self.dirty = true;
        Ok(())
    }
    fn print_bytes(&self) -> usize {
        self.printing
            .as_ref()
            .map_or(0, |settings| settings.memory_bytes())
    }
    /// Number of physically present cells, including explicit Empty values.
    pub fn len(&self) -> usize {
        self.cells.len()
    }
    /// Whether no physical cells are present.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
    /// Logical row extent, including empty appends; no dense rows are allocated.
    pub fn row_extent(&self) -> u32 {
        self.append_cursor
    }
    /// Extend the logical row count without allocating cells. This records
    /// explicitly present empty source rows and advances subsequent appends.
    /// Existing cells and a greater current extent are retained.
    pub fn extend_row_extent(&mut self, rows: u32) -> Result<()> {
        if rows > MAX_ROWS {
            return Err(invalid("Logical row extent exceeds worksheet bounds"));
        }
        self.append_cursor = self.append_cursor.max(rows);
        Ok(())
    }
    /// Iterate distinct physical row indices without allocating an index table.
    pub fn row_indices(&self) -> impl Iterator<Item = RowIndex> + '_ {
        self.cells
            .values()
            .scan(None, |last, cell| {
                let index = cell.address.row;
                let present = (*last != Some(index)).then_some(index);
                *last = Some(index);
                Some(present)
            })
            .flatten()
    }
    /// Borrow the physical cells of one row in column order.
    pub fn row_cells(&self, index: RowIndex) -> impl Iterator<Item = &Cell> + Clone {
        self.cells
            .range((index.get(), 0)..=(index.get(), u32::MAX))
            .map(|(_, cell)| cell)
    }
    /// Conservative charged bytes (256 per cell plus owned value/name payload).
    pub fn charged_bytes(&self) -> usize {
        self.charged
    }
    /// Whether this model has changed since creation or explicit mark_clean.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    /// Mark this model clean after a caller-controlled successful publication.
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }
    /// Read one cell without expanding missing rows/columns.
    pub fn get(&self, address: CellAddress) -> Option<&Cell> {
        self.cells.get(&key(address))
    }
    /// Iterate physical cells in row-major order; returned references borrow self.
    pub fn cells(&self) -> impl Iterator<Item = &Cell> {
        self.cells.values()
    }
    /// Set one cell; budget failures leave the previous value unchanged.
    pub fn set(&mut self, cell: Cell) -> Result<()> {
        let bytes = self.preflight_set(&cell)?;
        self.append_cursor = self.append_cursor.max(cell.address.row.get() + 1);
        self.cells.insert(key(cell.address), cell);
        self.charged = bytes;
        self.dirty = true;
        Ok(())
    }
    pub(crate) fn preflight_set(&self, cell: &Cell) -> Result<usize> {
        let old = self.get(cell.address).map_or(0, charge);
        let bytes = self
            .charged
            .saturating_sub(old)
            .saturating_add(charge(cell));
        self.check(bytes, self.len().saturating_add(usize::from(old == 0)))?;
        Ok(bytes)
    }
    /// Remove a physical cell; existing append position is retained.
    pub fn remove(&mut self, address: CellAddress) -> Option<Cell> {
        let cell = self.cells.remove(&key(address));
        if let Some(cell) = &cell {
            self.charged -= charge(cell);
            self.dirty = true;
        }
        cell
    }
    /// Append a row after the logical extent. Empty rows advance the cursor
    /// without allocating cells. Appending is atomic on count/budget failures.
    pub fn append(&mut self, values: Vec<CellValue>) -> Result<RowIndex> {
        self.append_with_styles(values, |_| StyleId::new(0))
    }
    /// Validate an append without mutation and return its planned total charged
    /// sheet bytes. Coordinators can reserve shared operation space before
    /// calling `append`; subsequent mutation requires a fresh validation.
    pub fn preflight_append(&self, values: &[CellValue]) -> Result<usize> {
        RowIndex::new(self.append_cursor)?;
        if values.len() > MAX_COLUMNS as usize {
            return Err(invalid("Appended row exceeds column bounds"));
        }
        let bytes = values
            .iter()
            .map(|value| ENTRY_BYTES.saturating_add(value.heap_bytes()))
            .fold(self.charged, usize::saturating_add);
        self.check(bytes, self.len().saturating_add(values.len()))?;
        Ok(bytes)
    }
    pub(crate) fn append_with_styles(
        &mut self,
        values: Vec<CellValue>,
        style: impl Fn(&CellValue) -> StyleId,
    ) -> Result<RowIndex> {
        let bytes = self.preflight_append(&values)?;
        let index = RowIndex::new(self.append_cursor)?;
        for (column, value) in values.into_iter().enumerate() {
            let style = style(&value);
            let cell = Cell {
                address: CellAddress::new(index.get(), column as u32)?,
                value,
                style,
            };
            self.cells.insert(key(cell.address), cell);
        }
        self.charged = bytes;
        self.append_cursor += 1;
        self.dirty = true;
        Ok(index)
    }
    /// Insert empty rows; out-of-bounds movement is rejected before mutation.
    pub fn insert_rows(&mut self, at: RowIndex, count: u32) -> Result<()> {
        self.shift(at.get(), count, true, true)
    }
    /// Delete rows and shift subsequent physical cells upward.
    pub fn delete_rows(&mut self, at: RowIndex, count: u32) -> Result<()> {
        self.shift(at.get(), count, true, false)
    }
    /// Insert empty columns; rows/columns are never densely expanded.
    pub fn insert_columns(&mut self, at: crate::ColumnIndex, count: u32) -> Result<()> {
        self.shift(at.get(), count, false, true)
    }
    /// Delete columns and shift subsequent physical cells leftward.
    pub fn delete_columns(&mut self, at: crate::ColumnIndex, count: u32) -> Result<()> {
        self.shift(at.get(), count, false, false)
    }
    /// Move present cells by signed offsets; overlapping destinations are
    /// overwritten after all source cells are removed. Formulas stay verbatim.
    pub fn move_range(&mut self, range: CellRange, rows: i32, columns: i32) -> Result<()> {
        self.validate_range(range, rows, columns)?;
        if rows == 0 && columns == 0 {
            return Ok(());
        }
        let destination = CellRange::new(
            offset(range.start, rows, columns)?,
            offset(range.end, rows, columns)?,
        )?;
        self.work_allowance(0)?;
        let original = std::mem::take(&mut self.cells);
        let mut moved = BTreeMap::new();
        for (_, mut cell) in original {
            if range.contains(cell.address) {
                cell.address = offset(cell.address, rows, columns)?;
                moved.insert(key(cell.address), cell);
            } else if !destination.contains(cell.address) {
                self.cells.insert(key(cell.address), cell);
            }
        }
        for (key, cell) in moved {
            self.append_cursor = self.append_cursor.max(cell.address.row.get() + 1);
            self.cells.insert(key, cell);
        }
        self.recount();
        self.dirty = true;
        Ok(())
    }
    /// Move a range while translating relative references in its normal formulas.
    /// Validates every translation and retained/transient allowance before any
    /// cell moves. New formulas discard old caches; other values/styles move as-is.
    /// References outside the moved cells are not updated automatically.
    pub fn move_range_translated(
        &mut self,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.validate_range(range, rows, columns)?;
        if rows == 0 && columns == 0 {
            return Ok(());
        }
        let formulas = self
            .cells
            .values()
            .filter(|cell| {
                range.contains(cell.address) && matches!(cell.value, CellValue::Formula(_))
            })
            .count();
        let mut translated = Vec::new();
        self.work_allowance(
            formulas.saturating_mul(size_of::<(CellAddress, Box<crate::Formula>)>()),
        )?;
        translated.try_reserve_exact(formulas).map_err(|error| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate formula move staging",
                error,
            )
        })?;
        let base = self
            .charged
            .saturating_add(self.len().saturating_mul(ENTRY_BYTES))
            .saturating_add(
                translated
                    .capacity()
                    .saturating_mul(size_of::<(CellAddress, Box<crate::Formula>)>()),
            );
        let mut staged = 0usize;
        let mut retained = self.charged;
        for cell in self
            .cells
            .values()
            .filter(|cell| range.contains(cell.address))
        {
            if let CellValue::Formula(formula) = &cell.value {
                if matches!(
                    formula.formula_type(),
                    crate::FormulaType::Array | crate::FormulaType::DataTable
                ) {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Structured formula graph translation is not implemented",
                    )
                    .with_cell(cell.address));
                }
                let maximum = self
                    .limits
                    .max_bytes
                    .saturating_sub(base)
                    .saturating_sub(staged)
                    .saturating_sub(size_of::<crate::Formula>());
                let expression = crate::translate_expression(
                    formula.expression(),
                    i64::from(rows),
                    i64::from(columns),
                    maximum,
                )
                .map_err(|error| error.with_cell(cell.address))?;
                let value = crate::Formula::from_source(expression.into_boxed_str(), None, None)?;
                staged = staged.saturating_add(value.memory_bytes());
                retained = retained
                    .saturating_sub(formula.memory_bytes())
                    .saturating_add(value.memory_bytes());
                self.check(base.saturating_add(staged), self.len())?;
                self.check(retained, self.len())?;
                translated.push((offset(cell.address, rows, columns)?, Box::new(value)));
            }
        }
        self.move_range(range, rows, columns)?;
        // Each staged address belongs to a physical source cell already moved
        // by the validated operation; no further input/limit checks are needed.
        for (address, formula) in translated {
            if let Some(cell) = self.cells.get_mut(&key(address)) {
                cell.value = CellValue::Formula(formula);
            }
        }
        self.recount();
        Ok(())
    }
    /// Copy physical cells to an offset rectangle, retaining styles and formulas.
    /// Clones only selected payloads and checks transient work before mutation.
    pub fn copy_range(&mut self, range: CellRange, rows: i32, columns: i32) -> Result<()> {
        self.validate_range(range, rows, columns)?;
        if rows == 0 && columns == 0 {
            return Ok(());
        }
        let destination = CellRange::new(
            offset(range.start, rows, columns)?,
            offset(range.end, rows, columns)?,
        )?;
        let selected = self
            .cells
            .values()
            .filter(|cell| range.contains(cell.address));
        let extra = selected
            .clone()
            .map(charge)
            .fold(0usize, usize::saturating_add);
        let count = selected.clone().count();
        let removed = self
            .cells
            .values()
            .filter(|cell| destination.contains(cell.address));
        let removed_bytes = removed.clone().map(charge).sum::<usize>();
        let removed_count = removed.count();
        self.check(
            self.charged
                .saturating_sub(removed_bytes)
                .saturating_add(extra),
            self.len() - removed_count + count,
        )?;
        // Includes clone Vec and destination nodes conservatively.
        self.work_allowance(extra)?;
        let mut copies = Vec::new();
        copies.try_reserve_exact(count).map_err(|error| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate copy staging",
                error,
            )
        })?;
        for cell in selected {
            let mut cell = cell.clone();
            cell.address = offset(cell.address, rows, columns)?;
            copies.push(cell);
        }
        self.cells
            .retain(|_, cell| !destination.contains(cell.address));
        for cell in copies {
            self.append_cursor = self.append_cursor.max(cell.address.row.get() + 1);
            self.cells.insert(key(cell.address), cell);
        }
        self.recount();
        self.dirty = true;
        Ok(())
    }
    fn validate_range(&self, range: CellRange, rows: i32, columns: i32) -> Result<()> {
        CellRange::new(range.start, range.end)?;
        offset(range.start, rows, columns)?;
        offset(range.end, rows, columns)?;
        Ok(())
    }
    fn shift(&mut self, at: u32, count: u32, rows: bool, insert: bool) -> Result<()> {
        let maximum = if rows { MAX_ROWS } else { MAX_COLUMNS };
        if count == 0 || at.checked_add(count).is_none_or(|end| end > maximum) {
            return Err(invalid("Invalid structural edit count"));
        }
        if insert {
            for cell in self.cells.values() {
                let value = if rows {
                    cell.address.row.get()
                } else {
                    cell.address.column.get()
                };
                if value >= at
                    && value
                        .checked_add(count)
                        .is_none_or(|index| index >= maximum)
                {
                    return Err(invalid("Structural edit exceeds worksheet bounds"));
                }
            }
            if rows
                && self.append_cursor > at
                && self
                    .append_cursor
                    .checked_add(count)
                    .is_none_or(|end| end > MAX_ROWS)
            {
                return Err(invalid("Append extent exceeds row bounds"));
            }
        }
        self.work_allowance(0)?;
        for (_, mut cell) in std::mem::take(&mut self.cells) {
            let value = if rows {
                cell.address.row.get()
            } else {
                cell.address.column.get()
            };
            if !insert && (at..at + count).contains(&value) {
                continue;
            }
            let new = if insert && value >= at {
                value + count
            } else if !insert && value >= at + count {
                value - count
            } else {
                value
            };
            cell.address = if rows {
                CellAddress::new(new, cell.address.column.get())?
            } else {
                CellAddress::new(cell.address.row.get(), new)?
            };
            self.cells.insert(key(cell.address), cell);
        }
        if rows && self.append_cursor > at {
            self.append_cursor = if insert {
                self.append_cursor + count
            } else {
                self.append_cursor - count.min(self.append_cursor - at)
            };
        }
        self.recount();
        self.dirty = true;
        Ok(())
    }
    fn recount(&mut self) {
        self.charged = self.name.len()
            + self.view_bytes()
            + self.print_bytes()
            + self.cells.values().map(charge).sum::<usize>();
    }
    fn work_allowance(&self, extra: usize) -> Result<()> {
        self.check(
            self.charged
                .saturating_add(self.len().saturating_mul(ENTRY_BYTES))
                .saturating_add(extra),
            self.len(),
        )
    }
    fn check(&self, bytes: usize, cells: usize) -> Result<()> {
        if bytes > self.limits.max_bytes || cells > self.limits.max_cells {
            return Err(budget());
        }
        Ok(())
    }
}
fn key(address: CellAddress) -> (u32, u32) {
    (address.row.get(), address.column.get())
}
fn charge(cell: &Cell) -> usize {
    ENTRY_BYTES.saturating_add(cell.value.heap_bytes())
}
fn offset(address: CellAddress, rows: i32, columns: i32) -> Result<CellAddress> {
    let row = i64::from(address.row.get()) + i64::from(rows);
    let column = i64::from(address.column.get()) + i64::from(columns);
    let row = u32::try_from(row).map_err(|_| invalid("Negative range destination"))?;
    let column = u32::try_from(column).map_err(|_| invalid("Negative range destination"))?;
    CellAddress::new(row, column)
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
fn budget() -> Error {
    Error::new(
        ErrorKind::MemoryBudgetExceeded,
        "Editable worksheet data or work allowance exceeded",
    )
}
