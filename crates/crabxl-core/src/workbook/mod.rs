//! Owned workbook sheet bank with stable IDs and aggregate managed allowances.
mod catalog;
mod styles;

use crate::{
    Cell, CellAddress, CellRange, CellValue, ColumnIndex, DateEpoch, EditLimits, Error, ErrorKind,
    Result, RowIndex, Worksheet,
};
use std::{
    ops::Deref,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);
const SLOT_BYTES: usize = if size_of::<Entry>() > 256 {
    size_of::<Entry>()
} else {
    256
};

/// Opaque runtime sheet identity. It survives reorder/rename and cannot alias
/// removed sheets or sheets in another workbook. It is not a persisted OOXML ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SheetId {
    owner: u64,
    serial: u64,
}
impl SheetId {
    /// Workbook-local serial for diagnostics; do not reconstruct handles from it.
    pub const fn serial(self) -> u64 {
        self.serial
    }
}
/// Aggregate allowances for an explicitly owned workbook and its sheet bank.
#[derive(Clone, Copy, Debug)]
pub struct WorkbookLimits {
    /// Conservative retained model/slot bytes, also guarding structural work.
    /// Caller values and allocator overhead are additional; not a hard RSS cap.
    pub max_bytes: usize,
    /// Total physical cells across all sheets. Defaults to usize::MAX while
    /// retained/work byte allowances remain enforced.
    pub max_cells: usize,
    /// Maximum sheet count and slot capacity. Defaults to usize::MAX while
    /// byte allowances continue to bound allocated slots.
    pub max_sheets: usize,
    /// Additional per-sheet retained/work and cardinality limits.
    pub sheet: EditLimits,
}
impl Default for WorkbookLimits {
    fn default() -> Self {
        Self {
            max_bytes: 256 * 1024 * 1024,
            max_cells: usize::MAX,
            max_sheets: usize::MAX,
            sheet: EditLimits::default(),
        }
    }
}
struct Entry {
    id: SheetId,
    sheet: Worksheet,
}
/// I/O-free owned workbook. Unknown original parts belong to the separate lazy
/// XLSX editor; this model does not imply loaded-package structural preservation.
/// Cells/styles/formulas use the same shared core representation.
pub struct Workbook {
    owner: u64,
    next_serial: u64,
    entries: Vec<Entry>,
    limits: WorkbookLimits,
    active: Option<SheetId>,
    epoch: DateEpoch,
    theme: Option<crate::Theme>,
    styles: Option<crate::StyleRegistry>,
}
impl Workbook {
    /// Create an empty workbook with aggregate and per-sheet allowances.
    /// A saveable XLSX requires at least one sheet; creation is explicit here.
    pub fn new(limits: WorkbookLimits) -> Result<Self> {
        if limits.max_bytes == 0
            || limits.max_cells == 0
            || limits.max_sheets == 0
            || limits.sheet.max_bytes == 0
            || limits.sheet.max_cells == 0
        {
            return Err(budget());
        }
        // Keep the Rust 1.88-compatible name; newer compilers call it try_update.
        #[allow(deprecated)]
        let owner = NEXT_OWNER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| {
                Error::new(ErrorKind::InvalidState, "Workbook identity space exhausted")
            })?;
        Ok(Self {
            owner,
            next_serial: 1,
            entries: Vec::new(),
            limits,
            active: None,
            epoch: DateEpoch::Windows1900,
            theme: None,
            styles: None,
        })
    }
    /// Number of sheets in display order.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Whether the sheet bank is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Iterate stable IDs and borrowed models in display order.
    pub fn sheets(&self) -> impl Iterator<Item = (SheetId, &Worksheet)> {
        self.entries.iter().map(|entry| (entry.id, &entry.sheet))
    }
    /// Resolve a case-sensitive display name to a stable sheet ID.
    pub fn sheet_id(&self, name: &str) -> Option<SheetId> {
        self.entries
            .iter()
            .find(|entry| entry.sheet.name() == name)
            .map(|entry| entry.id)
    }
    /// Borrow an existing sheet by its stable identity.
    pub fn sheet(&self, id: SheetId) -> Result<&Worksheet> {
        Ok(&self.entries[self.index(id)?].sheet)
    }
    /// Obtain a mutation facade that enforces remaining aggregate allowances.
    /// Immutable methods borrow the worksheet; raw mutable replacement is not
    /// exposed, so callers cannot bypass aggregate checks with another model.
    pub fn sheet_mut(&mut self, id: SheetId) -> Result<WorksheetEditor<'_>> {
        let index = self.index(id)?;
        let other_bytes = self
            .entries
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, entry)| entry.sheet.charged_bytes())
            .sum::<usize>();
        let other_cells = self
            .entries
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, entry)| entry.sheet.len())
            .sum::<usize>();
        let remaining = self
            .limits
            .max_bytes
            .saturating_sub(self.slot_bytes())
            .saturating_sub(other_bytes)
            .saturating_sub(self.theme_bytes())
            .saturating_sub(self.style_bytes());
        let cells = self.limits.max_cells.saturating_sub(other_cells);
        let allocation_allowance = remaining.saturating_add(self.style_bytes());
        let style_ceiling = self.limits.max_bytes;
        let sheet = &mut self.entries[index].sheet;
        let original = sheet.edit_limits();
        sheet.set_edit_limits(EditLimits {
            max_bytes: original.max_bytes.min(remaining),
            max_cells: original.max_cells.min(cells),
        });
        Ok(WorksheetEditor {
            sheet,
            original,
            styles: Some(&mut self.styles),
            allocation_allowance,
            style_ceiling,
        })
    }
    /// Remaining managed space for a separately decoded incoming worksheet.
    /// Includes the existing models until replacement commits, so lazy decoding
    /// cannot silently use the same allowance twice. Source catalogs and I/O
    /// working storage must be reserved separately by the format coordinator.
    pub fn remaining_bytes(&self) -> usize {
        self.limits.max_bytes.saturating_sub(self.charged_bytes())
    }
    /// Adjust the bank's managed byte ceiling after an I/O coordinator reserves
    /// source catalogs, caches or other separately owned resources. Existing
    /// retained models must fit before the update; failure preserves the ceiling.
    /// Per-sheet and cell-count limits remain unchanged.
    pub fn set_memory_allowance(&mut self, max_bytes: usize) -> Result<()> {
        if max_bytes == 0 || self.charged_bytes() > max_bytes {
            return Err(budget());
        }
        self.limits.max_bytes = max_bytes;
        Ok(())
    }
    pub(super) fn validate_incoming(&self, sheet: &Worksheet) -> Result<()> {
        if sheet.charged_bytes() > self.limits.sheet.max_bytes
            || sheet.len() > self.limits.sheet.max_cells
        {
            return Err(budget());
        }
        if let Some(styles) = &self.styles {
            sheet.validate_style_links(Some(styles.catalog()))?;
        }
        Ok(())
    }
    /// Rename a sheet after uniqueness and aggregate budget validation.
    pub fn rename_sheet(&mut self, id: SheetId, name: impl Into<Box<str>>) -> Result<()> {
        let name = name.into();
        self.validate_name(&name, Some(id))?;
        self.sheet_mut(id)?.sheet.rename(name)
    }
    /// Remove and transfer one model. Its ID is permanently invalidated.
    /// The remaining slot allocation is retained and stays charged.
    pub fn remove_sheet(&mut self, id: SheetId) -> Result<Worksheet> {
        let index = self.index(id)?;
        let entry = self.entries.remove(index);
        if self.active == Some(id) {
            self.active = self.entries.first().map(|entry| entry.id);
        }
        Ok(entry.sheet)
    }
    /// Reorder a sheet to a zero-based display position, preserving its ID.
    pub fn move_sheet(&mut self, id: SheetId, position: usize) -> Result<()> {
        let index = self.index(id)?;
        if position >= self.entries.len() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Sheet position is out of range",
            ));
        }
        let entry = self.entries.remove(index);
        self.entries.insert(position, entry);
        Ok(())
    }
    /// Current active sheet, or None in an empty bank.
    pub const fn active_sheet(&self) -> Option<SheetId> {
        self.active
    }
    /// Select an existing sheet as active; unknown/foreign handles are rejected.
    pub fn set_active_sheet(&mut self, id: SheetId) -> Result<()> {
        if self.sheet(id)?.visibility() != crate::SheetVisibility::Visible {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Active sheet must be visible",
            ));
        }
        self.active = Some(id);
        Ok(())
    }
    /// Select a deferred display view, including relative, hidden or unselected
    /// indexes. Serialization decides its active-tab attribute independently.
    pub fn set_active_view_index(&mut self, index: i64) {
        self.active = crate::resolve_sheet_index(index, self.entries.len())
            .map(|position| self.entries[position].id);
    }
    /// Change one sheet's catalog visibility without copying its cells or changing
    /// its stable identity. All-hidden models may be assembled but cannot be saved.
    pub fn set_sheet_visibility(
        &mut self,
        id: SheetId,
        visibility: crate::SheetVisibility,
    ) -> Result<()> {
        let index = self.index(id)?;
        self.entries[index].sheet.set_visibility(visibility);
        Ok(())
    }
    /// Active sheet's zero-based display position, if present.
    pub fn active_index(&self) -> Option<usize> {
        self.active
            .and_then(|id| self.entries.iter().position(|entry| entry.id == id))
    }
    /// Calendar serialization epoch for this model.
    pub const fn epoch(&self) -> DateEpoch {
        self.epoch
    }
    /// Change the calendar serialization epoch; owned dates retain their source
    /// interpretation and the XLSX codec performs the corresponding conversion.
    pub fn set_epoch(&mut self, epoch: DateEpoch) {
        self.epoch = epoch;
    }
    /// Transfer catalogs and sheets without cloning cell or style payloads.
    /// This consumes stable handles' owner; the returned sheets retain display order.
    pub fn into_parts(self) -> WorkbookParts {
        let active_sheet = self.active_index();
        WorkbookParts {
            epoch: self.epoch,
            active_sheet,
            theme: self.theme,
            styles: self.styles,
            sheets: OwnedWorksheets {
                entries: self.entries.into_iter(),
            },
        }
    }
    /// Borrow the canonical custom theme bytes, if explicitly assigned or imported.
    pub fn theme(&self) -> Option<&crate::Theme> {
        self.theme.as_ref()
    }
    /// Replace or clear the theme under the same aggregate allowance as worksheets.
    /// Failure preserves the previous theme and all sheet identities/values.
    pub fn set_theme(&mut self, theme: Option<crate::Theme>) -> Result<()> {
        let bytes = self
            .charged_bytes()
            .saturating_sub(self.theme_bytes())
            .saturating_add(theme.as_ref().map_or(0, crate::Theme::memory_bytes));
        self.check(bytes, self.cell_count())?;
        self.theme = theme;
        Ok(())
    }
    pub(super) fn theme_bytes(&self) -> usize {
        self.theme.as_ref().map_or(0, crate::Theme::memory_bytes)
    }
    /// Total physical cells in the owned sheet bank.
    pub fn cell_count(&self) -> usize {
        self.entries.iter().map(|entry| entry.sheet.len()).sum()
    }
    /// Conservative aggregate retained bytes, including unused slot capacity.
    pub fn charged_bytes(&self) -> usize {
        self.slot_bytes().saturating_add(self.model_bytes())
    }
    pub(super) fn slot_bytes(&self) -> usize {
        self.entries.capacity().saturating_mul(SLOT_BYTES)
    }
    pub(super) fn model_bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|entry| entry.sheet.charged_bytes())
            .sum::<usize>()
            .saturating_add(self.theme_bytes())
            .saturating_add(self.style_bytes())
    }
    pub(super) fn index(&self, id: SheetId) -> Result<usize> {
        if id.owner != self.owner {
            return Err(missing());
        }
        self.entries
            .iter()
            .position(|entry| entry.id == id)
            .ok_or_else(missing)
    }
    pub(super) fn next_capacity(&self) -> Result<usize> {
        if self.entries.len() >= self.limits.max_sheets {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                "Workbook sheet count limit exceeded",
            ));
        }
        Ok(if self.entries.len() < self.entries.capacity() {
            self.entries.capacity()
        } else {
            self.entries
                .capacity()
                .saturating_mul(2)
                .max(1)
                .min(self.limits.max_sheets)
        })
    }
    pub(super) fn check(&self, bytes: usize, cells: usize) -> Result<()> {
        if bytes > self.limits.max_bytes || cells > self.limits.max_cells {
            Err(budget())
        } else {
            Ok(())
        }
    }
    pub(super) fn validate_name(&self, name: &str, except: Option<SheetId>) -> Result<()> {
        if name.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Worksheet name is empty",
            ));
        }
        let folded = name.to_lowercase();
        if self
            .entries
            .iter()
            .any(|entry| Some(entry.id) != except && entry.sheet.name().to_lowercase() == folded)
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Duplicate worksheet name",
            ));
        }
        Ok(())
    }
    pub(super) fn insert(&mut self, sheet: Worksheet, capacity: usize) -> Result<SheetId> {
        let next = self
            .next_serial
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::InvalidState, "Sheet identity space exhausted"))?;
        if capacity > self.entries.capacity() {
            self.entries
                .try_reserve_exact(capacity - self.entries.len())
                .map_err(|error| {
                    Error::caused_by(
                        ErrorKind::MemoryBudgetExceeded,
                        "Cannot allocate workbook sheet bank",
                        error,
                    )
                })?;
        }
        self.check(
            self.slot_bytes()
                .saturating_add(self.model_bytes())
                .saturating_add(sheet.charged_bytes()),
            self.cell_count().saturating_add(sheet.len()),
        )?;
        let id = SheetId {
            owner: self.owner,
            serial: self.next_serial,
        };
        self.next_serial = next;
        self.entries.push(Entry { id, sheet });
        if self.active.is_none() {
            self.active = Some(id);
        }
        Ok(id)
    }
}
fn budget() -> Error {
    Error::new(
        ErrorKind::MemoryBudgetExceeded,
        "Workbook aggregate data/work allowance exceeded",
    )
}
fn missing() -> Error {
    Error::new(
        ErrorKind::SheetNotFound,
        "Sheet identity is removed, foreign or unknown",
    )
}

