//! Catalog operations.
use super::*;

impl Workbook {
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
}
