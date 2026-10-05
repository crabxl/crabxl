//! Lazy materialization into the canonical owned bank, with joint source accounting.
use crate::{EditorOptions, SaveOptions, SaveStats, SharedStringOptions, WorkbookEditor};
use crabxl_core::{
    Cell, CellAddress, CellValue, DateEpoch, EditLimits, Error, ErrorKind, MemoryAllowance,
    MemoryPolicy, ReadOptions, ResourceLimits, Result, Row, RowIndex, SheetId, StyleLimits,
    Workbook, WorkbookLimits, Worksheet,
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
        for sheet in reader.sheets() {
            let id = bank.create_sheet(sheet.name())?;
            sheets.push(SourceSheet {
                id,
                name: sheet.name().into(),
                loaded: false,
                kind: sheet.kind(),
            });
        }
        if let Some(index) = reader.active_index() {
            bank.set_active_sheet(sheets[index].id)?;
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
    /// Select a visible original sheet by stable identity without materializing
    /// its cells. Signed/unsupported metadata and resource failures reject before
    /// changing the bank or its pending original-package view.
    pub fn set_active_sheet(&mut self, id: SheetId) -> Result<()> {
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        let planned = self.editor.prepare_active(index)?;
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
        self.bank.set_memory_allowance(available)?;
        self.bank.set_active_sheet(id)?;
        self.editor.commit_active(index, planned);
        self.rebalance()
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
        let source = self.sheets.iter().find(|sheet| sheet.id == id)?;
        self.editor.pending_value(&source.name, address)
    }
    /// Borrow pending source cells in sparse order, without materialization.
    /// Source appearance is retained by the preserving editor on save.
    pub fn pending_cells(&self, id: SheetId) -> impl Iterator<Item = &Cell> {
        self.sheets
            .iter()
            .filter(move |sheet| sheet.id == id)
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
        let plan = self.editor.prepare_value(&source.name, address, &value)?;
        let loaded = source.loaded;
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
        self.editor.commit_value(plan, value, insert_missing);
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
                name,
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
                )?;
                let mut row = Row::new(RowIndex::new(0)?);
                rows.set_aggregate_retained(retained.saturating_add(incoming.charged_bytes()))?;
                while rows.read_row_into(&mut row)? {
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
        self.editor.save(output, options)
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