/// Aggregate-budget mutation facade. Dropping restores the sheet's configured
/// personal ceiling; the workbook recomputes remaining aggregate space on each
/// new borrow. Caller-owned removed values are outside the retained budget.
pub struct WorksheetEditor<'a> {
    sheet: &'a mut Worksheet,
    original: EditLimits,
    styles: Option<&'a mut Option<crate::StyleRegistry>>,
    allocation_allowance: usize,
    style_ceiling: usize,
}
impl Deref for WorksheetEditor<'_> {
    type Target = Worksheet;
    fn deref(&self) -> &Worksheet {
        self.sheet
    }
}
impl Drop for WorksheetEditor<'_> {
    fn drop(&mut self) {
        self.sheet.set_edit_limits(self.original);
    }
}
impl Worksheet {
    /// Borrow the same checked mutation facade used by owned workbook sheets.
    /// A standalone sheet enforces its own configured limits.
    pub fn edit(&mut self) -> WorksheetEditor<'_> {
        let original = self.edit_limits();
        WorksheetEditor {
            sheet: self,
            original,
            styles: None,
            allocation_allowance: 0,
            style_ceiling: 0,
        }
    }
}
impl WorksheetEditor<'_> {
    /// Assign an existing shared format without replacing or cloning the value.
    /// Owned-bank identities are validated before any cell mutation; standalone
    /// sheets retain the caller-managed catalog contract of raw cell insertion.
    pub fn set_style(&mut self, address: crate::CellAddress, style: crate::StyleId) -> Result<()> {
        self.validate_style_id(style, address)?;
        self.sheet.set_style(address, style)
    }
    /// Assign derived appearance without overriding automatic temporal encoding.
    pub fn set_appearance_style(
        &mut self,
        address: crate::CellAddress,
        style: crate::StyleId,
    ) -> Result<()> {
        self.validate_style_id(style, address)?;
        self.sheet.set_appearance_style(address, style)
    }
    fn validate_style_id(&self, style: crate::StyleId, address: crate::CellAddress) -> Result<()> {
        if let Some(styles) = self.styles.as_deref() {
            let valid = styles.as_ref().map_or(style.get() == 0, |registry| {
                registry.catalog().cell_format(style).is_some()
            });
            if !valid {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Unknown workbook cell style identity",
                )
                .with_cell(address));
            }
        }
        Ok(())
    }
    /// Set sparse row metadata after validating workbook-local style identity.
    pub fn set_row_dimension(&mut self, dimension: crate::RowDimension) -> Result<()> {
        if let Some(style) = dimension.style {
            self.validate_style_id(style, crate::CellAddress::new(dimension.index.get(), 0)?)?;
        }
        self.sheet.set_row_dimension(dimension)
    }
    /// Set sparse column metadata after validating workbook-local style identity.
    pub fn set_column_dimension(&mut self, dimension: crate::ColumnDimension) -> Result<()> {
        if let Some(style) = dimension.style {
            self.validate_style_id(style, crate::CellAddress::new(0, dimension.start.get())?)?;
        }
        self.sheet.set_column_dimension(dimension)
    }
    /// Adopt prepared merge geometry after validating workbook-local appearances.
    pub fn set_merged_ranges(&mut self, merges: crate::MergedRanges) -> Result<()> {
        for range in merges.ranges() {
            for style in range.appearances() {
                self.validate_style_id(*style, range.range().start)?;
            }
        }
        self.sheet.set_merged_ranges(merges)
    }
    /// Remove a merge declaration under the same canonical sheet ownership.
    pub fn unmerge_cells(&mut self, range: crate::CellRange) -> Result<()> {
        self.sheet.unmerge_cells(range)
    }
    /// Adopt sparse hyperlink metadata under the current joint allowance.
    pub fn set_hyperlinks(&mut self, links: crate::Hyperlinks) -> Result<()> {
        self.sheet.set_hyperlinks(links)
    }
    /// Adopt decoded metadata without declaring a user edit.
    pub fn adopt_hyperlinks(&mut self, links: crate::Hyperlinks) -> Result<()> {
        self.sheet.adopt_hyperlinks(links)
    }
    /// Change a link and its optional empty-cell display value atomically.
    pub fn set_hyperlink(
        &mut self,
        address: CellAddress,
        link: Option<crate::Hyperlink>,
    ) -> Result<()> {
        self.sheet.set_hyperlink(address, link)
    }
    /// Change declaration fields while preserving the anchor's physical value.
    pub fn update_hyperlink(
        &mut self,
        address: CellAddress,
        link: Option<crate::Hyperlink>,
    ) -> Result<()> {
        self.sheet.update_hyperlink(address, link)
    }
    /// Apply a prepared merge after validating every local appearance reference.
    pub fn merge_prepared(
        &mut self,
        range: crate::MergedCellRange,
        anchor: crate::StyleId,
    ) -> Result<()> {
        self.validate_style_id(anchor, range.range().start)?;
        for style in range.appearances() {
            self.validate_style_id(*style, range.range().start)?;
        }
        self.sheet.apply_merge(range, anchor)
    }
    /// Replace dimension metadata only after validating every shared style link.
    pub fn set_dimensions(&mut self, dimensions: crate::SheetDimensions) -> Result<()> {
        for row in dimensions.rows() {
            if let Some(style) = row.style {
                self.validate_style_id(style, crate::CellAddress::new(row.index.get(), 0)?)?;
            }
        }
        for column in dimensions.columns() {
            if let Some(style) = column.style {
                self.validate_style_id(style, crate::CellAddress::new(0, column.start.get())?)?;
            }
        }
        self.sheet.set_dimensions(dimensions)
    }

    /// Remove an explicit row dimension without changing its cells.
    pub fn remove_row_dimension(&mut self, index: RowIndex) -> Option<crate::RowDimension> {
        self.sheet.remove_row_dimension(index)
    }
    /// Remove a column declaration by its first column.
    pub fn remove_column_dimension(
        &mut self,
        index: ColumnIndex,
    ) -> Option<crate::ColumnDimension> {
        self.sheet.remove_column_dimension(index)
    }
    /// Group sparse row metadata under the current joint workbook allowance.
    pub fn group_rows(
        &mut self,
        start: RowIndex,
        end: RowIndex,
        level: u32,
        hidden: bool,
    ) -> Result<()> {
        self.sheet.group_rows(start, end, level, hidden)
    }
    /// Group sparse column metadata under the current joint workbook allowance.
    pub fn group_columns(
        &mut self,
        start: ColumnIndex,
        end: ColumnIndex,
        level: u32,
        hidden: bool,
    ) -> Result<()> {
        self.sheet.group_columns(start, end, level, hidden)
    }
    /// Replace canonical printing metadata under aggregate/per-sheet limits.
    pub fn set_print_settings(&mut self, settings: Option<crate::PrintSettings>) -> Result<()> {
        self.sheet.set_print_settings(settings)
    }

    /// Update one printing component under the joint workbook allowance.
    pub fn update_print_settings(&mut self, change: crate::PrintSettingsChange) -> Result<()> {
        self.sheet.update_print_settings(change)
    }

    /// Replace canonical display metadata within the aggregate/per-sheet allowance.
    pub fn set_sheet_views(&mut self, views: Option<crate::SheetViews>) -> Result<()> {
        self.sheet.set_sheet_views(views)
    }

    /// Insert or replace a shared-model cell within aggregate/per-sheet limits.
    pub fn set(&mut self, mut cell: Cell) -> Result<()> {
        let bytes = self.sheet.preflight_set(&cell)?;
        if let Some(value) = cell.value.temporal_value() {
            cell.style = self.prepare_temporal(cell.style, value.kind(), bytes)?;
        }
        self.sheet.set(cell)
    }
    /// Append one row, prevalidating cell allowances and preparing shared temporal
    /// styles before committing cells. Valid interned styles may remain after failure.
    pub fn append(&mut self, values: Vec<CellValue>) -> Result<RowIndex> {
        let bytes = self.sheet.preflight_append(&values)?;
        let mut styles = [crate::StyleId::new(0); 4];
        let mut prepared = [false; 4];
        for value in &values {
            if let Some(value) = value.temporal_value() {
                let index = temporal_index(value.kind());
                if !prepared[index] {
                    styles[index] =
                        self.prepare_temporal(crate::StyleId::new(0), value.kind(), bytes)?;
                    prepared[index] = true;
                }
            }
        }
        self.sheet.append_with_styles(values, |value| {
            value
                .temporal_value()
                .map_or(crate::StyleId::new(0), |value| {
                    styles[temporal_index(value.kind())]
                })
        })
    }
    fn prepare_temporal(
        &mut self,
        base: crate::StyleId,
        kind: crate::DateKind,
        bytes: usize,
    ) -> Result<crate::StyleId> {
        let Some(bank) = self.styles.as_mut() else {
            return Ok(base);
        };
        let maximum = self.allocation_allowance.saturating_sub(bytes);
        let result = if let Some(registry) = bank.as_mut() {
            registry.register_temporal_format_with_limit(base, kind, maximum)
        } else {
            let mut registry = crate::StyleRegistry::new(crate::StyleLimits {
                max_bytes: maximum,
                ..Default::default()
            })?;
            registry.register_temporal_presets_with_limit(maximum)?;
            let id = registry.register_temporal_format_with_limit(base, kind, maximum)?;
            registry.set_limits(crate::StyleLimits {
                max_bytes: self.style_ceiling,
                ..Default::default()
            })?;
            **bank = Some(registry);
            Ok(id)
        };
        let style_bytes = bank.as_ref().map_or(0, crate::StyleRegistry::memory_bytes);
        let mut limits = self.sheet.edit_limits();
        limits.max_bytes = self
            .original
            .max_bytes
            .min(self.allocation_allowance.saturating_sub(style_bytes));
        self.sheet.set_edit_limits(limits);
        result
    }
    /// Remove and transfer one physical cell.
    pub fn remove(&mut self, address: CellAddress) -> Result<Option<Cell>> {
        self.sheet.remove(address)
    }
    /// Insert rows under the combined work allowance.
    pub fn insert_rows(&mut self, index: RowIndex, count: u32) -> Result<()> {
        self.sheet.insert_rows(index, count)
    }
    /// Delete rows under the combined work allowance.
    pub fn delete_rows(&mut self, index: RowIndex, count: u32) -> Result<()> {
        self.sheet.delete_rows(index, count)
    }
    /// Insert columns under the combined work allowance.
    pub fn insert_columns(&mut self, index: ColumnIndex, count: u32) -> Result<()> {
        self.sheet.insert_columns(index, count)
    }
    /// Delete columns under the combined work allowance.
    pub fn delete_columns(&mut self, index: ColumnIndex, count: u32) -> Result<()> {
        self.sheet.delete_columns(index, count)
    }
    /// Move a finite rectangle without formula translation.
    pub fn move_range(&mut self, range: CellRange, rows: i32, columns: i32) -> Result<()> {
        self.sheet.move_range(range, rows, columns)
    }
    /// Move a finite rectangle with prevalidated normal-formula translation.
    pub fn move_range_translated(
        &mut self,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.sheet.move_range_translated(range, rows, columns)
    }
    /// Copy a finite rectangle under retained and transient aggregate allowances.
    pub fn copy_range(&mut self, range: CellRange, rows: i32, columns: i32) -> Result<()> {
        self.sheet.copy_range(range, rows, columns)
    }
}

