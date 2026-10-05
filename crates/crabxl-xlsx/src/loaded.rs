//! Lazy materialization into the canonical owned bank, with joint source accounting.
use crate::{SharedStringOptions, WorkbookReader};
use crabxl_core::{
    DateEpoch, EditLimits, Error, ErrorKind, MemoryAllowance, MemoryPolicy, ReadOptions,
    ResourceLimits, Result, Row, RowIndex, SheetId, StyleLimits, Workbook, WorkbookLimits,
    Worksheet,
};
use std::{
    fs::File,
    io::{Read, Seek},
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
}
struct SourceSheet {
    id: SheetId,
    name: Box<str>,
    loaded: bool,
}
/// Owns a seekable original package and the canonical workbook bank. Source
/// styles transfer into the bank without cloning; only date classifications stay
/// in the reader. Sheets decode lazily into independently bounded temporary
/// models before a stable-ID commit. This is explicit editable-model loading,
/// not a streaming row mode or a whole-process RSS cap.
///
/// Original source assets remain on the seekable source. This initial ownership
/// API exposes read-only models; original-package model edits are staged.
pub struct LoadedWorkbook<R: Read + Seek = File> {
    reader: WorkbookReader<R>,
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
        let mut reader = WorkbookReader::with_limits(source, options.resources)?;
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
        let source_fixed = reader.catalog_memory_bytes().saturating_add(mapping);
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
            if sheet.kind() != crate::SheetKind::Worksheet {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Typed loaded chartsheet/dialog models remain unimplemented",
                )
                .with_part(sheet.part()));
            }
            let id = bank.create_sheet(sheet.name())?;
            sheets.push(SourceSheet {
                id,
                name: sheet.name().into(),
                loaded: false,
            });
        }
        if let Some(index) = reader.active_index() {
            bank.set_active_sheet(sheets[index].id)?;
        }
        let mut value = Self {
            reader,
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
        self.reader.shared_string_stats()
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
            .saturating_add(self.reader.retained_source_bytes())
    }
    fn rebalance(&mut self) -> Result<()> {
        let retained = self
            .mapping_bytes()
            .saturating_add(self.bank.charged_bytes());
        let maximum = self
            .allowance
            .retained_data_bytes
            .min(self.options.workbook.max_bytes);
        self.reader
            .rebalance_strings_for_retained(retained, maximum)?;
        let available = maximum
            .checked_sub(self.source_bytes())
            .ok_or_else(budget)?;
        self.bank.set_memory_allowance(available)
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
                    self.reader
                        .shared_string_stats()
                        .map_or(0, |stats| stats.managed_bytes)
                } else {
                    0
                };
            let fixed = self
                .bank
                .charged_bytes()
                .saturating_add(self.mapping_bytes())
                .saturating_add(self.reader.catalog_memory_bytes())
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
                .saturating_add(self.mapping_bytes());
            let decode = (|| {
                let mut rows = self.reader.rows_with_catalog_allowance(
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
            incoming.mark_clean();
            self.rebalance()?;
            self.bank.replace_sheet(id, incoming)?;
            self.sheets[index].loaded = true;
            self.rebalance()?;
        }
        self.bank.sheet(id)
    }
    /// Release all loaded models and source caches, returning the caller's
    /// original seekable source without closing a caller-owned borrowed handle.
    pub fn into_source(self) -> R {
        self.reader.into_inner()
    }
}
fn budget() -> Error {
    Error::new(
        ErrorKind::MemoryBudgetExceeded,
        "Loaded source/model aggregate allowance exceeded",
    )
}
