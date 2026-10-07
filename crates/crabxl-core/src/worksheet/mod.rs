//! Sparse, runtime-independent worksheet editing with explicit allocation allowances.
mod dimensions;
mod hyperlinks;
mod merges;
mod metadata;
mod structure;

use crate::ColumnIndex;
use crate::cell_store::CellStore;
use crate::{
    Cell, CellAddress, CellValue, Error, ErrorKind, MAX_COLUMNS, MAX_ROWS, Result, RowIndex,
    StyleId,
};
use std::collections::BTreeMap;

// Conservative structural staging allowance; retained blocks are charged
// separately. This is not a claim about std's private BTreeMap layout.
const ENTRY_BYTES: usize = 256;
const STRUCTURAL_ROOT_BYTES: usize = 1024;

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
    /// Whether two inclusive finite rectangles overlap.
    pub fn intersects(self, other: Self) -> bool {
        self.start.row <= other.end.row
            && self.end.row >= other.start.row
            && self.start.column <= other.end.column
            && self.end.column >= other.start.column
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
    dimensions: crate::SheetDimensions,
    merges: crate::MergedRanges,
    hyperlinks: crate::Hyperlinks,
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
            dimensions: Default::default(),
            merges: Default::default(),
            hyperlinks: Default::default(),
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
    /// Adjust a prospective retained-data allowance without changing cell limits.
    pub fn set_memory_allowance(&mut self, max_bytes: usize) -> Result<()> {
        if max_bytes == 0 || self.charged > max_bytes {
            return Err(budget());
        }
        self.limits.max_bytes = max_bytes;
        Ok(())
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
        copy.set_dimensions(self.dimensions.clone())?;
        copy.set_merged_ranges(self.merges.clone())?;
        copy.set_hyperlinks(self.hyperlinks.clone())?;
        copy.append_cursor = self.append_cursor;
        copy.dirty = true;
        Ok(copy)
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
    /// Visible row extent including compact merged geometry, distinct from append position.
    pub fn display_row_extent(&self) -> u32 {
        self.merges
            .ranges()
            .iter()
            .map(|range| range.range().end.row.get() + 1)
            .max()
            .unwrap_or(0)
            .max(self.append_cursor)
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
    /// Borrow physical row cells starting at a selected column.
    pub fn row_cells_from(
        &self,
        index: RowIndex,
        column: ColumnIndex,
    ) -> impl Iterator<Item = &Cell> + Clone {
        self.cells
            .range((index.get(), column.get())..=(index.get(), u32::MAX))
            .map(|(_, cell)| cell)
    }
    /// Conservative retained block capacity, tree allowances and value/name payload.
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
    /// Replace only a cell's workbook-local style identity without cloning its
    /// value or invoking automatic temporal formatting. Missing cells become
    /// physically present empty styled cells under the ordinary allocation cap.
    /// Standalone callers validate identities against their writer's catalog.
    pub fn set_style(&mut self, address: CellAddress, style: StyleId) -> Result<()> {
        if let Some(cell) = self.cells.get_mut(&key(address)) {
            self.dirty |= cell.set_style(style);
            return Ok(());
        }
        self.set(Cell {
            address,
            value: CellValue::Empty,
            style,
        })
    }
    /// Replace appearance components while retaining temporal encoding preferences.
    pub fn set_appearance_style(&mut self, address: CellAddress, style: StyleId) -> Result<()> {
        if let Some(cell) = self.cells.get_mut(&key(address)) {
            self.dirty |= cell.set_appearance_style(style);
            return Ok(());
        }
        self.set(Cell {
            address,
            value: CellValue::Empty,
            style,
        })
    }
    /// Iterate physical cells in row-major order; returned references borrow self.
    pub fn cells(&self) -> impl Iterator<Item = &Cell> {
        self.cells.values()
    }
    /// Set one cell; budget failures leave the previous value unchanged.
    pub fn set(&mut self, cell: Cell) -> Result<()> {
        let (bytes, _) = self.plan_set(&cell)?;
        let storage = self.cells.storage_bytes();
        self.append_cursor = self.append_cursor.max(cell.address.row.get() + 1);
        self.cells.insert(key(cell.address), cell);
        self.charged = bytes
            .saturating_sub(storage)
            .saturating_add(self.cells.storage_bytes());
        self.dirty = true;
        Ok(())
    }
    pub(crate) fn preflight_set(&self, cell: &Cell) -> Result<usize> {
        self.plan_set(cell).map(|(_, peak)| peak)
    }
    #[inline(always)]
    pub(super) fn plan_set(&self, cell: &Cell) -> Result<(usize, usize)> {
        if !matches!(cell.value, CellValue::Empty)
            && self.merges.virtual_style(cell.address).is_some()
        {
            return Err(Error::new(
                ErrorKind::InvalidState,
                "Merged non-anchor values are read-only",
            ));
        }
        let previous = self.get(cell.address);
        let old = previous.map_or(0, charge);
        let present = previous.is_some();
        let (growth, work) = self.cells.insertion_growth(key(cell.address));
        let bytes = self
            .charged
            .saturating_sub(old)
            .saturating_add(charge(cell));
        let count = self.len().saturating_add(usize::from(!present));
        self.check(bytes.saturating_add(growth), count)?;
        let peak = bytes.saturating_add(work);
        self.check(peak, count)?;
        Ok((bytes, peak))
    }
    /// Remove a physical cell; existing append position is retained.
    pub fn remove(&mut self, address: CellAddress) -> Option<Cell> {
        let old_links = self.hyperlinks.heap_bytes();
        if self.hyperlinks.remove(address).is_some() {
            self.charged = self
                .charged
                .saturating_sub(old_links)
                .saturating_add(self.hyperlinks.heap_bytes());
            self.dirty = true;
        }
        let storage = self.cells.storage_bytes();
        let cell = self.cells.remove(
            &key(address),
            self.limits.max_bytes.saturating_sub(self.charged),
        );
        if let Some(cell) = &cell {
            self.charged = self
                .charged
                .saturating_sub(charge(cell))
                .saturating_sub(storage)
                .saturating_add(self.cells.storage_bytes());
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
        let row = RowIndex::new(self.row_extent())?;
        for column in 0..values.len() {
            let address = CellAddress::new(row.get(), column as u32)?;
            if self.merges.virtual_style(address).is_some() {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Appending over covered merged coordinates is not implemented",
                )
                .with_cell(address));
            }
        }

        RowIndex::new(self.row_extent())?;
        if values.len() > MAX_COLUMNS as usize {
            return Err(invalid("Appended row exceeds column bounds"));
        }
        let bytes = values
            .iter()
            .map(CellValue::heap_bytes)
            .fold(self.charged, usize::saturating_add);
        let (growth, work) = self.cells.append_growth(values.len());
        let count = self.len().saturating_add(values.len());
        self.check(bytes.saturating_add(work), count)?;
        Ok(bytes.saturating_add(growth))
    }
    pub(crate) fn append_with_styles(
        &mut self,
        values: Vec<CellValue>,
        style: impl Fn(&CellValue) -> StyleId,
    ) -> Result<RowIndex> {
        self.preflight_append(&values)?;
        let payload = values
            .iter()
            .map(CellValue::heap_bytes)
            .fold(0usize, usize::saturating_add);
        let storage = self.cells.storage_bytes();
        let index = RowIndex::new(self.row_extent())?;
        for (column, value) in values.into_iter().enumerate() {
            let style = style(&value);
            let cell = Cell {
                address: CellAddress::new(index.get(), column as u32)?,
                value,
                style,
            };
            self.cells.insert(key(cell.address), cell);
        }
        self.charged = self
            .charged
            .saturating_sub(storage)
            .saturating_add(self.cells.storage_bytes())
            .saturating_add(payload);
        self.append_cursor = index.get() + 1;
        self.dirty = true;
        Ok(index)
    }
    pub(super) fn recount(&mut self) {
        self.charged = self.name.len()
            + self.view_bytes()
            + self.print_bytes()
            + self.dimensions.heap_bytes()
            + self.merges.heap_bytes()
            + self.hyperlinks.heap_bytes()
            + self.cells.storage_bytes()
            + self.cells.values().map(charge).sum::<usize>();
    }
    pub(super) fn guard_merged_structure(
        &self,
        source: CellRange,
        destination: CellRange,
    ) -> Result<()> {
        if self.hyperlinks.intersects(source) || self.hyperlinks.intersects(destination) {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Structural editing of affected hyperlinks remains unimplemented",
            ));
        }
        if self
            .merges
            .ranges()
            .iter()
            .any(|merge| source.intersects(merge.range()) || destination.intersects(merge.range()))
        {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Affected merged geometry structural edits are not implemented",
            ));
        }
        Ok(())
    }
    pub(super) fn work_allowance(&self, extra: usize) -> Result<()> {
        self.check(
            self.charged
                .saturating_add(self.cell_work_bytes())
                .saturating_add(extra),
            self.len(),
        )
    }
    pub(super) fn cell_work_bytes(&self) -> usize {
        self.len()
            .saturating_mul(ENTRY_BYTES)
            .saturating_add(if self.is_empty() {
                0
            } else {
                STRUCTURAL_ROOT_BYTES
            })
    }
    pub(super) fn check(&self, bytes: usize, cells: usize) -> Result<()> {
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
    cell.value.heap_bytes()
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
