//! Owned workbook sheet bank with stable IDs and aggregate managed allowances.
use crate::{
    Cell, CellAddress, CellRange, CellValue, ColumnIndex, DateEpoch, EditLimits, Error, ErrorKind,
    Result, RowIndex, Worksheet,
};
use std::{
    ops::Deref,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);
const SLOT_BYTES: usize = 256;

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
    /// Create a uniquely named empty sheet at the end of display order.
    /// Names are case-insensitively unique; format naming rules apply on output.
    pub fn create_sheet(&mut self, name: impl Into<Box<str>>) -> Result<SheetId> {
        let name = name.into();
        self.validate_name(&name, None)?;
        let capacity = self.next_capacity()?;
        self.check(
            capacity
                .saturating_mul(SLOT_BYTES)
                .saturating_add(self.model_bytes())
                .saturating_add(name.len()),
            self.cell_count(),
        )?;
        let remaining = self
            .limits
            .max_bytes
            .saturating_sub(capacity.saturating_mul(SLOT_BYTES))
            .saturating_sub(self.model_bytes());
        let mut sheet = Worksheet::new(
            name,
            EditLimits {
                max_bytes: self.limits.sheet.max_bytes.min(remaining),
                max_cells: self.limits.sheet.max_cells,
            },
        )?;
        sheet.set_edit_limits(self.limits.sheet);
        self.insert(sheet, capacity)
    }
    /// Copy all owned scalar cells/styles/formulas and logical append extent.
    /// This does not copy original-package drawings or other feature graphs.
    pub fn copy_sheet(&mut self, id: SheetId, name: impl Into<Box<str>>) -> Result<SheetId> {
        let name = name.into();
        self.validate_name(&name, None)?;
        let source = self.index(id)?;
        let capacity = self.next_capacity()?;
        let bytes = self.entries[source]
            .sheet
            .charged_bytes()
            .saturating_sub(self.entries[source].sheet.name().len())
            .saturating_add(name.len());
        let cells = self
            .cell_count()
            .saturating_add(self.entries[source].sheet.len());
        self.check(
            capacity
                .saturating_mul(SLOT_BYTES)
                .saturating_add(self.model_bytes())
                .saturating_add(bytes),
            cells,
        )?;
        let remaining = self
            .limits
            .max_bytes
            .saturating_sub(capacity.saturating_mul(SLOT_BYTES))
            .saturating_sub(self.model_bytes());
        let mut sheet = self.entries[source].sheet.copy_named(
            name,
            EditLimits {
                max_bytes: self.limits.sheet.max_bytes.min(remaining),
                max_cells: self.limits.sheet.max_cells,
            },
        )?;
        sheet.set_edit_limits(self.limits.sheet);
        self.insert(sheet, capacity)
    }
    /// Transfer an already decoded worksheet into this bank without cloning cells.
    /// Validates aggregate/per-sheet limits, naming and any initialized style
    /// catalog before allocating a stable identity. On error this bank is unchanged;
    /// the caller-owned incoming model is dropped.
    pub fn adopt_sheet(&mut self, mut sheet: Worksheet) -> Result<SheetId> {
        self.validate_name(sheet.name(), None)?;
        self.validate_incoming(&sheet)?;
        let capacity = self.next_capacity()?;
        self.check(
            capacity
                .saturating_mul(SLOT_BYTES)
                .saturating_add(self.model_bytes())
                .saturating_add(sheet.charged_bytes()),
            self.cell_count().saturating_add(sheet.len()),
        )?;
        sheet.set_edit_limits(self.limits.sheet);
        self.insert(sheet, capacity)
    }
    /// Replace a registered model by ownership transfer while retaining its ID.
    /// Useful for atomic lazy loading: decode into a separately bounded model,
    /// then commit only after all source and aggregate checks have passed.
    /// The incoming model must have the same name. The former model is returned
    /// to the caller and no longer participates in this bank's retained allowance.
    pub fn replace_sheet(&mut self, id: SheetId, mut sheet: Worksheet) -> Result<Worksheet> {
        let index = self.index(id)?;
        let old = &self.entries[index].sheet;
        if old.name() != sheet.name() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Replacement worksheet must retain its registered name",
            ));
        }
        self.validate_incoming(&sheet)?;
        self.check(
            self.charged_bytes()
                .saturating_sub(old.charged_bytes())
                .saturating_add(sheet.charged_bytes()),
            self.cell_count()
                .saturating_sub(old.len())
                .saturating_add(sheet.len()),
        )?;
        sheet.set_edit_limits(self.limits.sheet);
        Ok(std::mem::replace(&mut self.entries[index].sheet, sheet))
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
    fn validate_incoming(&self, sheet: &Worksheet) -> Result<()> {
        if sheet.charged_bytes() > self.limits.sheet.max_bytes
            || sheet.len() > self.limits.sheet.max_cells
        {
            return Err(budget());
        }
        if let Some(styles) = &self.styles {
            for row in sheet.row_indices() {
                for cell in sheet.row_cells(row) {
                    styles.catalog().cell_style(cell.style)?;
                }
            }
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
    /// Borrow canonical workbook-local style identities, if explicitly initialized.
    pub fn style_catalog(&self) -> Option<&crate::StyleCatalog> {
        self.styles.as_ref().map(crate::StyleRegistry::catalog)
    }
    /// Adopt source style identities before registering styles. Existing cells must
    /// refer to the imported table. Failure preserves this workbook's model state.
    pub fn import_style_catalog(
        &mut self,
        catalog: crate::StyleCatalog,
        mut limits: crate::StyleLimits,
    ) -> Result<()> {
        if self.styles.is_some() {
            return Err(Error::new(
                ErrorKind::InvalidState,
                "Workbook styles already initialized",
            ));
        }
        for (_, sheet) in self.sheets() {
            for row in sheet.row_indices() {
                for cell in sheet.row_cells(row) {
                    catalog.cell_style(cell.style)?;
                }
            }
        }
        let requested = limits;
        limits.max_bytes = limits
            .max_bytes
            .min(self.limits.max_bytes.saturating_sub(self.charged_bytes()));
        let mut styles = crate::StyleRegistry::from_catalog(catalog, limits)?;
        styles.set_limits(requested)?;
        self.styles = Some(styles);
        Ok(())
    }
    /// Register appearance using the same aggregate allowance as all worksheets.
    /// An initial default registry is created lazily; existing source IDs are retained.
    pub fn register_style(&mut self, style: crate::CellStyle) -> Result<crate::StyleId> {
        let maximum = self.style_allowance();
        if let Some(styles) = &mut self.styles {
            return styles.register_with_limit(style, maximum);
        }
        let mut styles = crate::StyleRegistry::new(crate::StyleLimits {
            max_bytes: maximum,
            ..Default::default()
        })?;
        let id = styles.register(style)?;
        styles.set_limits(crate::StyleLimits {
            max_bytes: self.limits.max_bytes,
            ..Default::default()
        })?;
        self.styles = Some(styles);
        Ok(id)
    }
    /// Register a raw format referencing this bank's imported/shared components.
    pub fn register_format(&mut self, format: crate::CellFormat) -> Result<crate::StyleId> {
        let maximum = self.style_allowance();
        self.styles
            .as_mut()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidState,
                    "Workbook styles are not initialized",
                )
            })?
            .register_format_with_limit(format, maximum)
    }
    /// Intern a literal number-format code without rebuilding component payloads.
    pub fn register_number_format(&mut self, code: Box<str>) -> Result<u32> {
        let maximum = self.style_allowance();
        self.styles
            .as_mut()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidState,
                    "Workbook styles are not initialized",
                )
            })?
            .register_number_format_with_limit(code, maximum)
    }
    fn style_allowance(&self) -> usize {
        self.limits
            .max_bytes
            .saturating_sub(self.charged_bytes().saturating_sub(self.style_bytes()))
    }
    fn style_bytes(&self) -> usize {
        self.styles
            .as_ref()
            .map_or(0, crate::StyleRegistry::memory_bytes)
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
    fn theme_bytes(&self) -> usize {
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
    fn slot_bytes(&self) -> usize {
        self.entries.capacity().saturating_mul(SLOT_BYTES)
    }
    fn model_bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|entry| entry.sheet.charged_bytes())
            .sum::<usize>()
            .saturating_add(self.theme_bytes())
            .saturating_add(self.style_bytes())
    }
    fn index(&self, id: SheetId) -> Result<usize> {
        if id.owner != self.owner {
            return Err(missing());
        }
        self.entries
            .iter()
            .position(|entry| entry.id == id)
            .ok_or_else(missing)
    }
    fn next_capacity(&self) -> Result<usize> {
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
    fn check(&self, bytes: usize, cells: usize) -> Result<()> {
        if bytes > self.limits.max_bytes || cells > self.limits.max_cells {
            Err(budget())
        } else {
            Ok(())
        }
    }
    fn validate_name(&self, name: &str, except: Option<SheetId>) -> Result<()> {
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
    fn insert(&mut self, sheet: Worksheet, capacity: usize) -> Result<SheetId> {
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
        self.sheet.set_style(address, style)
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
    pub fn remove(&mut self, address: CellAddress) -> Option<Cell> {
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
