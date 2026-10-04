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
    /// Total physical cells across all sheets.
    pub max_cells: usize,
    /// Maximum sheet count and slot capacity.
    pub max_sheets: usize,
    /// Additional per-sheet retained/work and cardinality limits.
    pub sheet: EditLimits,
}
impl Default for WorkbookLimits {
    fn default() -> Self {
        Self {
            max_bytes: 256 * 1024 * 1024,
            max_cells: 10_000_000,
            max_sheets: 1024,
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
            .saturating_sub(other_bytes);
        let cells = self.limits.max_cells.saturating_sub(other_cells);
        let sheet = &mut self.entries[index].sheet;
        let original = sheet.edit_limits();
        sheet.set_edit_limits(EditLimits {
            max_bytes: original.max_bytes.min(remaining),
            max_cells: original.max_cells.min(cells),
        });
        Ok(WorksheetEditor { sheet, original })
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
        self.index(id)?;
        self.active = Some(id);
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
            .sum()
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
        }
    }
}
impl WorksheetEditor<'_> {
    /// Insert or replace a shared-model cell within aggregate/per-sheet limits.
    pub fn set(&mut self, cell: Cell) -> Result<()> {
        self.sheet.set(cell)
    }
    /// Append one row, atomically validating aggregate/per-sheet allowances.
    pub fn append(&mut self, values: Vec<CellValue>) -> Result<RowIndex> {
        self.sheet.append(values)
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
