//! Lazy materialization into the canonical owned bank, with joint source accounting.
use crate::{EditorOptions, SaveOptions, SaveStats, SharedStringOptions, WorkbookEditor};
use crabxl_core::{
    Cell, CellAddress, CellRange, CellValue, ColumnIndex, DateEpoch, EditLimits, Error, ErrorKind,
    MemoryAllowance, MemoryPolicy, ReadOptions, ResourceLimits, Result, Row, RowIndex, SheetId,
    StyleLimits, Workbook, WorkbookLimits, Worksheet, WorksheetEditor,
};
use std::{
    fs::File,
    io::{Read, Seek, Write},
    path::Path,
};

/// Managed policy for a source-backed canonical workbook bank.
#[derive(Clone, Debug, Default)]
pub struct LoadOptions {
    /// Archive, XML, value, metadata and materialized-model limits.
    pub resources: ResourceLimits,
    /// Joint managed source/model allowance, excluding the I/O working reserve.
    pub memory_policy: MemoryPolicy,
    /// RAM/disk/cache/temp settings for the source shared-string table.
    pub shared_strings: SharedStringOptions,
    /// Limits across all registered sheet models.
    pub workbook: WorkbookLimits,
    /// Full-sheet value semantics. Coordinate projections are rejected.
    pub read: ReadOptions,
    /// Overlay and output policies; resources/memory policy use the enclosing settings.
    pub editor: EditorOptions,
}
struct SourceSheet {
    id: SheetId,
    name: Box<str>,
    loaded: bool,
    kind: crate::SheetKind,
    original: Option<usize>,
}
/// Owns a seekable original package and the canonical workbook bank. Source
/// styles transfer into the bank without cloning; only date classifications stay
/// in the reader. Sheets decode lazily into independently bounded temporary
/// models before a stable-ID commit. This is explicit editable-model loading,
/// not a streaming row mode or a whole-process RSS cap.
///
/// Original source assets remain on the seekable source. Scalar/formula overlays
/// share this bank and preserve unrelated parts across repeat saves. Complete
/// typed-date/style mutation and structural feature edits remain staged.
pub struct LoadedWorkbook<R: Read + Seek = File> {
    editor: WorkbookEditor<R>,
    bank: Workbook,
    sheets: Vec<SourceSheet>,
    options: LoadOptions,
    allowance: MemoryAllowance,
}
impl LoadedWorkbook<File> {
    /// Open a path with default joint model/source allowances.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::with_options(
            File::open(path).map_err(|error| {
                Error::caused_by(ErrorKind::Io, "Cannot open loaded workbook", error)
            })?,
            LoadOptions::default(),
        )
    }
}
impl<R: Read + Seek> LoadedWorkbook<R> {
    /// Own a source with explicit managed policy and source/model limits.
    /// No worksheet cells are decoded during construction.
    pub fn with_options(source: R, mut options: LoadOptions) -> Result<Self> {
        if options.read.rows.is_some() || options.read.columns.is_some() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Loaded models require full-sheet coordinates",
            ));
        }
        let allowance = crate::memory_allowance(options.memory_policy, options.resources)?;
        options.resources.max_metadata_bytes = options
            .resources
            .max_metadata_bytes
            .min(allowance.retained_data_bytes as u64);
        let mut edit_options = options.editor.clone();
        edit_options.resources = options.resources;
        edit_options.memory_policy = options.memory_policy;
        let mut editor = WorkbookEditor::with_options(source, edit_options)?;
        let original_package_bytes = editor.retained_package_bytes();
        let reader = &mut editor.book;
        reader.set_shared_string_options(options.shared_strings.clone());
        let mut limits = options.workbook;
        limits.max_bytes = limits.max_bytes.min(allowance.retained_data_bytes);
        limits.sheet.max_bytes = limits
            .sheet
            .max_bytes
            .min(options.resources.max_materialized_bytes);
        limits.max_sheets = limits.max_sheets.min(options.resources.max_sheets);
        let count = reader.sheets().len();
        let mapping = size_of::<Self>()
            .saturating_add(count.saturating_mul(256))
            .saturating_add(
                reader
                    .sheets()
                    .iter()
                    .map(|sheet| sheet.name().len())
                    .sum::<usize>(),
            );
        let source_fixed = original_package_bytes.saturating_add(mapping);
        limits.max_bytes = limits
            .max_bytes
            .checked_sub(source_fixed)
            .ok_or_else(budget)?;
        let mut bank = Workbook::new(limits)?;
        bank.set_epoch(if reader.date_1904() {
            DateEpoch::Mac1904
        } else {
            DateEpoch::Windows1900
        });
        if let Some(catalog) = reader.transfer_style_catalog(bank.remaining_bytes())? {
            bank.import_style_catalog(
                catalog,
                StyleLimits {
                    max_bytes: options.resources.max_style_bytes,
                    max_records: options.resources.max_style_records,
                },
            )?;
        }
        let mut sheets = Vec::new();
        sheets.try_reserve_exact(count).map_err(|error| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate loaded sheet handles",
                error,
            )
        })?;
        for (index, sheet) in reader.sheets().iter().enumerate() {
            let id = bank.create_sheet(sheet.name())?;
            sheets.push(SourceSheet {
                id,
                name: sheet.name().into(),
                loaded: false,
                kind: sheet.kind(),
                original: Some(index),
            });
        }
        bank.set_active_view_index(reader.active_view_index());
        // Original declarations can select a hidden sheet; preserve their read
        // state while explicit future selection still requires a visible target.
        for (source, info) in sheets.iter().zip(reader.sheets()) {
            bank.set_sheet_visibility(source.id, info.visibility())?;
        }
        let mut value = Self {
            editor,
            bank,
            sheets,
            options,
            allowance,
        };
        value.rebalance()?;
        Ok(value)
    }
    /// Borrow the canonical bank; not-yet-loaded source sheets are placeholders.
    /// Use `sheet` to obtain a decoded model rather than reading placeholders.
    pub fn model(&self) -> &Workbook {
        &self.bank
    }
    /// Resolve a stable original worksheet identity without decoding cells.
    pub fn sheet_id(&self, name: &str) -> Option<SheetId> {
        self.bank.sheet_id(name)
    }
    /// Original worksheet/chartsheet/dialog kind. Unimplemented non-cell sheet
    /// models remain opaque source-backed catalog entries rather than preventing
    /// access to unrelated worksheets or dropping their parts on save.
    pub fn sheet_kind(&self, id: SheetId) -> Option<crate::SheetKind> {
        self.sheets
            .iter()
            .find(|sheet| sheet.id == id)
            .map(|sheet| sheet.kind)
    }
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
    fn add_sheet(&mut self, name: Box<str>, source: Option<SheetId>) -> Result<SheetId> {
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
    fn original_identities(&self) -> Result<Vec<SheetId>> {
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
    fn reserve_workbook_patch(&mut self, planned: usize) -> Result<()> {
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
    /// Whether a source worksheet has committed its decoded model.
    pub fn is_materialized(&self, id: SheetId) -> bool {
        self.sheets
            .iter()
            .any(|sheet| sheet.id == id && sheet.loaded)
    }
    /// Resolved joint allowance and explicit I/O working reserve.
    pub fn memory_allowance(&self) -> &MemoryAllowance {
        &self.allowance
    }
    /// Current jointly accounted retained source, styles, handles and models.
    pub fn managed_retained_bytes(&self) -> usize {
        self.bank
            .charged_bytes()
            .saturating_add(self.source_bytes())
    }
    /// Actual source string placement, cache, lookup and temporary-byte counters.
    /// None means the shared-string table is absent or not yet prepared.
    pub fn shared_string_stats(&self) -> Option<crate::SharedStringStats> {
        self.editor.book.shared_string_stats()
    }
    fn mapping_bytes(&self) -> usize {
        size_of::<Self>()
            .saturating_add(
                self.options
                    .shared_strings
                    .temp_directory
                    .as_ref()
                    .map_or(0, |path| path.capacity()),
            )
            .saturating_add(self.sheets.capacity().saturating_mul(256))
            .saturating_add(
                self.sheets
                    .iter()
                    .map(|sheet| sheet.name.len())
                    .sum::<usize>(),
            )
    }
    fn source_bytes(&self) -> usize {
        self.mapping_bytes()
            .saturating_add(self.editor.retained_package_bytes())
    }
    fn package_extra_bytes(&self) -> usize {
        self.editor
            .retained_package_bytes()
            .saturating_sub(self.editor.book.retained_source_bytes())
    }
    fn rebalance(&mut self) -> Result<()> {
        let retained = self
            .mapping_bytes()
            .saturating_add(self.bank.charged_bytes())
            .saturating_add(self.package_extra_bytes());
        let maximum = self
            .allowance
            .retained_data_bytes
            .min(self.options.workbook.max_bytes);
        self.editor
            .book
            .rebalance_strings_for_retained(retained, maximum)?;
        let available = maximum
            .checked_sub(self.source_bytes())
            .ok_or_else(budget)?;
        self.bank.set_memory_allowance(available)
    }
    /// Borrow a pending source overlay without decoding the original worksheet.
    pub fn pending_value(&self, id: SheetId, address: CellAddress) -> Option<&CellValue> {
        let source = self
            .sheets
            .iter()
            .find(|sheet| sheet.id == id && sheet.original.is_some())?;
        self.editor.pending_value(&source.name, address)
    }
    /// Borrow pending source cells in sparse order, without materialization.
    /// Source appearance is retained by the preserving editor on save.
    pub fn pending_cells(&self, id: SheetId) -> impl Iterator<Item = &Cell> {
        self.sheets
            .iter()
            .filter(move |sheet| sheet.id == id && sheet.original.is_some())
            .flat_map(|sheet| self.editor.pending_cells(&sheet.name))
    }
    /// Current retained overlay bytes inside the joint source/model cap.
    pub fn patch_bytes(&self) -> usize {
        self.editor.patch_bytes()
    }
    /// Replace a physical source cell, preserving its source appearance/metadata.
    /// Target existence and affected source graph guards are checked during save.
    pub fn set_value(&mut self, id: SheetId, address: CellAddress, value: CellValue) -> Result<()> {
        self.edit_value(id, address, value, false)
    }
    /// Set a source cell or insert a missing scalar/formula cell without eagerly
    /// materializing its worksheet. Source overlays and cached models share the
    /// same managed cap. This scalar checkpoint retains the editor's explicit
    /// typed-date and affected-graph restrictions.
    pub fn upsert_value(
        &mut self,
        id: SheetId,
        address: CellAddress,
        value: CellValue,
    ) -> Result<()> {
        self.edit_value(id, address, value, true)
    }
    /// Derive a source cell's number format while retaining its other style fields.
    /// Unknown style extensions and signed packages reject before registration.
    /// Successfully interned formats remain reusable if a later cell edit fails.
    pub fn set_number_format(
        &mut self,
        id: SheetId,
        address: CellAddress,
        code: Box<str>,
    ) -> Result<()> {
        if self.options.read.data_only {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Data-only style editing remains unimplemented",
            ));
        }
        crate::encode::validate_xml_text(&code)?;
        let catalog = self.bank.style_catalog().ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                "Adding a missing source stylesheet remains unimplemented",
            )
        })?;
        self.editor.validate_style_edit(catalog)?;
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if self.sheets[index].original.is_some()
            && !self.editor.model_is_dirty(&self.sheets[index].name)
        {
            self.editor.prepare_model(&self.sheets[index].name)?;
        }
        let style = self
            .sheet(id)?
            .get(address)
            .map_or(crabxl_core::StyleId::new(0), |cell| cell.style);
        self.rebalance()?;
        let result = self.bank.derive_number_format(style, code);
        if result.is_ok() {
            self.editor.styles_changed();
        }
        self.rebalance()?;
        self.set_style(id, address, result?)
    }
    /// Assign an existing workbook-local format without copying the cell value.
    /// Supported source worksheets materialize once and use the canonical save
    /// path. Affected unmodeled graphs reject before mutation. Unknown format
    /// identities reject before materialization; a missing coordinate becomes an
    /// empty styled cell under the same aggregate allowance.
    pub fn set_style(
        &mut self,
        id: SheetId,
        address: CellAddress,
        style: crabxl_core::StyleId,
    ) -> Result<()> {
        let valid = self
            .bank
            .style_catalog()
            .map_or(style.get() == 0, |catalog| {
                catalog.cell_format(style).is_some()
            });
        if !valid {
            return Err(
                Error::new(ErrorKind::InvalidData, "Unknown workbook style identity")
                    .with_cell(address),
            );
        }
        if self.options.read.data_only {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Data-only style editing remains unimplemented",
            ));
        }
        let source = self
            .sheets
            .iter()
            .find(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if source.original.is_none() || self.editor.model_is_dirty(&source.name) {
            let created = source.original.is_none();
            let result = self.bank.sheet_mut(id)?.set_style(address, style);
            if result.is_ok() && created {
                self.editor.created_values_dirty(id);
            }
            self.rebalance()?;
            return result;
        }
        self.edit_structure_when(
            id,
            |sheet| {
                let changed = sheet.get(address).is_none_or(|cell| cell.style != style || matches!(&cell.value, CellValue::DateTime(date) if !date.requires_serial_encoding()));
                sheet.set_style(address, style)?;
                Ok(changed)
            },
            |changed| *changed,
        )?;
        Ok(())
    }
    /// Remove one physical cell from a supported source-backed model and
    /// transfer its owned value/style to the caller. Logical append extent stays.
    /// Affected unmodeled graphs reject before any cell or package mutation.
    pub fn remove_cell(&mut self, id: SheetId, address: CellAddress) -> Result<Option<Cell>> {
        self.edit_structure_when(id, |sheet| Ok(sheet.remove(address)), Option::is_some)
    }
    /// Insert rows in a supported source-backed cell model. Unmodeled affected
    /// worksheet graphs are rejected before mutation; formulas are not translated.
    pub fn insert_rows(&mut self, id: SheetId, at: RowIndex, count: u32) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.insert_rows(at, count))
    }
    /// Delete rows while retaining unrelated original package parts.
    pub fn delete_rows(&mut self, id: SheetId, at: RowIndex, count: u32) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.delete_rows(at, count))
    }
    /// Insert columns without cloning the whole canonical worksheet.
    pub fn insert_columns(&mut self, id: SheetId, at: ColumnIndex, count: u32) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.insert_columns(at, count))
    }
    /// Delete columns using the same guarded source/model coordinator.
    pub fn delete_columns(&mut self, id: SheetId, at: ColumnIndex, count: u32) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.delete_columns(at, count))
    }
    /// Move a source-backed rectangle; reference expressions remain unchanged.
    pub fn move_range(
        &mut self,
        id: SheetId,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.move_range(range, rows, columns))
    }
    /// Move and translate relative references inside moved normal formulas.
    pub fn move_range_translated(
        &mut self,
        id: SheetId,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.edit_structure(id, |sheet| {
            sheet.move_range_translated(range, rows, columns)
        })
    }
    /// Copy an actual rectangle under aggregate cell/payload allowances.
    pub fn copy_range(
        &mut self,
        id: SheetId,
        range: CellRange,
        rows: i32,
        columns: i32,
    ) -> Result<()> {
        self.edit_structure(id, |sheet| sheet.copy_range(range, rows, columns))
    }
    fn edit_structure(
        &mut self,
        id: SheetId,
        edit: impl FnOnce(&mut WorksheetEditor<'_>) -> Result<()>,
    ) -> Result<()> {
        self.edit_structure_when(id, edit, |_| true)
    }
    fn edit_structure_when<T>(
        &mut self,
        id: SheetId,
        edit: impl FnOnce(&mut WorksheetEditor<'_>) -> Result<T>,
        changed: impl FnOnce(&T) -> bool,
    ) -> Result<T> {
        if self.options.read.data_only {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Data-only structural editing remains unimplemented",
            ));
        }
        let index = self
            .sheets
            .iter()
            .position(|source| source.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if self.sheets[index].original.is_none() {
            crate::loaded_codec::validate_model(self.bank.sheet(id)?, self.bank.style_catalog())?;
            let result = edit(&mut self.bank.sheet_mut(id)?);
            if let Ok(value) = &result
                && changed(value)
            {
                self.editor.created_values_dirty(id);
            }
            self.rebalance()?;
            return result;
        }
        let plan = self.editor.prepare_model(&self.sheets[index].name)?;
        self.sheet(id)?;
        crate::loaded_codec::validate_model(self.bank.sheet(id)?, self.bank.style_catalog())?;
        self.reserve_workbook_patch(plan.bytes.max(self.editor.patch_bytes()))?;
        let result = edit(&mut self.bank.sheet_mut(id)?);
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                self.rebalance()?;
                return Err(error);
            }
        };
        if changed(&value) {
            self.editor.commit_model(plan, id);
        }
        self.rebalance()?;
        Ok(value)
    }
    /// Append a complete scalar/formula row after actual source/pending extent.
    /// The selected sheet materializes once; advertised dimensions do not choose
    /// the append position. Validate all values and joint model/overlay/scratch
    /// allowances before committing either representation. Date and unresolved
    /// phonetic-font assignments retain their explicit unsupported errors.
    pub fn append(&mut self, id: SheetId, values: Vec<CellValue>) -> Result<RowIndex> {
        self.sheet(id)?;
        let source = self
            .sheets
            .iter()
            .find(|source| source.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if source.original.is_none() || self.editor.model_is_dirty(&source.name) {
            let created = source.original.is_none();
            let row = RowIndex::new(self.bank.sheet(id)?.row_extent())?;
            for (column, value) in values.iter().enumerate() {
                let address = CellAddress::new(row.get(), column as u32)?;
                if created {
                    self.editor.validate_created_value(address, value)?;
                } else {
                    self.editor.prepare_value(&source.name, address, value)?;
                }
            }
            let result = self.bank.sheet_mut(id)?.append(values);
            if created && result.is_ok() {
                self.editor.created_values_dirty(id);
            }
            self.rebalance()?;
            return result;
        }
        let sheet = self.bank.sheet(id)?;
        let row = RowIndex::new(sheet.row_extent())?;
        let increase = sheet
            .preflight_append(&values)?
            .saturating_sub(sheet.charged_bytes());
        let source = self
            .sheets
            .iter()
            .find(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        let maximum = self
            .allowance
            .retained_data_bytes
            .min(self.options.workbook.max_bytes);
        let planning_bytes = values
            .len()
            .saturating_mul(std::mem::size_of::<crate::editor::PatchPlan>());
        let planning_retained = self
            .mapping_bytes()
            .saturating_add(self.package_extra_bytes())
            .saturating_add(self.bank.charged_bytes())
            .saturating_add(planning_bytes);
        self.editor
            .book
            .rebalance_strings_for_retained(planning_retained, maximum)?;
        let scratch_allowance = maximum.saturating_sub(self.managed_retained_bytes());
        let plan = self
            .editor
            .prepare_row(&source.name, row, &values, scratch_allowance)?;
        let package = self
            .package_extra_bytes()
            .saturating_sub(self.editor.patch_bytes())
            .saturating_add(plan.bytes);
        let scratch = plan.scratch_bytes.saturating_add(
            values
                .len()
                .saturating_mul(std::mem::size_of::<CellValue>()),
        );
        let retained = self
            .mapping_bytes()
            .saturating_add(package)
            .saturating_add(self.bank.charged_bytes())
            .saturating_add(increase)
            .saturating_add(scratch);
        self.editor
            .book
            .rebalance_strings_for_retained(retained, maximum)?;
        let available = maximum
            .checked_sub(
                self.mapping_bytes()
                    .saturating_add(package)
                    .saturating_add(self.editor.book.retained_source_bytes())
                    .saturating_add(scratch),
            )
            .ok_or_else(budget)?;
        self.bank.set_memory_allowance(available)?;
        if self.bank.remaining_bytes() < increase {
            self.rebalance()?;
            return Err(budget());
        }
        let result = self.bank.sheet_mut(id)?.append(values.clone());
        if let Err(error) = result {
            self.rebalance()?;
            return Err(error);
        }
        self.editor.commit_row(plan, values);
        self.rebalance()?;
        Ok(row)
    }
    fn edit_value(
        &mut self,
        id: SheetId,
        address: CellAddress,
        value: CellValue,
        insert_missing: bool,
    ) -> Result<()> {
        let source = self
            .sheets
            .iter()
            .find(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if source.original.is_none() {
            self.editor.validate_created_value(address, &value)?;
            if !insert_missing && self.bank.sheet(id)?.get(address).is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Replacement targets a missing model cell",
                )
                .with_cell(address));
            }
            let maximum = self
                .allowance
                .retained_data_bytes
                .min(self.options.workbook.max_bytes);
            let retained = self
                .mapping_bytes()
                .saturating_add(self.package_extra_bytes())
                .saturating_add(self.bank.charged_bytes())
                .saturating_add(value.heap_bytes());
            self.editor
                .book
                .rebalance_strings_for_retained(retained, maximum)?;
            self.rebalance()?;
            let style = self
                .bank
                .sheet(id)?
                .get(address)
                .map_or(crabxl_core::StyleId::new(0), |cell| cell.style);
            let result = self.bank.sheet_mut(id)?.set(Cell {
                address,
                value,
                style,
            });
            if result.is_ok() {
                self.editor.created_values_dirty(id);
            }
            self.rebalance()?;
            return result;
        }
        let plan = self.editor.prepare_value(&source.name, address, &value)?;
        let loaded = source.loaded;
        let model_dirty = self.editor.model_is_dirty(&source.name);
        if model_dirty && !insert_missing && self.bank.sheet(id)?.get(address).is_none() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Replacement targets a missing model cell",
            )
            .with_cell(address));
        }
        let maximum = self
            .allowance
            .retained_data_bytes
            .min(self.options.workbook.max_bytes);
        let package = self
            .package_extra_bytes()
            .saturating_sub(self.editor.patch_bytes())
            .saturating_add(plan.bytes);
        let retained = self
            .mapping_bytes()
            .saturating_add(package)
            .saturating_add(self.bank.charged_bytes())
            .saturating_add(if loaded { value.heap_bytes() } else { 0 });
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
        self.bank.set_memory_allowance(available)?;
        if loaded {
            if self.bank.remaining_bytes() < value.heap_bytes() {
                self.rebalance()?;
                return Err(budget());
            }
            let style = self
                .bank
                .sheet(id)?
                .get(address)
                .map_or(crabxl_core::StyleId::new(0), |cell| cell.style);
            let result = self.bank.sheet_mut(id)?.set(Cell {
                address,
                value: value.clone(),
                style,
            });
            if let Err(error) = result {
                self.rebalance()?;
                return Err(error);
            }
        }
        if !model_dirty {
            self.editor.commit_value(plan, value, insert_missing);
        }
        self.rebalance()
    }
    /// Lazily decode a full supported worksheet. Failed parsing or allowance
    /// checks discard the incoming model and preserve the registered placeholder.
    /// Source SST preparation/cache adjustments may remain, within the joint cap.
    pub fn sheet(&mut self, id: SheetId) -> Result<&Worksheet> {
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if self.sheets[index].kind != crate::SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Typed chartsheet/dialog model access remains unimplemented",
            ));
        }
        if !self.sheets[index].loaded {
            self.rebalance()?;
            let maximum = self
                .allowance
                .retained_data_bytes
                .min(self.options.workbook.max_bytes);
            // Auto can release its current RAM table by spilling; disk caches
            // can shrink. Do not freeze the incoming model's ceiling to the
            // current cache/table placement before the row pool can rebalance.
            let strict_strings =
                if self.options.shared_strings.storage == crate::SharedStringStorage::Memory {
                    self.editor
                        .book
                        .shared_string_stats()
                        .map_or(0, |stats| stats.managed_bytes)
                } else {
                    0
                };
            let fixed = self
                .bank
                .charged_bytes()
                .saturating_add(self.mapping_bytes())
                .saturating_add(self.editor.book.catalog_memory_bytes())
                .saturating_add(self.package_extra_bytes())
                .saturating_add(strict_strings);
            let temporary_limit = maximum
                .saturating_sub(fixed)
                .min(self.options.resources.max_materialized_bytes)
                .min(self.options.workbook.sheet.max_bytes);
            let name = self.sheets[index].name.as_ref();
            let mut incoming = Worksheet::new(
                self.bank.sheet(id)?.name(),
                EditLimits {
                    max_bytes: temporary_limit,
                    max_cells: self.options.workbook.sheet.max_cells.min(
                        self.options
                            .workbook
                            .max_cells
                            .saturating_sub(self.bank.cell_count()),
                    ),
                },
            )?;
            let retained = self
                .bank
                .charged_bytes()
                .saturating_add(self.mapping_bytes())
                .saturating_add(self.package_extra_bytes());
            let decode = (|| {
                let mut rows = self.editor.book.rows_with_catalog_allowance(
                    name,
                    self.options.read.clone(),
                    Some(maximum),
                    self.bank.style_catalog(),
                    retained.saturating_add(incoming.charged_bytes()),
                    true,
                )?;
                let mut row = Row::new(RowIndex::new(0)?);
                rows.set_aggregate_retained(retained.saturating_add(incoming.charged_bytes()))?;
                while rows.read_row_into(&mut row)? {
                    incoming.extend_row_extent(row.index.get() + 1)?;
                    rows.set_aggregate_retained(
                        retained
                            .saturating_add(incoming.charged_bytes())
                            .saturating_add(row.memory_bytes()),
                    )?;
                    for cell in row.cells.drain(..) {
                        incoming.set(cell)?;
                    }
                    rows.set_aggregate_retained(
                        retained
                            .saturating_add(incoming.charged_bytes())
                            .saturating_add(row.memory_bytes()),
                    )?;
                }
                Ok(())
            })();
            if let Err(error) = decode {
                drop(incoming);
                self.rebalance()?;
                return Err(error);
            }
            if let Err(error) =
                self.editor
                    .apply_pending_model(name, &mut incoming, retained, maximum)
            {
                drop(incoming);
                self.rebalance()?;
                return Err(error);
            }
            incoming.set_visibility(self.bank.sheet(id)?.visibility());
            incoming.mark_clean();
            self.rebalance()?;
            self.bank.replace_sheet(id, incoming)?;
            self.sheets[index].loaded = true;
            self.rebalance()?;
        }
        self.bank.sheet(id)
    }
    /// Save the original package without consuming source assets or loaded models.
    /// Materialization alone does not mark original parts dirty: untouched rich/
    /// unknown content stays preserved; queued overlays reuse the original editor.
    /// Caller-owned sinks can contain partial output on failure.
    pub fn save<W: Write + Seek>(
        &mut self,
        output: W,
        options: SaveOptions,
    ) -> Result<(W, SaveStats)> {
        if self.options.read.data_only && self.editor.is_dirty() {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Edited data-only model output remains unimplemented",
            ));
        }
        let result = self
            .editor
            .save_with_models(output, options, Some(&self.bank))?;
        self.bank
            .set_active_view_index(self.editor.active_view_index());
        Ok(result)
    }
    /// Atomically replace a path after a successful original-package save.
    pub fn save_path(&mut self, path: impl AsRef<Path>, options: SaveOptions) -> Result<SaveStats> {
        let path = path.as_ref();
        let parent = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temporary = tempfile::Builder::new()
            .prefix("crabxl-save-")
            .tempfile_in(parent)
            .map_err(|error| {
                Error::caused_by(ErrorKind::Io, "Cannot create adjacent loaded output", error)
            })?;
        let (_, stats) = self.save(&mut temporary, options)?;
        let temporary = temporary.into_temp_path();
        std::fs::rename(&temporary, path).map_err(|error| {
            Error::caused_by(ErrorKind::Io, "Cannot replace loaded output", error)
        })?;
        Ok(stats)
    }
    /// Release all loaded models and source caches, returning the caller's
    /// original seekable source without closing a caller-owned borrowed handle.
    pub fn into_source(self) -> R {
        self.editor.into_source()
    }
}
fn budget() -> Error {
    Error::new(
        ErrorKind::MemoryBudgetExceeded,
        "Loaded source/model aggregate allowance exceeded",
    )
}
