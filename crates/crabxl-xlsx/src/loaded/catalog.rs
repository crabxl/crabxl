//! Catalog operations for the canonical LoadedWorkbook owner.
use super::*;

impl<R: Read + Seek> LoadedWorkbook<R> {
    /// Create an empty worksheet in the same source-backed canonical bank.
    /// A bounded catalog/relationship/content-type transaction commits after
    /// source policies and aggregate model/metadata allowances are checked.
    pub fn create_sheet(&mut self, name: impl Into<Box<str>>) -> Result<SheetId> {
        self.add_sheet(name.into(), None)
    }
    /// Copy supported cell content and source worksheet properties into a new
    /// budgeted canonical model. Affected unmodeled graphs reject before mutation.
    pub fn copy_sheet(&mut self, id: SheetId, name: impl Into<Box<str>>) -> Result<SheetId> {
        self.add_sheet(name.into(), Some(id))
    }
    pub(super) fn add_sheet(&mut self, name: Box<str>, source: Option<SheetId>) -> Result<SheetId> {
        if self.options.read.data_only {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Data-only sheet creation remains unimplemented",
            ));
        }
        crate::encode::validate_catalog_name(&name)?;
        let template = if let Some(id) = source {
            let maximum = self
                .allowance
                .retained_data_bytes
                .min(self.options.workbook.max_bytes);
            self.editor
                .guard_copy_context(maximum.saturating_sub(self.managed_retained_bytes()))?;
            let source = self
                .sheets
                .iter()
                .find(|entry| entry.id == id)
                .ok_or_else(|| {
                    Error::new(ErrorKind::SheetNotFound, "Unknown copied sheet identity")
                })?;
            let template = source.original.or_else(|| self.editor.copy_template(id));
            if let Some(index) = template {
                self.editor.prepare_copy_template(index)?;
            }
            self.sheet(id)?;
            crate::loaded_codec::validate_model(self.bank.sheet(id)?, self.bank.style_catalog())?;
            template
        } else {
            None
        };
        let originals = self.original_identities()?;
        let maximum = self
            .allowance
            .retained_data_bytes
            .min(self.options.workbook.max_bytes);
        let allowance = maximum.saturating_sub(self.managed_retained_bytes());
        let plan = if source.is_some() {
            self.editor.prepare_copy(originals, allowance, template)?
        } else {
            self.editor.prepare_create(originals, allowance)?
        };
        self.sheets.try_reserve_exact(1).map_err(|cause| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate created sheet handle",
                cause,
            )
        })?;
        self.reserve_workbook_patch(plan.bytes.saturating_add(plan.scratch_bytes))?;
        let view = self.active_view_index();
        let incoming = if let Some(id) = source {
            self.bank.copy_sheet(id, name)
        } else {
            self.bank.create_sheet(name)
        };
        let id = match incoming {
            Ok(id) => id,
            Err(error) => {
                self.rebalance()?;
                return Err(error);
            }
        };
        self.sheets.push(SourceSheet {
            id,
            name: "".into(),
            loaded: true,
            kind: crate::SheetKind::Worksheet,
            original: None,
        });
        self.editor.commit_create(plan, id);
        self.bank.set_active_view_index(view);
        self.rebalance()?;
        Ok(id)
    }
    /// Remove a supported worksheet and return its detached canonical model.
    /// Package graph ownership is validated before loading or modifying cells.
    /// Source parts stay available as immutable templates for existing copies.
    pub fn remove_sheet(&mut self, id: SheetId) -> Result<Worksheet> {
        if self.options.read.data_only {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Data-only sheet removal remains unimplemented",
            ));
        }
        let index = self
            .sheets
            .iter()
            .position(|source| source.id == id)
            .ok_or_else(|| {
                Error::new(ErrorKind::SheetNotFound, "Unknown removed sheet identity")
            })?;
        let original = self.sheets[index].original;
        let maximum = self
            .allowance
            .retained_data_bytes
            .min(self.options.workbook.max_bytes);
        if let Some(original) = original {
            self.editor.check_remove_graph(
                original,
                maximum.saturating_sub(self.managed_retained_bytes()),
            )?;
        }
        self.sheet(id)?;
        let originals = self.original_identities()?;
        let plan = self.editor.prepare_remove(
            id,
            originals,
            original,
            maximum.saturating_sub(self.managed_retained_bytes()),
        )?;
        self.reserve_workbook_patch(plan.bytes.saturating_add(plan.scratch_bytes))?;
        let view = self.active_view_index();
        let detached = match self.bank.remove_sheet(id) {
            Ok(sheet) => sheet,
            Err(error) => {
                self.rebalance()?;
                return Err(error);
            }
        };
        self.sheets.remove(index);
        self.bank.set_active_view_index(view);
        self.editor.commit_remove(plan, view);
        self.rebalance()?;
        Ok(detached)
    }
    pub(super) fn original_identities(&self) -> Result<Vec<SheetId>> {
        let mut originals = Vec::new();
        if self.editor.membership_is_dirty() {
            return Ok(originals);
        }
        originals
            .try_reserve_exact(self.sheets.len())
            .map_err(|cause| {
                Error::caused_by(
                    ErrorKind::MemoryBudgetExceeded,
                    "Cannot allocate original catalog handles",
                    cause,
                )
            })?;
        originals.extend(
            self.sheets
                .iter()
                .filter(|source| source.original.is_some())
                .map(|source| source.id),
        );
        Ok(originals)
    }
    /// Rename a stable source-backed identity without decoding cells or changing
    /// its original part. Catalog/model changes share one preflight allowance.
    /// Existing formula and defined-name expressions are not rewritten.
    pub fn rename_sheet(&mut self, id: SheetId, name: impl Into<Box<str>>) -> Result<()> {
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        let name = name.into();
        crate::encode::validate_catalog_name(&name)?;
        if self.bank.sheet(id)?.name() == name.as_ref() {
            return Ok(());
        }
        if self.editor.membership_is_dirty() {
            let planned = self.editor.prepare_membership_metadata()?;
            self.reserve_workbook_patch(planned)?;
            if let Err(error) = self.bank.rename_sheet(id, name) {
                self.rebalance()?;
                return Err(error);
            }
            self.editor
                .commit_active_view(self.active_view_index(), planned);
            return self.rebalance();
        }
        let planned = self.editor.prepare_name(index, &name)?;
        self.reserve_workbook_patch(planned)?;
        let patch_name = name.clone();
        if let Err(error) = self.bank.rename_sheet(id, name) {
            self.rebalance()?;
            return Err(error);
        }
        self.editor.commit_name(index, patch_name, planned);
        self.rebalance()
    }
    /// Reorder a source sheet to a zero-based display position without decoding
    /// cells. Retain the active display index, matching public reference behavior.
    /// Affected local defined-name/catalog graphs reject before mutation.
    pub fn move_sheet(&mut self, id: SheetId, position: usize) -> Result<()> {
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        let current = self
            .bank
            .sheets()
            .position(|(sheet, _)| sheet == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if current == position {
            return Ok(());
        }
        if self.editor.membership_is_dirty() {
            let planned = self.editor.prepare_membership_metadata()?;
            self.reserve_workbook_patch(planned)?;
            let view = self.active_view_index();
            if let Err(error) = self.bank.move_sheet(id, position) {
                self.rebalance()?;
                return Err(error);
            }
            self.bank.set_active_view_index(view);
            self.editor.commit_active_view(view, planned);
            return self.rebalance();
        }
        self.rebalance()?;
        let maximum = self
            .allowance
            .retained_data_bytes
            .min(self.options.workbook.max_bytes);
        let plan = self.editor.prepare_order(
            index,
            position,
            maximum.saturating_sub(self.managed_retained_bytes()),
        )?;
        self.reserve_workbook_patch(plan.bytes.saturating_add(plan.scratch_bytes))?;
        let view = plan.view_index;
        self.bank.move_sheet(id, position)?;
        self.bank.set_active_view_index(view);
        self.editor.commit_order(plan);
        self.rebalance()
    }
    /// Select a visible original sheet by stable identity without materializing
    /// its cells. Signed/unsupported metadata and resource failures reject before
    /// changing the bank or its pending original-package view.
    pub fn set_active_sheet(&mut self, id: SheetId) -> Result<()> {
        let index = self
            .bank
            .sheets()
            .position(|(sheet, _)| sheet == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if self.editor.membership_is_dirty() {
            let planned = self.editor.prepare_membership_metadata()?;
            self.reserve_workbook_patch(planned)?;
            if let Err(error) = self.bank.set_active_sheet(id) {
                self.rebalance()?;
                return Err(error);
            }
            self.editor.commit_active_view(index as i64, planned);
            return self.rebalance();
        }
        let planned = self.editor.prepare_active(index)?;
        self.reserve_workbook_patch(planned)?;
        self.bank.set_active_sheet(id)?;
        self.editor.commit_active(index, planned);
        self.rebalance()
    }
    /// Select a deferred display view without loading worksheet cells.
    pub fn set_active_view_index(&mut self, index: i64) -> Result<()> {
        let planned = if self.editor.membership_is_dirty() {
            self.editor.prepare_membership_metadata()?
        } else {
            self.editor.prepare_active_view(index)?
        };
        self.reserve_workbook_patch(planned)?;
        self.bank.set_active_view_index(index);
        self.editor.commit_active_view(index, planned);
        self.rebalance()
    }
    /// Current signed view, including an unselected or relative request.
    pub fn active_view_index(&self) -> i64 {
        self.editor.active_view_index()
    }
    /// Change original catalog visibility without materializing worksheet cells.
    /// All-hidden intermediate states are allowed; saving requires a visible sheet.
    /// Validation and joint allowance checks precede model and overlay changes.
    pub fn set_sheet_visibility(
        &mut self,
        id: SheetId,
        visibility: crabxl_core::SheetVisibility,
    ) -> Result<()> {
        if self.editor.membership_is_dirty() {
            let planned = self.editor.prepare_membership_metadata()?;
            self.reserve_workbook_patch(planned)?;
            let view = self.active_view_index();
            if let Err(error) = self.bank.set_sheet_visibility(id, visibility) {
                self.rebalance()?;
                return Err(error);
            }
            self.bank.set_active_view_index(view);
            self.editor.commit_active_view(view, planned);
            return self.rebalance();
        }
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        let planned = self.editor.prepare_visibility(index)?;
        self.reserve_workbook_patch(planned)?;
        self.bank.set_sheet_visibility(id, visibility)?;
        self.editor.commit_visibility(index, visibility, planned);
        self.rebalance()
    }
    /// Atomically update visibility and a deferred view for a UI holding both
    /// controls. All validation/resource reservation precedes either change.
    pub fn set_sheet_visibility_and_active_view(
        &mut self,
        id: SheetId,
        visibility: crabxl_core::SheetVisibility,
        view_index: i64,
    ) -> Result<()> {
        if self.editor.membership_is_dirty() {
            let planned = self.editor.prepare_membership_metadata()?;
            self.reserve_workbook_patch(planned)?;
            if let Err(error) = self.bank.set_sheet_visibility(id, visibility) {
                self.rebalance()?;
                return Err(error);
            }
            self.bank.set_active_view_index(view_index);
            self.editor.commit_active_view(view_index, planned);
            return self.rebalance();
        }
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        let planned = self.editor.prepare_visibility(index)?;
        self.reserve_workbook_patch(planned)?;
        self.bank.set_sheet_visibility(id, visibility)?;
        self.bank.set_active_view_index(view_index);
        self.editor.commit_visibility(index, visibility, planned);
        self.editor.commit_active_view(view_index, planned);
        self.rebalance()
    }
    pub(super) fn reserve_workbook_patch(&mut self, planned: usize) -> Result<()> {
        let package = self
            .package_extra_bytes()
            .saturating_sub(self.editor.patch_bytes())
            .saturating_add(planned);
        let retained = self
            .mapping_bytes()
            .saturating_add(package)
            .saturating_add(self.bank.charged_bytes());
        let maximum = self
            .allowance
            .retained_data_bytes
            .min(self.options.workbook.max_bytes);
        self.editor
            .book
            .rebalance_strings_for_retained(retained, maximum)?;
        let available = maximum
            .checked_sub(
                self.mapping_bytes()
                    .saturating_add(package)
                    .saturating_add(self.editor.book.retained_source_bytes()),
            )
            .ok_or_else(budget)?;
        self.bank.set_memory_allowance(available)
    }
}
