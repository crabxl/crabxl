//! Lazy materialization into the canonical owned bank, with joint source accounting.
mod catalog;
mod dimensions;
mod hyperlinks;
mod merges;
mod structure;
mod styles;
mod values;

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
    normalized_styles: bool,
    hyperlinks_requested: bool,
    hyperlinks_loaded: bool,
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
    source_style_count: usize,
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
        editor.structural_rich_text = options.read.rich_text;
        editor.structural_inline_rich_text = options
            .read
            .inline_rich_text
            .unwrap_or(options.read.rich_text);
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
                normalized_styles: false,
                hyperlinks_requested: false,
                hyperlinks_loaded: false,
            });
        }
        bank.set_active_view_index(reader.active_view_index());
        // Original declarations can select a hidden sheet; preserve their read
        // state while explicit future selection still requires a visible target.
        for (source, info) in sheets.iter().zip(reader.sheets()) {
            bank.set_sheet_visibility(source.id, info.visibility())?;
        }
        let source_style_count = bank
            .style_catalog()
            .map_or(1, |catalog| catalog.cell_formats.len().max(1));
        let mut value = Self {
            editor,
            bank,
            sheets,
            options,
            allowance,
            source_style_count,
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
    pub(super) fn mapping_bytes(&self) -> usize {
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
    pub(super) fn source_bytes(&self) -> usize {
        self.mapping_bytes()
            .saturating_add(self.editor.retained_package_bytes())
    }
    pub(super) fn package_extra_bytes(&self) -> usize {
        self.editor
            .retained_package_bytes()
            .saturating_sub(self.editor.book.retained_source_bytes())
    }
    pub(super) fn rebalance(&mut self) -> Result<()> {
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
            incoming.set_dimensions(self.editor.book.column_dimensions_with_allowance(
                name,
                temporary_limit.saturating_sub(incoming.charged_bytes()),
            )?)?;
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
                rows.capture_dimensions();
                rows.capture_merges();
                if self.sheets[index].hyperlinks_requested {
                    rows.capture_hyperlinks();
                }
                let mut row = Row::new(RowIndex::new(0)?);
                rows.set_aggregate_retained(retained.saturating_add(incoming.charged_bytes()))?;
                while rows.read_row_into(&mut row)? {
                    if let Some(dimension) = rows.row_dimension() {
                        incoming.set_row_dimension(dimension.clone())?;
                    }
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
                Ok((rows.take_merge_ranges(), rows.take_hyperlinks()))
            })();
            let (merges, links) = match decode {
                Ok(merges) => merges,
                Err(error) => {
                    drop(incoming);
                    self.rebalance()?;
                    return Err(error);
                }
            };
            let hyperlink_bytes = links.heap_bytes();
            let mut normalized_styles = false;
            let normalized = (|| {
                let geometry_bytes = merges
                    .capacity()
                    .saturating_mul(size_of::<crabxl_core::CellRange>())
                    .saturating_add(hyperlink_bytes);
                for range in &merges {
                    self.editor.book.rebalance_strings_for_retained(
                        self.bank
                            .charged_bytes()
                            .saturating_add(self.mapping_bytes())
                            .saturating_add(self.package_extra_bytes())
                            .saturating_add(incoming.charged_bytes())
                            .saturating_add(geometry_bytes),
                        maximum,
                    )?;
                    self.bank.set_memory_allowance(
                        maximum
                            .checked_sub(
                                self.source_bytes()
                                    .saturating_add(incoming.charged_bytes())
                                    .saturating_add(geometry_bytes),
                            )
                            .ok_or_else(budget)?,
                    )?;
                    let anchor = incoming.style_at(range.start);
                    let corner = (incoming.get(range.end).is_some()
                        || incoming.merged_ranges().virtual_style(range.end).is_some())
                    .then(|| incoming.style_at(range.end));
                    let (prepared, anchor) = self.bank.prepare_merge(*range, anchor, corner)?;
                    normalized_styles |= prepared
                        .appearances()
                        .iter()
                        .chain(std::iter::once(&anchor))
                        .any(|style| style.get() as usize >= self.source_style_count);
                    let available = maximum
                        .checked_sub(
                            self.source_bytes()
                                .saturating_add(self.bank.charged_bytes())
                                .saturating_add(geometry_bytes),
                        )
                        .ok_or_else(budget)?;
                    incoming.set_memory_allowance(
                        available
                            .min(self.options.workbook.sheet.max_bytes)
                            .min(self.options.resources.max_materialized_bytes),
                    )?;
                    incoming.edit().merge_prepared(prepared, anchor)?;
                }
                Ok(())
            })();
            drop(merges);
            if let Err(error) = normalized {
                drop(incoming);
                self.rebalance()?;
                return Err(error);
            }
            if self.sheets[index].hyperlinks_requested {
                let available = maximum
                    .saturating_sub(self.managed_retained_bytes())
                    .saturating_sub(incoming.charged_bytes());
                let result = (|| {
                    let links = self.editor.book.resolve_hyperlinks(
                        self.sheets[index].name.as_ref(),
                        links,
                        available,
                    )?;
                    incoming.set_memory_allowance(
                        incoming
                            .charged_bytes()
                            .saturating_add(available)
                            .min(self.options.workbook.sheet.max_bytes)
                            .min(self.options.resources.max_materialized_bytes),
                    )?;
                    incoming.set_hyperlinks(links)
                })();
                if let Err(error) = result {
                    drop(incoming);
                    self.rebalance()?;
                    return Err(error);
                }
            }
            let retained = self
                .bank
                .charged_bytes()
                .saturating_add(self.mapping_bytes())
                .saturating_add(self.package_extra_bytes());
            if let Err(error) = self.editor.apply_pending_model(
                self.sheets[index].name.as_ref(),
                &mut incoming,
                retained,
                maximum,
            ) {
                drop(incoming);
                self.rebalance()?;
                return Err(error);
            }
            incoming.set_visibility(self.bank.sheet(id)?.visibility());
            incoming.mark_clean();
            self.rebalance()?;
            self.bank.replace_sheet(id, incoming)?;
            self.sheets[index].hyperlinks_loaded = self.sheets[index].hyperlinks_requested;
            self.sheets[index].loaded = true;
            self.sheets[index].normalized_styles = normalized_styles;
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
        let result = self.editor.save_with_models(
            output,
            options,
            Some(&self.bank),
            self.mapping_bytes(),
        )?;
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