fn temporal_index(kind: crate::DateKind) -> usize {
    match kind {
        crate::DateKind::Date => 0,
        crate::DateKind::DateTime => 1,
        crate::DateKind::Time => 2,
        crate::DateKind::Duration => 3,
    }
}

/// Ownership transfer for exporting a canonical workbook without catalog snapshots.
pub struct WorkbookParts {
    /// Numeric date epoch shared by all sheets.
    pub epoch: DateEpoch,
    /// Active display index, absent for an empty workbook.
    pub active_sheet: Option<usize>,
    /// Shared immutable custom theme, if present.
    pub theme: Option<crate::Theme>,
    /// Canonical editable registry with source identities and existing indices.
    pub styles: Option<crate::StyleRegistry>,
    /// Sheets consumed in display order without a second collection allocation.
    pub sheets: OwnedWorksheets,
}
/// Consuming sheet iterator backed by the workbook's original entry allocation.
pub struct OwnedWorksheets {
    entries: std::vec::IntoIter<Entry>,
}
impl Iterator for OwnedWorksheets {
    type Item = Worksheet;
    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next().map(|entry| entry.sheet)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.entries.size_hint()
    }
}
impl ExactSizeIterator for OwnedWorksheets {}
impl std::iter::FusedIterator for OwnedWorksheets {}

fn merge_border_side(target: &mut Option<crate::BorderSide>, incoming: &Option<crate::BorderSide>) {
    let Some(incoming) = incoming else { return };
    if let Some(target) = target {
        if target
            .line
            .is_none_or(|line| line == crate::BorderLine::None)
        {
            target.line = incoming.line;
        }
        if target.color.is_none() {
            target.color = incoming.color.clone();
        }
    } else {
        *target = Some(incoming.clone());
    }
}
