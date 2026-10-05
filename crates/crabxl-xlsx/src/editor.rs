//! Lazy original-package preservation and bounded existing-cell value overlays.
use crate::encode::{RowBuffer, StyleContext, ValueEncoding, encode_cells, validate_value};
use crate::writer::{io_error, zip_error};
use crate::xml::{Scope, XmlStream, attribute};
use crate::{SheetInfo, SheetKind, WorkbookReader};
use crabxl_core::{
    Cell, CellAddress, CellStyle, CellValue, DateEpoch, Error, ErrorKind, ResourceLimits, Result,
    StyleId,
};
use crabxl_core::{MemoryAllowance, MemoryPolicy};
use quick_xml::{
    Writer,
    events::{BytesStart, Event},
};
use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::{self, BufReader, BufWriter, Read, Seek, Write},
    path::Path,
};
use zip::ZipWriter;

const PATCH_BYTES: usize = 256;
// Box large metadata values so sparse BTree nodes retain pointer-sized slots.
// Conservative per-part node allowance; model payload/capacities are additional.
const METADATA_ENTRY_BYTES: usize = 1024;
struct Patch {
    cell: Cell,
    insert_missing: bool,
}
type Patches = BTreeMap<(u32, u32), Patch>;
pub(crate) struct PatchPlan {
    sheet: usize,
    address: CellAddress,
    pub(crate) bytes: usize,
    cells: usize,
}
pub(crate) struct RowPatchPlan {
    plans: Vec<PatchPlan>,
    pub(crate) bytes: usize,
    pub(crate) scratch_bytes: usize,
}
struct CatalogOrder {
    positions: Vec<usize>,
    entries: Vec<BytesStart<'static>>,
    charged: usize,
}
fn catalog_order_bytes(positions: &Vec<usize>, entries: &Vec<BytesStart<'static>>) -> usize {
    PATCH_BYTES
        .saturating_add(positions.capacity().saturating_mul(size_of::<usize>()))
        .saturating_add(
            entries
                .capacity()
                .saturating_mul(size_of::<BytesStart<'static>>()),
        )
        .saturating_add(
            entries
                .iter()
                .map(|entry| entry.as_ref().len())
                .sum::<usize>(),
        )
}
pub(crate) struct OrderPlan {
    positions: Vec<usize>,
    entries: Option<Vec<BytesStart<'static>>>,
    pub(crate) bytes: usize,
    pub(crate) scratch_bytes: usize,
    pub(crate) view_index: i64,
}

/// Owned original-part inventory; content is not loaded into RAM.
#[derive(Clone, Debug)]
pub struct PartInfo {
    /// Original ZIP/OPC name.
    pub name: Box<str>,
    /// Declared compressed bytes.
    pub compressed_bytes: u64,
    /// Declared uncompressed bytes.
    pub uncompressed_bytes: u64,
    /// Original CRC; cataloging does not validate payloads.
    pub crc32: u32,
}
/// Policy for the derived calculation order after values or formulas change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalculationChainPolicy {
    /// Discard the obsolete chain and its package references; request full
    /// recalculation. Unchanged saves retain the original chain.
    #[default]
    DiscardOnEdit,
    /// Reject value/formula edits when a calculation-chain part is present.
    /// Pure display/printing changes retain the chain.
    RejectEdits,
}
/// Budgets for a lazy package editor and its owned value overlays.
#[derive(Clone, Debug)]
pub struct EditorOptions {
    /// Original archive/XML/metadata limits, also limiting rewritten XML parts.
    pub resources: ResourceLimits,
    /// Shared Auto or explicit managed operation budget, including work reserve.
    pub memory_policy: MemoryPolicy,
    /// Owned cell/view/printing overlay cap; excludes allocator overhead.
    pub max_patch_bytes: usize,
    /// Maximum distinct pending cell replacements.
    pub max_patch_cells: usize,
    /// How to handle the original derived calculation order during edits.
    pub calculation_chain: CalculationChainPolicy,
    /// Nonfinite value/caches use compatible blanks unless strict rejection is requested.
    pub non_finite: crate::NonFiniteWritePolicy,
    /// Formula attribute omission for assigned replacement values.
    pub formula_attributes: crate::FormulaWritePolicy,
}
impl Default for EditorOptions {
    fn default() -> Self {
        Self {
            resources: ResourceLimits::default(),
            max_patch_bytes: usize::MAX,
            memory_policy: MemoryPolicy::default(),
            max_patch_cells: 10_000_000,
            calculation_chain: CalculationChainPolicy::default(),
            non_finite: crate::NonFiniteWritePolicy::default(),
            formula_attributes: crate::FormulaWritePolicy::default(),
        }
    }
}
/// Controls CRC validation during copying. Rewritten XML is always parsed to EOF.
#[derive(Clone, Copy, Debug, Default)]
pub struct SaveOptions {
    /// ZIP level 0..=9 for rewritten parts; 0 stores without compression.
    /// Copied entries retain their original compressed bytes.
    /// None retains the backend default (6).
    pub compression_level: Option<u8>,
    /// Decompress unchanged parts to a bounded sink and validate CRC before raw
    /// copying. False preserves compressed payloads without revalidating them.
    pub verify_unchanged: bool,
}
/// Per-save part counts and XML rewrite bytes. No worksheet temp files are used.
#[derive(Clone, Copy, Debug, Default)]
pub struct SaveStats {
    /// Entries passed through without decompression/recompression (except validation).
    pub copied_parts: usize,
    /// Obsolete calculation-chain parts/relationships omitted on an edited save.
    pub removed_parts: usize,
    /// XML parts rewritten through bounded events.
    pub rewritten_parts: usize,
    /// Actual bytes emitted across rewritten XML parts.
    pub rewritten_xml_bytes: u64,
}
#[derive(Clone, Copy)]
enum ActivePatch {
    Visible(usize),
    Deferred(i64),
}
/// Owns the original source and a bounded overlay, keeping unknown XML/binary
/// parts on their original source. save borrows self, so repeated saves retain
/// images/macros and never consume the original or the pending edits.
///
/// Cell values/upserts, worksheet views and printing share bounded overlays.
/// Styles, original relationships and unrelated content are preserved.
/// Date/style registration and structural edits in existing packages remain staged.
/// Value/formula edits discard derived chains under the default policy, invalidate
/// worksheet formula caches and request recalculation. Pure display/printing edits
/// retain caches/chains and rewrite only their selected worksheets.
pub struct WorkbookEditor<R: Read + Seek = File> {
    pub(crate) book: WorkbookReader<R>,
    parts: Vec<PartInfo>,
    patches: BTreeMap<String, Patches>,
    view_patches: BTreeMap<String, Box<crabxl_core::SheetViews>>,
    print_patches: BTreeMap<String, Box<crabxl_core::PrintSettings>>,
    active_patch: Option<ActivePatch>,
    visibility_patches: BTreeMap<usize, crabxl_core::SheetVisibility>,
    name_patches: BTreeMap<usize, Box<str>>,
    catalog_order: Option<Box<CatalogOrder>>,
    options: EditorOptions,
    patch_bytes: usize,
    patch_cells: usize,
    signed: bool,
    calc_chain_parts: HashSet<String>,
    chain_removals: HashSet<String>,
    chain_safe: bool,
    workbook_relationships: String,
    allowance: MemoryAllowance,
    shared_string_parts: HashSet<String>,
}
impl WorkbookEditor<File> {
    /// Open a source without loading worksheet cells, images or shared strings.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(path)
            .map_err(|error| io_error("Cannot open editable workbook source", error))?;
        Self::new(file)
    }
}
impl<R: Read + Seek> WorkbookEditor<R> {
    /// Catalog an owned original package with default edit limits.
    pub fn new(source: R) -> Result<Self> {
        Self::with_options(source, EditorOptions::default())
    }
    /// Catalog a package with explicit source and patch allowances.
    pub fn with_options(source: R, mut options: EditorOptions) -> Result<Self> {
        if options.max_patch_bytes == 0 || options.max_patch_cells == 0 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Editor patch limits must be positive",
            ));
        }
        let allowance =
            crate::adaptive::memory_allowance(options.memory_policy, options.resources)?;
        options.max_patch_bytes = options.max_patch_bytes.min(allowance.retained_data_bytes);
        let mut book = WorkbookReader::with_limits(source, options.resources)?;
        let mut parts = Vec::new();
        let entries = book.archive.len();
        let base = entries.saturating_mul(size_of::<PartInfo>());
        if base as u128 > u128::from(options.resources.max_metadata_bytes) {
            return Err(limit("Original part inventory budget exceeded"));
        }
        parts.try_reserve_exact(entries).map_err(|error| {
            Error::caused_by(
                ErrorKind::LimitExceeded,
                "Cannot allocate original part inventory",
                error,
            )
        })?;
        // Also charge duplicate-name detection conservatively, including names.
        let mut bytes = parts.capacity() * size_of::<PartInfo>();
        let mut names = HashSet::new();
        let mut signed = false;
        for index in 0..entries {
            let file = book
                .archive
                .by_index(index)
                .map_err(|error| zip_error("Cannot catalog original part", error))?;
            if file.size() > options.resources.max_part_bytes {
                return Err(limit("Original part byte limit exceeded").with_part(file.name()));
            }
            bytes = bytes
                .saturating_add(file.name().len().saturating_mul(2))
                .saturating_add(128);
            if bytes as u128 > u128::from(options.resources.max_metadata_bytes) {
                return Err(limit("Original part inventory budget exceeded"));
            }
            if !names.insert(file.name().to_owned()) {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Duplicate original ZIP part name",
                ));
            }
            signed |= file.name().starts_with("_xmlsignatures/");
            parts.push(PartInfo {
                name: file.name().into(),
                compressed_bytes: file.compressed_size(),
                uncompressed_bytes: file.size(),
                crc32: file.crc32(),
            });
        }
        let part = "[Content_Types].xml";
        let file = book
            .archive
            .by_name(part)
            .map_err(|error| zip_error("Cannot inspect signature/calculation part types", error))?;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(options.resources.input_buffer_bytes, file),
            part.into(),
            options.resources.max_metadata_bytes,
            options.resources,
        );
        let mut calc_chain_parts = HashSet::new();
        let mut shared_string_parts = HashSet::new();
        loop {
            let frame = xml.next()?;
            match frame.event {
                Event::Start(e) if frame.scope == Scope::ContentTypes && frame.depth == 2 => {
                    if let Some(kind) = attribute(&e, b"ContentType")? {
                        signed |= kind.contains("digital-signature");
                        if e.local_name().as_ref().as_bytes() == b"Override"
                            && (kind.ends_with("sharedStrings+xml")
                                || kind.ends_with("calcChain+xml"))
                        {
                            let name = attribute(&e, b"PartName")?.ok_or_else(|| {
                                invalid("Cataloged content type override has no part name")
                            })?;
                            bytes = bytes.saturating_add(name.len()).saturating_add(128);
                            if bytes as u128 > u128::from(options.resources.max_metadata_bytes) {
                                return Err(limit(
                                    "Original string/calculation part catalog budget exceeded",
                                ));
                            }
                            let name = crate::package::resolve_part("", &name)?;
                            if kind.ends_with("calcChain+xml") {
                                calc_chain_parts.insert(name);
                            } else {
                                shared_string_parts.insert(name);
                            }
                        }
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        drop(xml);
        let workbook_relationships = crate::package::relationship_part(&book.workbook_part);
        let (chain_removals, chain_safe) = catalog_chain_removal(
            &mut book,
            &parts,
            &calc_chain_parts,
            &workbook_relationships,
            options.resources,
            &mut bytes,
        )?;
        Ok(Self {
            book,
            parts,
            patches: BTreeMap::new(),
            view_patches: BTreeMap::new(),
            print_patches: BTreeMap::new(),
            active_patch: None,
            visibility_patches: BTreeMap::new(),
            name_patches: BTreeMap::new(),
            catalog_order: None,
            options,
            patch_bytes: 0,
            patch_cells: 0,
            signed,
            calc_chain_parts,
            chain_removals,
            chain_safe,
            workbook_relationships,
            allowance,
            shared_string_parts,
        })
    }
    /// Original sheet catalog; opaque preservation does not imply typed reading.
    pub fn sheets(&self) -> &[SheetInfo] {
        self.book.sheets()
    }
    /// Original part inventory; no payloads are resident here.
    pub fn parts(&self) -> &[PartInfo] {
        &self.parts
    }
    /// Whether pending overlays are present. Saving does not discard overlays.
    pub fn is_dirty(&self) -> bool {
        self.patch_cells != 0
            || !self.view_patches.is_empty()
            || !self.print_patches.is_empty()
            || self.active_patch.is_some()
            || !self.visibility_patches.is_empty()
            || !self.name_patches.is_empty()
            || self.catalog_order.is_some()
    }
    /// Shared automatic/explicit operation allowance computed at construction.
    /// Original ZIP/catalog/inventory allocations are additional.
    pub fn memory_allowance(&self) -> &MemoryAllowance {
        &self.allowance
    }
    /// Effective patch cap after combining policy and max_patch_bytes.
    pub fn patch_allowance(&self) -> usize {
        self.options.max_patch_bytes
    }
    /// Conservative managed patch allowance currently used.
    pub fn patch_bytes(&self) -> usize {
        self.patch_bytes
    }
    /// Rename an original catalog entry without changing its part or relationship
    /// identity. The selector is its original source name, even after a rename.
    /// Formula expressions and defined-name text retain reference behavior: they
    /// are not automatically rewritten when a sheet title changes.
    pub fn rename_sheet(&mut self, source_name: &str, name: impl Into<Box<str>>) -> Result<()> {
        let index = self
            .book
            .sheets()
            .iter()
            .position(|sheet| sheet.name() == source_name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet does not exist"))?;
        let name = name.into();
        let planned = self.prepare_name(index, &name)?;
        self.commit_name(index, name, planned);
        Ok(())
    }
    pub(crate) fn prepare_name(&mut self, index: usize, name: &str) -> Result<usize> {
        crate::encode::validate_xml_text(name)?;
        if name.is_empty() || name.chars().any(|ch| ":\\/?*[]".contains(ch)) {
            return Err(invalid("Invalid worksheet name"));
        }
        let folded = name.to_lowercase();
        if self
            .book
            .sheets()
            .iter()
            .enumerate()
            .any(|(position, sheet)| {
                position != index
                    && self
                        .name_patches
                        .get(&position)
                        .map_or(sheet.name(), |value| value.as_ref())
                        .to_lowercase()
                        == folded
            })
        {
            return Err(invalid("Duplicate worksheet name"));
        }
        let previous = self.name_patches.get(&index);
        let bytes = self
            .patch_bytes
            .saturating_sub(previous.map_or(0, |name| name.len()))
            .saturating_add(if previous.is_none() { PATCH_BYTES } else { 0 })
            .saturating_add(name.len());
        self.validate_workbook_patch(index, bytes, false)?;
        Ok(bytes)
    }
    pub(crate) fn commit_name(&mut self, index: usize, name: Box<str>, bytes: usize) {
        self.name_patches.insert(index, name);
        self.patch_bytes = bytes;
    }
    /// Reorder an original sheet to a zero-based display position, retaining its
    /// source part and the signed active-view index. Local defined-name graphs
    /// remain an explicit staged dependency and reject before catalog mutation.
    pub fn move_sheet(&mut self, source_name: &str, position: usize) -> Result<()> {
        let index = self
            .book
            .sheets()
            .iter()
            .position(|sheet| sheet.name() == source_name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet does not exist"))?;
        let available = self
            .options
            .max_patch_bytes
            .saturating_sub(self.patch_bytes);
        let plan = self.prepare_order(index, position, available)?;
        self.commit_order(plan);
        Ok(())
    }
    pub(crate) fn prepare_order(
        &mut self,
        index: usize,
        position: usize,
        scratch_allowance: usize,
    ) -> Result<OrderPlan> {
        let count = self.book.sheets().len();
        if index >= count || position >= count {
            return Err(invalid("Sheet position is out of range"));
        }
        self.validate_workbook_patch(index, self.patch_bytes, false)?;
        let scratch_bytes = count.saturating_mul(size_of::<usize>());
        if scratch_bytes > scratch_allowance {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Sheet order planning allowance exceeded",
            ));
        }
        let mut positions = Vec::new();
        positions.try_reserve_exact(count).map_err(|cause| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate sheet order",
                cause,
            )
        })?;
        positions.extend((0..count).map(|display| self.source_index(display)));
        let old = positions
            .iter()
            .position(|source| *source == index)
            .ok_or_else(|| invalid("Original sheet order is inconsistent"))?;
        positions.remove(old);
        positions.insert(position, index);
        let entries = if self.catalog_order.is_none() {
            Some(self.read_catalog_entries(scratch_allowance.saturating_sub(scratch_bytes))?)
        } else {
            None
        };
        let charged = entries.as_ref().map_or_else(
            || self.catalog_order.as_ref().map_or(0, |order| order.charged),
            |entries| catalog_order_bytes(&positions, entries),
        );
        let previous = self.catalog_order.as_ref().map_or(0, |order| order.charged);
        let bytes = self
            .patch_bytes
            .saturating_sub(previous)
            .saturating_add(charged)
            .saturating_add(if self.active_patch.is_none() {
                PATCH_BYTES
            } else {
                0
            });
        if bytes > self.options.max_patch_bytes {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Sheet order patch allowance exceeded",
            ));
        }
        Ok(OrderPlan {
            positions,
            entries,
            bytes,
            scratch_bytes,
            view_index: self.active_view_index(),
        })
    }
    fn read_catalog_entries(&mut self, allowance: usize) -> Result<Vec<BytesStart<'static>>> {
        let part = self.book.workbook_part.clone();
        let count = self.book.sheets().len();
        let mut entries = Vec::new();
        let fixed = PATCH_BYTES
            .saturating_add(count.saturating_mul(size_of::<BytesStart<'static>>()))
            .saturating_add(count.saturating_mul(size_of::<usize>()));
        if fixed > allowance {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Sheet catalog planning allowance exceeded",
            ));
        }
        entries.try_reserve_exact(count).map_err(|cause| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate original sheet catalog",
                cause,
            )
        })?;
        let input = self
            .book
            .archive
            .by_name(&part)
            .map_err(|cause| zip_error("Cannot inspect original sheet order", cause))?;
        let limits = self.options.resources;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(limits.input_buffer_bytes, input),
            part.clone(),
            limits.max_metadata_bytes.min(limits.max_part_bytes),
            limits,
        );
        let mut charged = fixed;
        let mut sheets_open = false;
        let mut sheet_open = false;
        let mut names_open = false;
        loop {
            let frame = xml.next()?;
            check_declaration(&frame.event)?;
            match &frame.event {
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 2
                        && e.local_name().as_ref().as_bytes() == b"definedNames" =>
                {
                    names_open = true
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 1
                        && e.local_name().as_ref().as_bytes() == b"definedNames" =>
                {
                    names_open = false
                }
                Event::Start(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 2
                        && e.local_name().as_ref().as_bytes() == b"sheets" =>
                {
                    sheets_open = true
                }
                Event::End(e)
                    if frame.scope == Scope::Spreadsheet
                        && frame.depth == 1
                        && e.local_name().as_ref().as_bytes() == b"sheets" =>
                {
                    sheets_open = false
                }
                Event::Start(e) if sheets_open && frame.depth == 3 => {
                    if frame.scope != Scope::Spreadsheet
                        || e.local_name().as_ref().as_bytes() != b"sheet"
                    {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "Sheet catalog extensions require typed reorder handling",
                        )
                        .with_part(&part));
                    }
                    charged = charged.saturating_add(e.as_ref().len());
                    if charged > allowance || entries.len() == count {
                        return Err(Error::new(
                            ErrorKind::MemoryBudgetExceeded,
                            "Sheet catalog planning allowance exceeded",
                        )
                        .with_part(&part));
                    }
                    for attribute in e.attributes() {
                        attribute.map_err(|cause| {
                            Error::caused_by(
                                ErrorKind::Xml,
                                "Invalid sheet catalog attribute",
                                cause,
                            )
                            .with_part(&part)
                        })?;
                    }
                    entries.push(e.to_owned());
                    sheet_open = true;
                }
                Event::Start(_) if sheet_open => {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Nested sheet catalog content requires typed reorder handling",
                    )
                    .with_part(&part));
                }
                Event::End(_) if sheet_open && frame.depth == 2 => sheet_open = false,
                Event::Text(text)
                    if sheet_open
                        && !text.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) =>
                {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Nested sheet catalog content requires typed reorder handling",
                    )
                    .with_part(&part));
                }
                Event::CData(_) | Event::GeneralRef(_) | Event::Comment(_) | Event::PI(_)
                    if sheet_open =>
                {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Nested sheet catalog content requires typed reorder handling",
                    )
                    .with_part(&part));
                }
                Event::Start(e)
                    if names_open
                        && frame.scope == Scope::Spreadsheet
                        && frame.depth == 3
                        && e.local_name().as_ref().as_bytes() == b"definedName"
                        && attribute(e, b"localSheetId")?.is_some() =>
                {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Local defined-name graph reordering remains unimplemented",
                    )
                    .with_part(&part));
                }
                Event::Eof => break,
                _ => {}
            }
        }
        if entries.len() != count {
            return Err(invalid("Original sheet catalog changed").with_part(&part));
        }
        Ok(entries)
    }
    pub(crate) fn commit_order(&mut self, plan: OrderPlan) {
        if let Some(entries) = plan.entries {
            let charged = catalog_order_bytes(&plan.positions, &entries);
            self.catalog_order = Some(Box::new(CatalogOrder {
                positions: plan.positions,
                entries,
                charged,
            }));
        } else if let Some(order) = &mut self.catalog_order {
            order.positions = plan.positions;
        }
        self.active_patch = Some(ActivePatch::Deferred(plan.view_index));
        self.patch_bytes = plan.bytes;
    }
    fn source_index(&self, display: usize) -> usize {
        self.catalog_order
            .as_ref()
            .map_or(display, |order| order.positions[display])
    }
    /// Select a visible original worksheet/chartsheet without decoding its cells.
    /// Only workbook view metadata changes; formula caches/chains stay intact.
    pub fn set_active_sheet(&mut self, name: &str) -> Result<()> {
        let index = self
            .book
            .sheets()
            .iter()
            .position(|sheet| sheet.name() == name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Active sheet does not exist"))?;
        let index = (0..self.book.sheets().len())
            .find(|display| self.source_index(*display) == index)
            .ok_or_else(|| invalid("Original sheet order is inconsistent"))?;
        let bytes = self.prepare_active(index)?;
        self.commit_active(index, bytes);
        Ok(())
    }
    pub(crate) fn prepare_active(&mut self, index: usize) -> Result<usize> {
        let bytes = self
            .patch_bytes
            .saturating_add(if self.active_patch.is_none() {
                PATCH_BYTES
            } else {
                0
            });
        self.validate_workbook_patch(index, bytes, true)?;
        Ok(bytes)
    }
    /// Set a deferred workbook view. Relative, hidden and out-of-range indexes
    /// are resolved when saving, independently of strict visible-ID selection.
    pub fn set_active_view_index(&mut self, index: i64) -> Result<()> {
        let bytes = self.prepare_active_view(index)?;
        self.commit_active_view(index, bytes);
        Ok(())
    }
    pub(crate) fn prepare_active_view(&mut self, index: i64) -> Result<usize> {
        let bytes = self
            .patch_bytes
            .saturating_add(if self.active_patch.is_none() {
                PATCH_BYTES
            } else {
                0
            });
        let position =
            crabxl_core::resolve_sheet_index(index, self.book.sheets().len()).unwrap_or(0);
        self.validate_workbook_patch(position, bytes, false)?;
        Ok(bytes)
    }
    pub(crate) fn commit_active_view(&mut self, index: i64, bytes: usize) {
        self.active_patch = Some(ActivePatch::Deferred(index));
        self.patch_bytes = bytes;
    }
    /// Pending signed view, or the original declaration when unchanged.
    pub fn active_view_index(&self) -> i64 {
        match self.active_patch {
            Some(ActivePatch::Visible(index)) => index as i64,
            Some(ActivePatch::Deferred(index)) => index,
            None => self.book.active_view_index(),
        }
    }
    /// Effective catalog visibility without decoding any worksheet cells.
    pub fn sheet_visibility(&self, name: &str) -> Result<crabxl_core::SheetVisibility> {
        let index = self
            .book
            .sheets()
            .iter()
            .position(|sheet| sheet.name() == name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet does not exist"))?;
        Ok(self.visibility_at_source(index))
    }
    /// Change an original sheet's catalog state without changing its contents.
    /// An all-hidden intermediate model is permitted; saving rejects it before
    /// writing output, so callers can restore a visible sheet and retry.
    pub fn set_sheet_visibility(
        &mut self,
        name: &str,
        visibility: crabxl_core::SheetVisibility,
    ) -> Result<()> {
        let index = self
            .book
            .sheets()
            .iter()
            .position(|sheet| sheet.name() == name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet does not exist"))?;
        let bytes = self.prepare_visibility(index)?;
        self.commit_visibility(index, visibility, bytes);
        Ok(())
    }
    pub(crate) fn prepare_visibility(&mut self, index: usize) -> Result<usize> {
        let bytes = self
            .patch_bytes
            .saturating_add(if self.visibility_patches.contains_key(&index) {
                0
            } else {
                PATCH_BYTES
            })
            // Reserve the fixed active overlay now, so successful save can
            // normalize a newly hidden selection without growing the ledger.
            .saturating_add(if self.active_patch.is_none() {
                PATCH_BYTES
            } else {
                0
            });
        self.validate_workbook_patch(index, bytes, false)?;
        Ok(bytes)
    }
    pub(crate) fn commit_visibility(
        &mut self,
        index: usize,
        visibility: crabxl_core::SheetVisibility,
        bytes: usize,
    ) {
        self.visibility_patches.insert(index, visibility);
        if self.active_patch.is_none() {
            self.active_patch = Some(ActivePatch::Visible(self.book.active_index().unwrap_or(0)));
        }
        self.patch_bytes = bytes;
    }
    fn visibility_at(&self, index: usize) -> crabxl_core::SheetVisibility {
        self.visibility_at_source(self.source_index(index))
    }
    fn visibility_at_source(&self, index: usize) -> crabxl_core::SheetVisibility {
        self.visibility_patches
            .get(&index)
            .copied()
            .unwrap_or_else(|| self.book.sheets()[index].visibility())
    }
    pub(crate) fn active_index(&self) -> Option<usize> {
        crabxl_core::resolve_sheet_index(self.active_view_index(), self.book.sheets().len())
    }
    fn active_for_save(&self) -> Result<Option<crabxl_core::ActiveViewSelection>> {
        if self.active_patch.is_none() && self.visibility_patches.is_empty() {
            return Ok(None);
        }
        if let Some(ActivePatch::Deferred(index)) = self.active_patch {
            return crabxl_core::normalize_active_view(
                index,
                self.book.sheets().len(),
                |position| self.visibility_at(position),
            )
            .map(Some);
        }
        let mut visible = (0..self.book.sheets().len())
            .filter(|index| self.visibility_at(*index) == crabxl_core::SheetVisibility::Visible);
        let first = visible
            .clone()
            .next()
            .ok_or_else(|| invalid("A workbook requires at least one visible sheet"))?;
        let active = self.active_index().unwrap_or(first);
        let index = visible.find(|index| *index >= active).unwrap_or(first) as i64;
        Ok(Some(crabxl_core::ActiveViewSelection {
            serialized_index: Some(index),
            requested_index: index,
        }))
    }
    fn validate_workbook_patch(
        &mut self,
        index: usize,
        bytes: usize,
        require_visible: bool,
    ) -> Result<()> {
        let count = self.book.sheets().len();
        if index >= count {
            return Err(Error::new(
                ErrorKind::SheetNotFound,
                "Active sheet does not exist",
            ));
        }
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing signed packages is unsupported",
            ));
        }
        if require_visible && self.visibility_at(index) != crabxl_core::SheetVisibility::Visible {
            return Err(invalid("Active sheet must be visible"));
        }
        if bytes > self.options.max_patch_bytes {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Workbook metadata patch allowance exceeded",
            ));
        }
        let part = self.book.workbook_part.clone();
        let input = self
            .book
            .archive
            .by_name(&part)
            .map_err(|error| zip_error("Cannot inspect active sheet metadata", error))?;
        let limits = self.options.resources;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(limits.input_buffer_bytes, input),
            part.clone(),
            limits.max_metadata_bytes.min(limits.max_part_bytes),
            limits,
        );
        let mut sheet_index = 0usize;
        let mut sheets_open = false;
        loop {
            let frame = xml.next()?;
            check_declaration(&frame.event)?;
            if frame.scope == Scope::Spreadsheet {
                match &frame.event {
                    Event::Start(e)
                        if frame.depth == 2 && e.local_name().as_ref().as_bytes() == b"sheets" =>
                    {
                        sheets_open = true
                    }
                    Event::End(e)
                        if frame.depth == 1 && e.local_name().as_ref().as_bytes() == b"sheets" =>
                    {
                        sheets_open = false
                    }
                    _ => {}
                }
            }
            match frame.event {
                Event::Start(e) if e.local_name().as_ref().as_bytes() == b"AlternateContent" => {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Editing markup-compatibility alternatives requires typed branch handling",
                    )
                    .with_part(&part));
                }
                Event::Start(e)
                    if sheets_open
                        && frame.scope == Scope::Spreadsheet
                        && frame.depth == 3
                        && e.local_name().as_ref().as_bytes() == b"sheet" =>
                {
                    sheet_index += 1;
                }
                Event::Eof => break,
                _ => {}
            }
        }
        if sheet_index != count {
            return Err(invalid("Original sheet catalog changed").with_part(&part));
        }
        Ok(())
    }
    pub(crate) fn commit_active(&mut self, index: usize, bytes: usize) {
        self.active_patch = Some(ActivePatch::Visible(index));
        self.patch_bytes = bytes;
    }
    pub(crate) fn retained_package_bytes(&self) -> usize {
        self.book
            .retained_source_bytes()
            .saturating_add(size_of::<Self>())
            .saturating_add(self.parts.capacity().saturating_mul(size_of::<PartInfo>()))
            .saturating_add(self.parts.iter().map(|part| part.name.len()).sum::<usize>())
            .saturating_add(self.workbook_relationships.capacity())
            .saturating_add(
                [
                    &self.calc_chain_parts,
                    &self.chain_removals,
                    &self.shared_string_parts,
                ]
                .into_iter()
                .map(|parts| {
                    parts
                        .capacity()
                        .saturating_mul(128)
                        .saturating_add(parts.iter().map(String::capacity).sum::<usize>())
                })
                .sum::<usize>(),
            )
            .saturating_add(self.patch_bytes)
    }
    pub(crate) fn apply_pending_model(
        &mut self,
        name: &str,
        incoming: &mut crabxl_core::Worksheet,
        retained: usize,
        maximum: usize,
    ) -> Result<()> {
        let part = self
            .book
            .sheets()
            .iter()
            .find(|sheet| sheet.name() == name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Source worksheet not found"))?
            .part()
            .to_owned();
        if let Some(patches) = self.patches.get(&part) {
            for patch in patches.values() {
                // Reserve incoming cloned payload and a conservative new cell
                // node before transferring an overlay into the cached model.
                let desired = retained
                    .saturating_add(incoming.charged_bytes())
                    .saturating_add(PATCH_BYTES)
                    .saturating_add(patch.cell.value.heap_bytes());
                self.book.rebalance_strings_for_retained(desired, maximum)?;
                if desired.saturating_add(self.book.retained_source_bytes()) > maximum {
                    return Err(Error::new(
                        ErrorKind::MemoryBudgetExceeded,
                        "Loaded overlay/model allowance exceeded",
                    ));
                }
                let mut cell = patch.cell.clone();
                cell.style = incoming
                    .get(cell.address)
                    .map_or(StyleId::new(0), |original| original.style);
                incoming.set(cell)?;
            }
        }
        self.book.rebalance_strings_for_retained(
            retained.saturating_add(incoming.charged_bytes()),
            maximum,
        )
    }
    /// Read original canonical view metadata. Pending replacements can be borrowed
    /// separately without cloning their payloads.
    pub fn sheet_views(&mut self, sheet: &str) -> Result<crabxl_core::SheetViews> {
        self.book.sheet_views(sheet)
    }
    /// Borrow a pending display replacement.
    pub fn pending_sheet_views(&self, sheet: &str) -> Option<&crabxl_core::SheetViews> {
        let part = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)?
            .part();
        self.view_patches.get(part).map(Box::as_ref)
    }
    /// Replace viewport metadata while preserving cell values, caches and unrelated
    /// package parts. Unknown source view extensions reject replacement explicitly.
    pub fn set_sheet_views(&mut self, sheet: &str, views: crabxl_core::SheetViews) -> Result<()> {
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing signed worksheet views requires an explicit signature policy",
            ));
        }
        crate::worksheet_view::validate(&views)?;
        let info = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)
            .ok_or_else(|| {
                Error::new(ErrorKind::SheetNotFound, "Worksheet view source not found")
            })?;
        let part = info.part().to_owned();
        let old = self
            .view_patches
            .get(&part)
            .map_or(0, |views| views.memory_bytes());
        let node = if self.view_patches.contains_key(&part) {
            0
        } else {
            METADATA_ENTRY_BYTES.saturating_add(part.len())
        };
        let bytes = self
            .patch_bytes
            .saturating_sub(old)
            .saturating_add(node)
            .saturating_add(views.memory_bytes());
        if bytes > self.options.max_patch_bytes {
            return Err(limit("Worksheet view overlay allowance exceeded"));
        }
        // Header parsing verifies supported source metadata before it can be replaced.
        // The temporary original model is bounded separately and released immediately.
        let source_allowance = self
            .allowance
            .retained_data_bytes
            .saturating_sub(bytes.max(self.patch_bytes));
        self.book
            .sheet_views_with_allowance(sheet, source_allowance)?;
        self.view_patches.insert(part, Box::new(views));
        self.patch_bytes = bytes;
        Ok(())
    }
    /// Read original printing metadata through worksheet EOF/CRC.
    pub fn print_settings(&mut self, sheet: &str) -> Result<crabxl_core::PrintSettings> {
        self.book.print_settings(sheet)
    }
    /// Borrow a pending canonical printing replacement.
    pub fn pending_print_settings(&self, sheet: &str) -> Option<&crabxl_core::PrintSettings> {
        let part = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)?
            .part();
        self.print_patches.get(part).map(Box::as_ref)
    }
    /// Replace printing metadata while preserving unrelated worksheet/package data.
    /// Printer relationship identity must match the original; graph mutation is separate.
    pub fn set_print_settings(
        &mut self,
        sheet: &str,
        settings: crabxl_core::PrintSettings,
    ) -> Result<()> {
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing signed printing settings requires an explicit signature policy",
            ));
        }
        crate::printing::validate(&settings)?;
        let info = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::SheetNotFound,
                    "Printing worksheet source not found",
                )
            })?;
        let part = info.part().to_owned();
        let old = self
            .print_patches
            .get(&part)
            .map_or(0, |settings| settings.memory_bytes());
        let node = if self.print_patches.contains_key(&part) {
            0
        } else {
            METADATA_ENTRY_BYTES.saturating_add(part.len())
        };
        let bytes = self
            .patch_bytes
            .saturating_sub(old)
            .saturating_add(node)
            .saturating_add(settings.memory_bytes());
        if bytes > self.options.max_patch_bytes {
            return Err(limit("Printing overlay allowance exceeded"));
        }
        let source_allowance = self
            .allowance
            .retained_data_bytes
            .saturating_sub(bytes.max(self.patch_bytes));
        if let Some(pending) = self.print_patches.get(&part) {
            check_printer_identity(&pending.setup, &settings.setup)?;
        } else {
            let original = self
                .book
                .print_settings_with_allowance(sheet, source_allowance)?;
            check_printer_identity(&original.setup, &settings.setup)?;
        }
        self.print_patches.insert(part, Box::new(settings));
        self.patch_bytes = bytes;
        Ok(())
    }
    /// Update one canonical printing component without cloning unrelated vectors.
    /// The first update validates the original through EOF/CRC; later updates use
    /// the validated overlay without reopening the source. Graph identity remains fixed.
    /// Incoming payloads and parser working buffers are additional to retained overlays.
    pub fn update_print_settings(
        &mut self,
        sheet: &str,
        change: crabxl_core::PrintSettingsChange,
    ) -> Result<()> {
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing signed printing settings requires an explicit signature policy",
            ));
        }
        crate::printing::validate_change(&change)?;
        let info = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::SheetNotFound,
                    "Printing worksheet source not found",
                )
            })?;
        let part = info.part();
        if let Some(settings) = self.print_patches.get_mut(part) {
            if let crabxl_core::PrintSettingsChange::Setup(setup) = &change {
                check_printer_identity(&settings.setup, setup)?;
            }
            let other = self.patch_bytes.saturating_sub(settings.memory_bytes());
            let maximum = self.options.max_patch_bytes.saturating_sub(other);
            settings.update(change, maximum)?;
            self.patch_bytes = other.saturating_add(settings.memory_bytes());
            return Ok(());
        }
        let part = part.to_owned();
        let other = self
            .patch_bytes
            .saturating_add(METADATA_ENTRY_BYTES)
            .saturating_add(part.len());
        let maximum = self
            .options
            .max_patch_bytes
            .saturating_sub(other)
            .min(self.allowance.retained_data_bytes.saturating_sub(other));
        let mut settings = self.book.print_settings_with_allowance(sheet, maximum)?;
        if let crabxl_core::PrintSettingsChange::Setup(setup) = &change {
            check_printer_identity(&settings.setup, setup)?;
        }
        settings.update(change, maximum)?;
        self.patch_bytes = other.saturating_add(settings.memory_bytes());
        self.print_patches.insert(part, Box::new(settings));
        Ok(())
    }
    /// Replace an existing cell's value, preserving its style and unrelated cell
    /// attributes. Cell existence and unsupported metadata are checked on save.
    /// Original dates/shared/rich strings can be retained opaquely; creating a
    /// typed date here requires the later read-side style catalog.
    pub fn set_value(&mut self, sheet: &str, address: CellAddress, value: CellValue) -> Result<()> {
        self.queue_value(sheet, address, value, false)
    }
    /// Replace an existing cell or insert a missing physical cell. Existing
    /// cells retain their style; new cells use the default style. Insertions
    /// update an existing dimension and make inferred coordinates explicit.
    /// Non-anchor cells of merged ranges are rejected during save.
    pub fn upsert_value(
        &mut self,
        sheet: &str,
        address: CellAddress,
        value: CellValue,
    ) -> Result<()> {
        self.queue_value(sheet, address, value, true)
    }
    fn queue_value(
        &mut self,
        sheet: &str,
        address: CellAddress,
        value: CellValue,
        insert_missing: bool,
    ) -> Result<()> {
        let plan = self.prepare_value(sheet, address, &value)?;
        self.commit_value(plan, value, insert_missing);
        Ok(())
    }
    pub(crate) fn prepare_value(
        &self,
        sheet: &str,
        address: CellAddress,
        value: &CellValue,
    ) -> Result<PatchPlan> {
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing a digitally signed package requires an explicit signature policy",
            ));
        }
        if !self.chain_safe
            || (!self.calc_chain_parts.is_empty()
                && self.options.calculation_chain == CalculationChainPolicy::RejectEdits)
        {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Calculation-chain edits are rejected by policy or unsupported incoming relationships",
            ));
        }
        let sheet = self
            .book
            .sheets()
            .iter()
            .position(|info| info.name() == sheet)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet does not exist"))?;
        let info = &self.book.sheets()[sheet];
        if info.kind() != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Selected sheet is not a cell worksheet",
            ));
        }
        if contains_date(value) {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing dates requires an existing style/date catalog",
            )
            .with_cell(address));
        }
        if matches!(value, CellValue::RichText(v) if v.phonetic_properties.is_some()) {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing phonetic font references requires the imported font catalog",
            )
            .with_cell(address));
        }
        let epoch = if self.book.date_1904() {
            DateEpoch::Mac1904
        } else {
            DateEpoch::Windows1900
        };
        crate::encode::validate_non_finite(value, self.options.non_finite)
            .map_err(|error| error.with_cell(address))?;
        validate_value(value, self.options.resources.max_cell_bytes, epoch)
            .map_err(|error| error.with_cell(address))?;
        let key = (address.row.get(), address.column.get());
        let old = self
            .patches
            .get(info.part())
            .and_then(|patches| patches.get(&key));
        let old_bytes = old.map_or(0, |cell| {
            PATCH_BYTES.saturating_add(cell.cell.value.heap_bytes())
        });
        let new_part = !self.patches.contains_key(info.part());
        let bytes = self
            .patch_bytes
            .saturating_sub(old_bytes)
            .saturating_add(PATCH_BYTES)
            .saturating_add(value.heap_bytes())
            .saturating_add(if new_part {
                PATCH_BYTES + info.part().len()
            } else {
                0
            });
        let cells = self.patch_cells + usize::from(old.is_none());
        if bytes > self.options.max_patch_bytes || cells > self.options.max_patch_cells {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Pending cell overlay allowance exceeded",
            )
            .with_cell(address));
        }
        Ok(PatchPlan {
            sheet,
            address,
            bytes,
            cells,
        })
    }
    pub(crate) fn commit_value(&mut self, plan: PatchPlan, value: CellValue, insert_missing: bool) {
        let part = self.book.sheets()[plan.sheet].part();
        let key = (plan.address.row.get(), plan.address.column.get());
        self.patches.entry(part.into()).or_default().insert(
            key,
            Patch {
                cell: Cell {
                    address: plan.address,
                    value,
                    style: StyleId::new(0),
                },
                insert_missing,
            },
        );
        self.patch_bytes = plan.bytes;
        self.patch_cells = plan.cells;
    }
    /// Validate the entire row and its combined overlay charge before mutation.
    pub(crate) fn prepare_row(
        &self,
        sheet: &str,
        row: crabxl_core::RowIndex,
        values: &[CellValue],
        scratch_allowance: usize,
    ) -> Result<RowPatchPlan> {
        if values.len() > crabxl_core::MAX_COLUMNS as usize {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Appended row exceeds column bounds",
            ));
        }
        if values
            .len()
            .saturating_mul(std::mem::size_of::<PatchPlan>())
            > scratch_allowance
        {
            return Err(Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Row edit planning allowance exceeded",
            ));
        }
        let mut plans = Vec::new();
        plans.try_reserve_exact(values.len()).map_err(|cause| {
            Error::caused_by(
                ErrorKind::MemoryBudgetExceeded,
                "Cannot allocate row edit plan",
                cause,
            )
        })?;
        let mut bytes = self.patch_bytes;
        let mut cells = self.patch_cells;
        for (column, value) in values.iter().enumerate() {
            let address = CellAddress::new(row.get(), column as u32)?;
            let mut plan = self.prepare_value(sheet, address, value)?;
            let part = self.book.sheets()[plan.sheet].part();
            let repeated_part = if !plans.is_empty() && !self.patches.contains_key(part) {
                PATCH_BYTES + part.len()
            } else {
                0
            };
            bytes = bytes.saturating_add(
                plan.bytes
                    .saturating_sub(self.patch_bytes)
                    .saturating_sub(repeated_part),
            );
            cells = cells.saturating_add(plan.cells.saturating_sub(self.patch_cells));
            if bytes > self.options.max_patch_bytes || cells > self.options.max_patch_cells {
                return Err(Error::new(
                    ErrorKind::MemoryBudgetExceeded,
                    "Pending row overlay allowance exceeded",
                )
                .with_cell(address));
            }
            plan.bytes = bytes;
            plan.cells = cells;
            plans.push(plan);
        }
        let scratch_bytes = plans
            .capacity()
            .saturating_mul(std::mem::size_of::<PatchPlan>());
        Ok(RowPatchPlan {
            plans,
            bytes,
            scratch_bytes,
        })
    }
    pub(crate) fn commit_row(&mut self, plan: RowPatchPlan, values: Vec<CellValue>) {
        for (patch, value) in plan.plans.into_iter().zip(values) {
            self.commit_value(patch, value, true);
        }
    }
    /// Inspect only a pending replacement, without decoding the original cell.
    pub fn pending_value(&self, sheet: &str, address: CellAddress) -> Option<&CellValue> {
        let part = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)?
            .part();
        self.patches
            .get(part)?
            .get(&(address.row.get(), address.column.get()))
            .map(|patch| &patch.cell.value)
    }
    /// Borrow pending cells for one original sheet in row/column order.
    /// Style IDs here are placeholders; original styles are resolved on save.
    pub fn pending_cells(&self, sheet: &str) -> impl Iterator<Item = &Cell> {
        let part = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)
            .map(SheetInfo::part);
        part.into_iter()
            .flat_map(|part| self.patches.get(part))
            .flat_map(|patches| patches.values())
            .map(|patch| &patch.cell)
    }
    /// Revert all overlays to the original source; releases owned payloads.
    /// This does not adopt a previously saved file as the new source.
    pub fn clear_edits(&mut self) {
        self.patches = BTreeMap::new();
        self.view_patches = BTreeMap::new();
        self.print_patches = BTreeMap::new();
        self.active_patch = None;
        self.visibility_patches = BTreeMap::new();
        self.name_patches = BTreeMap::new();
        self.catalog_order = None;
        self.patch_bytes = 0;
        self.patch_cells = 0;
    }
    /// Save to a caller-owned fresh/truncated sink; failures may leave partial
    /// sink bytes. Caller ownership can be retained by passing &mut W.
    /// Unchanged entries preserve compressed payloads. Value/formula edits rewrite
    /// all worksheets to remove old caches, plus calculation properties; pure
    /// display/printing edits rewrite only the selected worksheets.
    pub fn save<W: Write + Seek>(
        &mut self,
        output: W,
        options: SaveOptions,
    ) -> Result<(W, SaveStats)> {
        crate::writer::validate_compression_level(options.compression_level)?;
        let active = self.active_for_save()?;
        let dirty = self.patch_cells != 0;
        let mut zip = ZipWriter::new(output);
        zip.set_raw_comment(self.book.archive.comment().to_vec().into_boxed_slice())
            .map_err(|error| zip_error("Cannot preserve ZIP archive comment", error))?;
        let mut stats = SaveStats::default();
        let mut total: u128 = self
            .parts
            .iter()
            .filter(|part| !dirty || !self.chain_removals.contains(part.name.as_ref()))
            .map(|part| u128::from(part.uncompressed_bytes))
            .sum();
        for index in 0..self.parts.len() {
            let part = &self.parts[index];
            if dirty && self.chain_removals.contains(part.name.as_ref()) {
                stats.removed_parts += 1;
                continue;
            }
            let chain_metadata = dirty
                && !self.calc_chain_parts.is_empty()
                && (part.name.as_ref() == "[Content_Types].xml"
                    || part.name.as_ref() == self.workbook_relationships);
            let view_patch = self.view_patches.get(part.name.as_ref()).map(Box::as_ref);
            let print_patch = self.print_patches.get(part.name.as_ref()).map(Box::as_ref);
            let worksheet = (dirty || view_patch.is_some() || print_patch.is_some())
                && self.book.sheets().iter().any(|sheet| {
                    sheet.kind() == SheetKind::Worksheet && sheet.part() == part.name.as_ref()
                });
            let workbook = (dirty
                || active.is_some()
                || !self.visibility_patches.is_empty()
                || !self.name_patches.is_empty()
                || self.catalog_order.is_some())
                && part.name.as_ref() == self.book.workbook_part;
            let shared_strings = dirty && self.shared_string_parts.contains(part.name.as_ref());
            if worksheet || workbook || shared_strings || chain_metadata {
                let file = self.book.archive.by_index(index).map_err(|error| {
                    zip_error("Cannot read affected XML part", error).with_part(part.name.as_ref())
                })?;
                // Rewritten parts use the selected level; copied parts keep their bytes.
                zip.start_file(
                    part.name.as_ref(),
                    crate::writer::compression_options(options.compression_level)
                        .large_file(self.options.resources.max_part_bytes >= u64::from(u32::MAX)),
                )
                .map_err(|error| {
                    zip_error("Cannot start affected XML part", error).with_part(part.name.as_ref())
                })?;
                let budget = PartOutput {
                    // Batch XML event fragments before feeding the compressor.
                    // The fixed buffer fits the operation's 64 KiB work reserve.
                    inner: BufWriter::with_capacity(64 * 1024, &mut zip),
                    bytes: 0,
                    maximum: self.options.resources.max_part_bytes,
                };
                let written = if worksheet {
                    patch_worksheet(
                        file,
                        budget,
                        &part.name,
                        WorksheetRewrite {
                            patches: self.patches.get(part.name.as_ref()),
                            limits: self.options.resources,
                            formula_attributes: self.options.formula_attributes,
                            views: view_patch,
                            printing: print_patch,
                            invalidate_caches: dirty,
                        },
                    )
                } else if workbook {
                    patch_workbook(
                        file,
                        budget,
                        &part.name,
                        WorkbookRewrite {
                            limits: self.options.resources,
                            invalidate_caches: dirty,
                            active,
                            visibility: &self.visibility_patches,
                            names: &self.name_patches,
                            order: self.catalog_order.as_deref(),
                        },
                    )
                } else if shared_strings {
                    patch_shared_strings(file, budget, &part.name, self.options.resources)
                } else {
                    patch_chain_metadata(
                        file,
                        budget,
                        &part.name,
                        &self.calc_chain_parts,
                        self.options.resources,
                    )
                }
                .map_err(|error| error.with_part(part.name.as_ref()))?;
                total = total - u128::from(part.uncompressed_bytes) + u128::from(written);
                if total > u128::from(self.options.resources.max_total_uncompressed_bytes) {
                    return Err(limit("Edited package uncompressed byte limit exceeded"));
                }
                stats.rewritten_parts += 1;
                stats.rewritten_xml_bytes += written;
            } else {
                if options.verify_unchanged {
                    let file = self
                        .book
                        .archive
                        .by_index(index)
                        .map_err(|error| zip_error("Cannot validate unchanged part", error))?;
                    let bytes = io::copy(
                        &mut file.take(self.options.resources.max_part_bytes.saturating_add(1)),
                        &mut io::sink(),
                    )
                    .map_err(|error| {
                        io_error("Cannot validate unchanged part CRC", error)
                            .with_part(part.name.as_ref())
                    })?;
                    if bytes > self.options.resources.max_part_bytes {
                        return Err(limit("Unchanged part byte limit exceeded")
                            .with_part(part.name.as_ref()));
                    }
                }
                let file = self
                    .book
                    .archive
                    .by_index(index)
                    .map_err(|error| zip_error("Cannot reopen original part", error))?;
                zip.raw_copy_file(file).map_err(|error| {
                    zip_error("Cannot copy original compressed part", error)
                        .with_part(part.name.as_ref())
                })?;
                stats.copied_parts += 1;
            }
        }
        let mut output = zip
            .finish()
            .map_err(|error| zip_error("Cannot finish edited package", error))?;
        output
            .flush()
            .map_err(|error| io_error("Cannot flush edited package", error))?;
        if let Some(active) = active {
            self.active_patch = Some(match self.active_patch {
                Some(ActivePatch::Deferred(_)) => ActivePatch::Deferred(active.requested_index),
                _ => ActivePatch::Visible(active.requested_index as usize),
            });
        }
        Ok((output, stats))
    }
    /// Write a fresh adjacent temporary output and replace the target only after
    /// successful ZIP completion. Supports saving over the original path without
    /// truncating its live source. Failed saves leave the target unchanged.
    /// No crash-durability or cross-platform replacement guarantee is implied.
    pub fn save_path(&mut self, path: impl AsRef<Path>, options: SaveOptions) -> Result<SaveStats> {
        let path = path.as_ref();
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temporary = tempfile::Builder::new()
            .prefix("crabxl-save-")
            .tempfile_in(parent)
            .map_err(|error| io_error("Cannot create adjacent output temporary file", error))?;
        let (_, stats) = self.save(&mut temporary, options)?;
        // std::fs::rename supports Windows replacement with a live source handle.
        // Keep the temporary path guarded so failures still remove the output.
        let temporary = temporary.into_temp_path();
        std::fs::rename(&temporary, path)
            .map_err(|error| io_error("Cannot replace edited workbook target", error))?;
        Ok(stats)
    }
    /// Release this editor and return its original owned source.
    pub fn into_source(self) -> R {
        self.book.into_inner()
    }
}
fn check_printer_identity(
    original: &crabxl_core::PageSetup,
    replacement: &crabxl_core::PageSetup,
) -> Result<()> {
    if original.printer_relationship != replacement.printer_relationship {
        return Err(Error::new(
            ErrorKind::Unsupported,
            "Changing printer identities requires a package feature graph",
        ));
    }
    Ok(())
}
fn contains_date(value: &CellValue) -> bool {
    match value {
        CellValue::DateTime(_) => true,
        CellValue::Formula(formula) => formula.cached().is_some_and(contains_date),
        _ => false,
    }
}
fn limit(message: &str) -> Error {
    Error::new(ErrorKind::LimitExceeded, message)
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::InvalidData, message)
}
struct PartOutput<W> {
    inner: W,
    bytes: u64,
    maximum: u64,
}
impl<W: Write> Write for PartOutput<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.len() as u64 > self.maximum.saturating_sub(self.bytes) {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "Rewritten XML part byte limit exceeded",
            ));
        }
        let count = self.inner.write(data)?;
        self.bytes += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
fn emit<W: Write>(writer: &mut Writer<PartOutput<W>>, event: Event<'_>) -> Result<()> {
    writer.write_event(event).map_err(|error| {
        Error::caused_by(
            if error.kind() == io::ErrorKind::FileTooLarge {
                ErrorKind::LimitExceeded
            } else {
                ErrorKind::Io
            },
            "Cannot write preserved XML event",
            error,
        )
    })
}
fn check_declaration(event: &Event<'_>) -> Result<()> {
    if let Event::Decl(declaration) = event
        && let Some(encoding) = declaration.encoding()
    {
        let encoding = encoding.map_err(|error| {
            Error::caused_by(ErrorKind::Xml, "Invalid XML encoding declaration", error)
        })?;
        if !encoding.eq_ignore_ascii_case("UTF-8") {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Rewriting non-UTF-8 XML is not supported",
            ));
        }
    }
    Ok(())
}
fn unsigned_attribute(e: &BytesStart<'_>, name: &[u8]) -> Result<Option<u32>> {
    let Some(attribute) = e
        .try_get_attribute(
            std::str::from_utf8(name)
                .map_err(|e| Error::caused_by(ErrorKind::Xml, "Invalid attribute name", e))?,
        )
        .map_err(|error| Error::caused_by(ErrorKind::Xml, "Invalid position attribute", error))?
    else {
        return Ok(None);
    };
    let value = attribute
        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
        .map_err(|error| Error::caused_by(ErrorKind::Xml, "Invalid position value", error))?;
    value.parse().map(Some).map_err(|error| {
        Error::caused_by(ErrorKind::InvalidData, "Invalid position integer", error)
    })
}
fn patched_start(e: &BytesStart<'_>, uri: &str, value: &CellValue) -> Result<BytesStart<'static>> {
    let mut start = e.to_owned();
    start.clear_attributes();
    for attribute in e.attributes() {
        let attribute = attribute
            .map_err(|error| Error::caused_by(ErrorKind::Xml, "Invalid cell attribute", error))?;
        if !matches!(
            attribute.key.as_ref().as_bytes(),
            b"t" | b"xmlns" | b"cm" | b"vm"
        ) {
            start.push_attribute(attribute);
        }
    }
    start.push_attribute(("xmlns", uri));
    let literal = match value {
        CellValue::Formula(formula) => formula.cached(),
        value => Some(value),
    };
    match literal {
        Some(CellValue::Text(_) | CellValue::RichText(_)) => start.push_attribute((
            "t",
            if matches!(value, CellValue::Formula(_)) {
                "str"
            } else {
                "inlineStr"
            },
        )),
        Some(CellValue::Boolean(_)) => start.push_attribute(("t", "b")),
        Some(CellValue::Error(_)) => start.push_attribute(("t", "e")),
        _ => {}
    }
    Ok(start)
}
fn write_body<W: Write>(
    writer: &mut Writer<PartOutput<W>>,
    cell: &Cell,
    buffer: &mut RowBuffer,
    limits: ResourceLimits,
    formula_attributes: crate::FormulaWritePolicy,
) -> Result<()> {
    encode_cells(
        buffer,
        cell.address.row,
        std::slice::from_ref(cell).iter(),
        limits.max_cell_bytes,
        1,
        StyleContext::Appearance(&[CellStyle::default()]),
        ValueEncoding {
            epoch: DateEpoch::Windows1900,
            iso_dates: false,
            non_finite: crate::NonFiniteWritePolicy::Blank,
            formula_attributes,
            date_styles: crate::encode::DateStyleIds {
                datetime: crabxl_core::StyleId::new(1),
                time: crabxl_core::StyleId::new(2),
                duration: crabxl_core::StyleId::new(3),
                date: crabxl_core::StyleId::new(4),
            },
        },
    )
    .map_err(|error| error.with_cell(cell.address))?;
    // Reuse the shared cell body under a namespace-aware original/new header.
    let begin = buffer
        .data
        .iter()
        .position(|byte| *byte == b'>')
        .and_then(|row_end| {
            buffer.data[row_end + 1..]
                .iter()
                .position(|byte| *byte == b'>')
                .map(|cell_end| row_end + cell_end + 2)
        })
        .ok_or_else(|| invalid("Encoded cell header is missing"))?;
    let end = buffer
        .data
        .len()
        .checked_sub(b"</c></row>".len())
        .ok_or_else(|| invalid("Encoded cell footer is missing"))?;
    writer
        .get_mut()
        .write_all(&buffer.data[begin..end])
        .map_err(|error| {
            Error::caused_by(
                if error.kind() == io::ErrorKind::FileTooLarge {
                    ErrorKind::LimitExceeded
                } else {
                    ErrorKind::Io
                },
                "Cannot write replacement cell body",
                error,
            )
        })
}
fn write_inserted_cell<W: Write>(
    writer: &mut Writer<PartOutput<W>>,
    patch: &Patch,
    uri: &str,
    buffer: &mut RowBuffer,
    limits: ResourceLimits,
    formula_attributes: crate::FormulaWritePolicy,
) -> Result<()> {
    if !patch.insert_missing {
        return Err(
            invalid("Pending replacement targets a missing physical cell")
                .with_cell(patch.cell.address),
        );
    }
    let mut base = BytesStart::new("c");
    let reference = patch.cell.address.to_string();
    base.push_attribute(("r", reference.as_str()));
    let start = patched_start(&base, uri, &patch.cell.value)?;
    emit(writer, Event::Start(start))?;
    write_body(writer, &patch.cell, buffer, limits, formula_attributes)?;
    emit(writer, Event::End(quick_xml::events::BytesEnd::new("c")))
}
fn positioned_start(
    e: &BytesStart<'_>,
    position: &str,
    omit_spans: bool,
) -> Result<BytesStart<'static>> {
    let mut start = e.to_owned();
    start.clear_attributes();
    for attribute in e.attributes() {
        let attribute = attribute.map_err(|error| {
            Error::caused_by(ErrorKind::Xml, "Invalid coordinate attribute", error)
        })?;
        if attribute.key.as_ref().as_bytes() != b"r"
            && !(omit_spans && attribute.key.as_ref().as_bytes() == b"spans")
        {
            start.push_attribute(attribute);
        }
    }
    start.push_attribute(("r", position));
    Ok(start)
}
fn expanded_dimension(e: &BytesStart<'_>, patches: &Patches) -> Result<BytesStart<'static>> {
    let reference =
        attribute(e, b"ref")?.ok_or_else(|| invalid("Worksheet dimension has no reference"))?;
    let (first, last) = reference
        .split_once(':')
        .unwrap_or((&reference, &reference));
    let first: CellAddress = first.parse()?;
    let last: CellAddress = last.parse()?;
    let mut low = (first.row.get(), first.column.get());
    let mut high = (last.row.get(), last.column.get());
    if low.0 > high.0 || low.1 > high.1 {
        return Err(invalid("Worksheet dimension is reversed"));
    }
    for patch in patches.values().filter(|patch| patch.insert_missing) {
        let address = patch.cell.address;
        low.0 = low.0.min(address.row.get());
        low.1 = low.1.min(address.column.get());
        high.0 = high.0.max(address.row.get());
        high.1 = high.1.max(address.column.get());
    }
    let reference = format!(
        "{}:{}",
        CellAddress::new(low.0, low.1)?,
        CellAddress::new(high.0, high.1)?
    );
    let mut start = e.to_owned();
    start.clear_attributes();
    for attribute in e.attributes() {
        let attribute = attribute.map_err(|error| {
            Error::caused_by(ErrorKind::Xml, "Invalid dimension attribute", error)
        })?;
        if attribute.key.as_ref().as_bytes() != b"ref" {
            start.push_attribute(attribute);
        }
    }
    start.push_attribute(("ref", reference.as_str()));
    Ok(start)
}
struct WorksheetRewrite<'a> {
    patches: Option<&'a Patches>,
    limits: ResourceLimits,
    formula_attributes: crate::FormulaWritePolicy,
    views: Option<&'a crabxl_core::SheetViews>,
    printing: Option<&'a crabxl_core::PrintSettings>,
    invalidate_caches: bool,
}
fn patch_worksheet<R: Read + Seek, W: Write>(
    input: zip::read::ZipFile<'_, R>,
    output: PartOutput<W>,
    part: &str,
    rewrite: WorksheetRewrite<'_>,
) -> Result<u64> {
    let WorksheetRewrite {
        patches,
        limits,
        formula_attributes,
        views,
        printing,
        invalidate_caches,
    } = rewrite;
    let mut xml = XmlStream::new(
        BufReader::with_capacity(limits.input_buffer_bytes, input),
        part.into(),
        limits.max_part_bytes,
        limits,
    );
    let mut writer = Writer::new(output);
    let mut print_rewrite = printing.map(crate::printing::Rewrite::new);
    let mut row = 0u32;
    let mut next_row = 0u32;
    let mut next_column = 0u32;
    let mut last_row = None;
    let mut selected_row = false;
    let mut pending = patches
        .into_iter()
        .flat_map(|patches| patches.values())
        .peekable();
    let mut data_uri = None;
    let mut row_tail = false;
    let mut seen_data = false;
    let mut found = 0usize;
    let mut in_data = false;
    let mut in_row = false;
    let mut in_cell = false;
    let mut views_written = false;
    let mut skipped_views = false;
    let mut formula = false;
    let mut seen_v = false;
    let mut buffer = RowBuffer {
        data: Vec::new(),
        maximum: limits.max_row_bytes,
    };
    loop {
        let frame = xml.next()?;
        check_declaration(&frame.event)?;
        if matches!(&frame.event,Event::Start(e) if e.local_name().as_ref().as_bytes()==b"AlternateContent")
        {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing markup-compatibility alternatives requires typed branch handling",
            ));
        }
        if let Some(rewrite) = &mut print_rewrite
            && frame.scope == Scope::Spreadsheet
        {
            match &frame.event {
                Event::Start(e) => {
                    let skip = rewrite
                        .before_start(
                            writer.get_mut(),
                            e.local_name().as_ref().as_bytes(),
                            frame.depth,
                            frame.spreadsheet_uri,
                        )
                        .map_err(|error| io_error("Cannot replace printing metadata", error))?;
                    if skip {
                        let depth = frame.depth;
                        crate::style_codec::skip(&mut xml, depth)?;
                        continue;
                    }
                }
                Event::End(e) => rewrite
                    .before_end(
                        writer.get_mut(),
                        e.local_name().as_ref().as_bytes(),
                        frame.depth,
                        frame.spreadsheet_uri,
                    )
                    .map_err(|error| io_error("Cannot finish printing metadata", error))?,
                _ => {}
            }
        }
        if let Some(views) = views
            && let Event::Start(e) = &frame.event
            && frame.depth == 2
            && frame.scope == Scope::Spreadsheet
        {
            let name = e.local_name();
            if !views_written && !matches!(name.as_ref().as_bytes(), b"sheetPr" | b"dimension") {
                crate::worksheet_view::write_views(writer.get_mut(), views, frame.spreadsheet_uri)
                    .map_err(|error| io_error("Cannot write worksheet views", error))?;
                views_written = true;
            }
            if name.as_ref().as_bytes() == b"sheetViews" {
                if skipped_views {
                    return Err(invalid("Duplicate worksheet views container"));
                }
                skipped_views = true;
                loop {
                    let old = xml.next()?;
                    if matches!(&old.event, Event::End(e) if old.depth == 1 && e.local_name().as_ref().as_bytes() == b"sheetViews")
                    {
                        break;
                    }
                    if matches!(old.event, Event::Eof) {
                        return Err(invalid("Incomplete replaced worksheet views"));
                    }
                }
                continue;
            }
        }
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                if frame.scope != Scope::Spreadsheet
                    || e.local_name().as_ref().as_bytes() != b"worksheet"
                {
                    return Err(invalid("Affected part is not a worksheet"));
                }
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"dimension"
                    && patches.is_some() =>
            {
                let start =
                    expanded_dimension(&e, patches.ok_or_else(|| invalid("Missing overlays"))?)?;
                emit(&mut writer, Event::Start(start))?;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref().as_bytes() == b"mergeCell"
                    && patches.is_some() =>
            {
                let reference = attribute(&e, b"ref")?
                    .ok_or_else(|| invalid("Merged range has no reference"))?;
                let (first, last) = reference
                    .split_once(':')
                    .unwrap_or((&reference, &reference));
                let start: CellAddress = first.parse()?;
                let end: CellAddress = last.parse()?;
                let range = crabxl_core::CellRange::new(start, end)?;
                for patch in patches.into_iter().flat_map(|patches| {
                    patches
                        .range((start.row.get(), 0)..=(end.row.get(), u32::MAX))
                        .map(|(_, patch)| patch)
                }) {
                    if range.contains(patch.cell.address) && patch.cell.address != start {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            "Editing a non-anchor merged cell is not supported",
                        )
                        .with_cell(patch.cell.address));
                    }
                }
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"sheetData" =>
            {
                if seen_data {
                    return Err(invalid("Duplicate sheetData in affected worksheet"));
                }
                seen_data = true;
                in_data = true;
                data_uri = frame.spreadsheet_uri;
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if in_data
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref().as_bytes() == b"row" =>
            {
                in_row = true;
                row = unsigned_attribute(&e, b"r")?
                    .map(|value| {
                        value
                            .checked_sub(1)
                            .ok_or_else(|| invalid("Invalid row index"))
                    })
                    .transpose()?
                    .unwrap_or(next_row);
                crabxl_core::RowIndex::new(row)?;
                while pending
                    .peek()
                    .is_some_and(|patch| patch.cell.address.row.get() < row)
                {
                    let inserted_row = pending
                        .peek()
                        .ok_or_else(|| invalid("Missing pending row"))?
                        .cell
                        .address
                        .row
                        .get();
                    let uri = data_uri.ok_or_else(|| invalid("Worksheet namespace is missing"))?;
                    let mut start = BytesStart::new("row");
                    let reference = (inserted_row + 1).to_string();
                    start.push_attribute(("r", reference.as_str()));
                    start.push_attribute(("xmlns", uri));
                    emit(&mut writer, Event::Start(start))?;
                    while pending
                        .peek()
                        .is_some_and(|patch| patch.cell.address.row.get() == inserted_row)
                    {
                        let patch = pending
                            .next()
                            .ok_or_else(|| invalid("Missing pending cell"))?;
                        write_inserted_cell(
                            &mut writer,
                            patch,
                            uri,
                            &mut buffer,
                            limits,
                            formula_attributes,
                        )?;
                        found += 1;
                    }
                    emit(
                        &mut writer,
                        Event::End(quick_xml::events::BytesEnd::new("row")),
                    )?;
                }
                row_tail = false;
                if last_row.is_some_and(|last| row <= last) {
                    return Err(invalid("Affected worksheet rows are not ordered"));
                }
                last_row = Some(row);
                next_row = row + 1;
                next_column = 0;
                selected_row = patches.is_some_and(|patches| {
                    patches.range((row, 0)..=(row, u32::MAX)).next().is_some()
                });
                let start = positioned_start(&e, &(row + 1).to_string(), selected_row)?;
                emit(&mut writer, Event::Start(start))?;
            }
            Event::Start(e)
                if in_row
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 4
                    && e.local_name().as_ref().as_bytes() == b"extLst" =>
            {
                row_tail = true;
                while pending
                    .peek()
                    .is_some_and(|patch| patch.cell.address.row.get() == row)
                {
                    let patch = pending
                        .next()
                        .ok_or_else(|| invalid("Missing pending cell"))?;
                    let uri = data_uri.ok_or_else(|| invalid("Worksheet namespace is missing"))?;
                    write_inserted_cell(
                        &mut writer,
                        patch,
                        uri,
                        &mut buffer,
                        limits,
                        formula_attributes,
                    )?;
                    found += 1;
                }
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if in_row
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 4
                    && e.local_name().as_ref().as_bytes() == b"c" =>
            {
                if row_tail {
                    return Err(invalid("Cell follows row extension list"));
                }
                in_cell = true;
                formula = false;
                seen_v = false;
                let replacement = if selected_row {
                    let address = attribute(&e, b"r")?
                        .map(|value| value.parse::<CellAddress>())
                        .transpose()?
                        .unwrap_or(CellAddress::new(row, next_column)?);
                    if address.row.get() != row || address.column.get() < next_column {
                        return Err(
                            invalid("Affected cell coordinates are not ordered").with_cell(address)
                        );
                    }
                    while pending.peek().is_some_and(|patch| {
                        patch.cell.address.row.get() == row
                            && patch.cell.address.column.get() < address.column.get()
                    }) {
                        let patch = pending
                            .next()
                            .ok_or_else(|| invalid("Missing pending cell"))?;
                        let uri =
                            data_uri.ok_or_else(|| invalid("Worksheet namespace is missing"))?;
                        write_inserted_cell(
                            &mut writer,
                            patch,
                            uri,
                            &mut buffer,
                            limits,
                            formula_attributes,
                        )?;
                        found += 1;
                    }
                    next_column = address.column.get() + 1;
                    if pending
                        .peek()
                        .is_some_and(|patch| patch.cell.address == address)
                    {
                        pending.next().map(|patch| &patch.cell)
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(cell) = replacement {
                    let uri = frame
                        .spreadsheet_uri
                        .ok_or_else(|| invalid("Affected cell namespace is missing"))?;
                    let positioned = positioned_start(&e, &cell.address.to_string(), false)?;
                    let start = patched_start(&positioned, uri, &cell.value)
                        .map_err(|error| error.with_cell(cell.address))?;
                    let name = start.name().as_ref().as_bytes().to_vec();
                    // Validate the old cell before replacing its body.
                    loop {
                        let old = xml.next()?;
                        match &old.event {
                            Event::End(end)
                                if old.depth == 3
                                    && end.local_name().as_ref().as_bytes() == b"c" =>
                            {
                                break;
                            }
                            Event::Start(child)
                                if old.scope == Scope::Spreadsheet
                                    && old.depth == 5
                                    && child.local_name().as_ref().as_bytes() == b"is" =>
                            {
                                crate::rich_text::read_container(
                                    &mut xml,
                                    5,
                                    b"is",
                                    limits.max_cell_bytes,
                                    true,
                                )
                                .map_err(|e| e.with_cell(cell.address))?;
                            }
                            Event::Start(child) => {
                                if old.scope != Scope::Spreadsheet
                                    || !matches!(
                                        child.local_name().as_ref().as_bytes(),
                                        b"v" | b"is" | b"t" | b"f"
                                    )
                                {
                                    return Err(Error::new(ErrorKind::Unsupported,"Replacing unknown or rich cell content requires typed support").with_cell(cell.address));
                                }
                                if child.local_name().as_ref().as_bytes() == b"f" {
                                    let metadata = crate::formula_codec::header(
                                        child,
                                        limits.max_cell_bytes,
                                        crate::formula_codec::HeaderPolicy::KnownRecords,
                                    )
                                    .map_err(|error| error.with_cell(cell.address))?;
                                    if matches!(
                                        metadata.kind,
                                        crabxl_core::FormulaType::Shared { .. }
                                    ) {
                                        return Err(Error::new(ErrorKind::Unsupported, "Replacing shared formula records requires group normalization").with_cell(cell.address));
                                    }
                                }
                            }
                            Event::Eof => return Err(invalid("Unexpected end of replaced cell")),
                            _ => {}
                        }
                    }
                    emit(&mut writer, Event::Start(start))?;
                    write_body(&mut writer, cell, &mut buffer, limits, formula_attributes)?;
                    let name = std::str::from_utf8(&name).map_err(|error| {
                        Error::caused_by(ErrorKind::Xml, "Invalid cell name", error)
                    })?;
                    emit(
                        &mut writer,
                        Event::End(quick_xml::events::BytesEnd::new(name)),
                    )?;
                    in_cell = false;
                    found += 1;
                } else if selected_row {
                    let address = CellAddress::new(row, next_column - 1)?;
                    let start = positioned_start(&e, &address.to_string(), false)?;
                    emit(&mut writer, Event::Start(start))?;
                } else {
                    emit(&mut writer, Event::Start(e))?;
                }
            }
            Event::Start(e)
                if in_cell
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 5
                    && e.local_name().as_ref().as_bytes() == b"f" =>
            {
                if seen_v {
                    return Err(invalid("Formula follows its cached value"));
                }
                formula = true;
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if in_cell
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 5
                    && e.local_name().as_ref().as_bytes() == b"v" =>
            {
                seen_v = true;
                if formula && invalidate_caches {
                    loop {
                        let frame = xml.next()?;
                        if matches!(&frame.event,Event::End(end) if frame.depth==4 && end.local_name().as_ref().as_bytes()==b"v")
                        {
                            break;
                        }
                        if matches!(frame.event, Event::Eof) {
                            return Err(invalid("Unexpected end of formula cache"));
                        }
                    }
                } else {
                    emit(&mut writer, Event::Start(e))?;
                }
            }
            Event::End(e)
                if in_cell
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref().as_bytes() == b"c" =>
            {
                in_cell = false;
                emit(&mut writer, Event::End(e))?;
            }
            Event::End(e)
                if in_row
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"row" =>
            {
                while pending
                    .peek()
                    .is_some_and(|patch| patch.cell.address.row.get() == row)
                {
                    let patch = pending
                        .next()
                        .ok_or_else(|| invalid("Missing pending cell"))?;
                    let uri = data_uri.ok_or_else(|| invalid("Worksheet namespace is missing"))?;
                    write_inserted_cell(
                        &mut writer,
                        patch,
                        uri,
                        &mut buffer,
                        limits,
                        formula_attributes,
                    )?;
                    found += 1;
                }
                in_row = false;
                emit(&mut writer, Event::End(e))?;
            }
            Event::End(e)
                if in_data
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref().as_bytes() == b"sheetData" =>
            {
                while let Some(patch) = pending.peek() {
                    let inserted_row = patch.cell.address.row.get();
                    let uri = data_uri.ok_or_else(|| invalid("Worksheet namespace is missing"))?;
                    let mut start = BytesStart::new("row");
                    let reference = (inserted_row + 1).to_string();
                    start.push_attribute(("r", reference.as_str()));
                    start.push_attribute(("xmlns", uri));
                    emit(&mut writer, Event::Start(start))?;
                    while pending
                        .peek()
                        .is_some_and(|patch| patch.cell.address.row.get() == inserted_row)
                    {
                        let patch = pending
                            .next()
                            .ok_or_else(|| invalid("Missing pending cell"))?;
                        write_inserted_cell(
                            &mut writer,
                            patch,
                            uri,
                            &mut buffer,
                            limits,
                            formula_attributes,
                        )?;
                        found += 1;
                    }
                    emit(
                        &mut writer,
                        Event::End(quick_xml::events::BytesEnd::new("row")),
                    )?;
                }
                in_data = false;
                emit(&mut writer, Event::End(e))?;
            }
            Event::Eof => break,
            event => emit(&mut writer, event)?,
        }
    }
    if !seen_data {
        return Err(invalid("Affected worksheet has no sheetData"));
    }
    if patches.is_some_and(|patches| found != patches.len()) {
        return Err(invalid(
            "Pending replacement targets a missing physical cell",
        ));
    }
    let mut output = writer.into_inner();
    output
        .flush()
        .map_err(|error| io_error("Cannot flush rewritten XML part", error))?;
    Ok(output.bytes)
}
struct WorkbookRewrite<'a> {
    limits: ResourceLimits,
    invalidate_caches: bool,
    active: Option<crabxl_core::ActiveViewSelection>,
    visibility: &'a BTreeMap<usize, crabxl_core::SheetVisibility>,
    names: &'a BTreeMap<usize, Box<str>>,
    order: Option<&'a CatalogOrder>,
}
fn rewritten_catalog_entry(
    original: &BytesStart<'_>,
    name: Option<&str>,
    state: Option<crabxl_core::SheetVisibility>,
) -> Result<BytesStart<'static>> {
    let mut start = original.to_owned();
    start.clear_attributes();
    for attribute in original.attributes() {
        let attribute = attribute.map_err(|cause| {
            Error::caused_by(ErrorKind::Xml, "Invalid sheet catalog attribute", cause)
        })?;
        if !(state.is_some() && attribute.key.as_ref().as_bytes() == b"state"
            || name.is_some() && attribute.key.as_ref().as_bytes() == b"name")
        {
            start.push_attribute(attribute);
        }
    }
    if let Some(state) = state {
        start.push_attribute(("state", state.as_str()));
    }
    if let Some(name) = name {
        start.push_attribute(("name", name));
    }
    Ok(start)
}
fn patch_workbook<R: Read + Seek, W: Write>(
    input: zip::read::ZipFile<'_, R>,
    output: PartOutput<W>,
    part: &str,
    rewrite: WorkbookRewrite<'_>,
) -> Result<u64> {
    let WorkbookRewrite {
        limits,
        invalidate_caches,
        active,
        visibility,
        names,
        order,
    } = rewrite;
    let mut xml = XmlStream::new(
        BufReader::with_capacity(limits.input_buffer_bytes, input),
        part.into(),
        limits.max_part_bytes,
        limits,
    );
    let mut writer = Writer::new(output);
    let mut seen = false;
    let mut uri = None;
    let mut views_seen = false;
    let mut views_open = false;
    let mut active_written = false;
    let mut sheet_index = 0usize;
    let mut sheets_open = false;
    let mut skipped_sheet = false;
    loop {
        let frame = xml.next()?;
        check_declaration(&frame.event)?;
        if skipped_sheet {
            match &frame.event {
                Event::End(_) if frame.depth == 2 => skipped_sheet = false,
                Event::Text(text)
                    if text.as_ref().as_bytes().iter().all(u8::is_ascii_whitespace) => {}
                _ => {
                    return Err(Error::new(
                        ErrorKind::Unsupported,
                        "Nested sheet catalog content requires typed reorder handling",
                    ));
                }
            }
            continue;
        }
        if frame.scope == Scope::Spreadsheet {
            match &frame.event {
                Event::Start(e)
                    if frame.depth == 2 && e.local_name().as_ref().as_bytes() == b"sheets" =>
                {
                    sheets_open = true
                }
                Event::End(e)
                    if frame.depth == 1 && e.local_name().as_ref().as_bytes() == b"sheets" =>
                {
                    sheets_open = false
                }
                _ => {}
            }
        }
        if matches!(&frame.event,Event::Start(e) if e.local_name().as_ref().as_bytes()==b"AlternateContent")
        {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing markup-compatibility alternatives requires typed branch handling",
            ));
        }
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                if frame.scope != Scope::Spreadsheet
                    || e.local_name().as_ref().as_bytes() != b"workbook"
                {
                    return Err(invalid("Affected part is not a workbook"));
                }
                uri = frame.spreadsheet_uri;
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if sheets_open
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref().as_bytes() == b"sheet" =>
            {
                if order.is_some() {
                    sheet_index += 1;
                    skipped_sheet = true;
                    continue;
                }
                let state = visibility.get(&sheet_index);
                let name = names.get(&sheet_index);
                sheet_index += 1;
                if state.is_some() || name.is_some() {
                    let start =
                        rewritten_catalog_entry(&e, name.map(AsRef::as_ref), state.copied())?;
                    emit(&mut writer, Event::Start(start))?;
                } else {
                    emit(&mut writer, Event::Start(e))?;
                }
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref().as_bytes() == b"sheets"
                    && order.is_some() =>
            {
                if let Some(order) = order {
                    if sheet_index != order.entries.len() {
                        return Err(invalid("Original sheet catalog changed"));
                    }
                    for &source in &order.positions {
                        let original = order
                            .entries
                            .get(source)
                            .ok_or_else(|| invalid("Original sheet order is inconsistent"))?;
                        let state = visibility.get(&source).copied();
                        let name = names.get(&source).map(AsRef::as_ref);
                        if state.is_some() || name.is_some() {
                            let start = rewritten_catalog_entry(original, name, state)?;
                            emit(&mut writer, Event::Empty(start))?;
                        } else {
                            emit(&mut writer, Event::Empty(original.borrow()))?;
                        }
                    }
                }
                emit(&mut writer, Event::End(e))?;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"bookViews" =>
            {
                views_seen = true;
                views_open = true;
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if active.is_some()
                    && views_open
                    && !active_written
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref().as_bytes() == b"workbookView" =>
            {
                let mut start = e.to_owned();
                start.clear_attributes();
                for attribute in e.attributes() {
                    let attribute = attribute.map_err(|error| {
                        Error::caused_by(ErrorKind::Xml, "Invalid workbook view attribute", error)
                    })?;
                    if attribute.key.as_ref().as_bytes() != b"activeTab" {
                        start.push_attribute(attribute);
                    }
                }
                let value = active
                    .and_then(|view| view.serialized_index)
                    .map(|index| index.to_string());
                if let Some(value) = &value {
                    start.push_attribute(("activeTab", value.as_str()));
                }
                emit(&mut writer, Event::Start(start))?;
                active_written = true;
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref().as_bytes() == b"bookViews" =>
            {
                if active.is_some() && !active_written {
                    emit_active_view(&mut writer, uri, active)?;
                    active_written = true;
                }
                views_open = false;
                emit(&mut writer, Event::End(e))?;
            }
            Event::Start(e)
                if active.is_some()
                    && !views_seen
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"sheets" =>
            {
                let mut start = BytesStart::new("bookViews");
                start.push_attribute((
                    "xmlns",
                    uri.ok_or_else(|| invalid("Workbook namespace is missing"))?,
                ));
                emit(&mut writer, Event::Start(start))?;
                emit_active_view(&mut writer, uri, active)?;
                emit(
                    &mut writer,
                    Event::End(quick_xml::events::BytesEnd::new("bookViews")),
                )?;
                views_seen = true;
                active_written = true;
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if invalidate_caches
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref().as_bytes() == b"calcPr" =>
            {
                if seen {
                    return Err(invalid("Duplicate calculation properties"));
                }
                seen = true;
                let mut start = e.to_owned();
                start.clear_attributes();
                for attribute in e.attributes() {
                    let attribute = attribute.map_err(|error| {
                        Error::caused_by(ErrorKind::Xml, "Invalid calculation attribute", error)
                    })?;
                    if !matches!(
                        attribute.key.as_ref().as_bytes(),
                        b"calcMode" | b"fullCalcOnLoad" | b"forceFullCalc"
                    ) {
                        start.push_attribute(attribute);
                    }
                }
                start.push_attribute(("calcMode", "auto"));
                start.push_attribute(("fullCalcOnLoad", "1"));
                start.push_attribute(("forceFullCalc", "1"));
                emit(&mut writer, Event::Start(start))?;
            }
            Event::Start(e)
                if invalidate_caches
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && matches!(
                        e.local_name().as_ref().as_bytes(),
                        b"oleSize"
                            | b"customWorkbookViews"
                            | b"pivotCaches"
                            | b"smartTagPr"
                            | b"smartTagTypes"
                            | b"webPublishing"
                            | b"fileRecoveryPr"
                            | b"webPublishObjects"
                            | b"extLst"
                    ) =>
            {
                if !seen {
                    emit_calculation(&mut writer, uri)?;
                    seen = true;
                }
                emit(&mut writer, Event::Start(e))?;
            }
            Event::End(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 0
                    && e.local_name().as_ref().as_bytes() == b"workbook" =>
            {
                if invalidate_caches && !seen {
                    emit_calculation(&mut writer, uri)?;
                    seen = true;
                }
                emit(&mut writer, Event::End(e))?;
            }
            Event::Eof => break,
            event => emit(&mut writer, event)?,
        }
    }
    if invalidate_caches && !seen {
        return Err(invalid("Workbook calculation properties were not written"));
    }
    if active.is_some() && !active_written {
        return Err(invalid("Workbook active view was not written"));
    }
    let mut output = writer.into_inner();
    output
        .flush()
        .map_err(|error| io_error("Cannot flush rewritten XML part", error))?;
    Ok(output.bytes)
}
fn emit_active_view<W: Write>(
    writer: &mut Writer<PartOutput<W>>,
    uri: Option<&str>,
    active: Option<crabxl_core::ActiveViewSelection>,
) -> Result<()> {
    let mut start = BytesStart::new("workbookView");
    start.push_attribute((
        "xmlns",
        uri.ok_or_else(|| invalid("Workbook namespace is missing"))?,
    ));
    let value = active
        .and_then(|view| view.serialized_index)
        .map(|index| index.to_string());
    if let Some(value) = &value {
        start.push_attribute(("activeTab", value.as_str()));
    }
    emit(writer, Event::Empty(start))
}
fn emit_calculation<W: Write>(writer: &mut Writer<PartOutput<W>>, uri: Option<&str>) -> Result<()> {
    let mut start = BytesStart::new("calcPr");
    start.push_attribute((
        "xmlns",
        uri.ok_or_else(|| invalid("Workbook namespace is missing"))?,
    ));
    start.push_attribute(("calcMode", "auto"));
    start.push_attribute(("fullCalcOnLoad", "1"));
    start.push_attribute(("forceFullCalc", "1"));
    emit(writer, Event::Empty(start))
}

fn patch_shared_strings<R: Read + Seek, W: Write>(
    input: zip::read::ZipFile<'_, R>,
    output: PartOutput<W>,
    part: &str,
    limits: ResourceLimits,
) -> Result<u64> {
    let mut xml = XmlStream::new(
        BufReader::with_capacity(limits.input_buffer_bytes, input),
        part.into(),
        limits.max_part_bytes,
        limits,
    );
    let mut writer = Writer::new(output);
    let mut root = false;
    loop {
        let frame = xml.next()?;
        check_declaration(&frame.event)?;
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                if frame.scope != Scope::Spreadsheet || e.local_name().as_ref().as_bytes() != b"sst"
                {
                    return Err(invalid("Shared string part has an invalid root"));
                }
                let mut start = e.to_owned();
                start.clear_attributes();
                for attribute in e.attributes() {
                    let attribute = attribute.map_err(|error| {
                        Error::caused_by(ErrorKind::Xml, "Invalid shared string attribute", error)
                    })?;
                    if attribute.key.as_ref().as_bytes() != b"count" {
                        start.push_attribute(attribute);
                    }
                }
                root = true;
                emit(&mut writer, Event::Start(start))?;
            }
            Event::Eof => break,
            event => emit(&mut writer, event)?,
        }
    }
    if !root {
        return Err(invalid("Shared string part has no root"));
    }
    let mut output = writer.into_inner();
    output
        .flush()
        .map_err(|error| io_error("Cannot flush rewritten XML part", error))?;
    Ok(output.bytes)
}

fn catalog_chain_removal<R: Read + Seek>(
    book: &mut WorkbookReader<R>,
    parts: &[PartInfo],
    chains: &HashSet<String>,
    workbook_relationships: &str,
    limits: ResourceLimits,
    used: &mut usize,
) -> Result<(HashSet<String>, bool)> {
    // Retain only tiny path inventories. Do not parse chain cells or retain an
    // all-package relationship DOM. Additional graph scans share one byte cap.
    let mut removals = HashSet::new();
    for chain in chains {
        for name in [chain.clone(), crate::package::relationship_part(chain)] {
            if !parts.iter().any(|part| part.name.as_ref() == name) {
                continue;
            }
            *used = used.saturating_add(name.len()).saturating_add(128);
            if *used as u128 > u128::from(limits.max_metadata_bytes) {
                return Err(limit("Calculation-chain inventory budget exceeded"));
            }
            removals.insert(name);
        }
    }
    let mut safe = chains
        .iter()
        .all(|chain| parts.iter().any(|part| part.name.as_ref() == chain));
    let mut remaining = limits.max_metadata_bytes;
    for part in parts {
        // Always inspect workbook chain relationships, including references with
        // missing/misdeclared content types. Other incoming edges matter only
        // when deleting cataloged chain parts.
        if part.name.as_ref() != workbook_relationships && chains.is_empty() {
            continue;
        }
        let Some(source) = crate::package::relationship_source(&part.name) else {
            continue;
        };
        if part.uncompressed_bytes > remaining {
            return Err(
                limit("Calculation-chain relationship scan byte limit exceeded")
                    .with_part(part.name.as_ref()),
            );
        }
        let file = book
            .archive
            .by_name(&part.name)
            .map_err(|error| zip_error("Cannot inspect calculation-chain relationships", error))?;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(limits.input_buffer_bytes, file),
            part.name.to_string(),
            remaining,
            limits,
        );
        loop {
            let frame = xml.next()?;
            if !chains.is_empty()
                && matches!(&frame.event, Event::Start(e) if e.local_name().as_ref().as_bytes()==b"AlternateContent")
            {
                safe = false;
            }
            match frame.event {
                Event::Start(e) if frame.depth == 1 => {
                    if frame.scope != Scope::Relationships
                        || e.local_name().as_ref().as_bytes() != b"Relationships"
                    {
                        safe = false;
                    }
                }
                Event::Start(e)
                    if frame.depth == 2
                        && frame.scope == Scope::Relationships
                        && e.local_name().as_ref().as_bytes() == b"Relationship" =>
                {
                    let kind = attribute(&e, b"Type")?
                        .ok_or_else(|| invalid("Relationship has no type"))?;
                    let target = attribute(&e, b"Target")?
                        .ok_or_else(|| invalid("Relationship has no target"))?;
                    if attribute(&e, b"TargetMode")?.as_deref() == Some("External") {
                        if crate::package::relationship_is(&kind, "calcChain") {
                            safe = false;
                        }
                        continue;
                    }
                    let target = crate::package::resolve_part(&source, &target)?;
                    if crate::package::relationship_is(&kind, "calcChain")
                        && (part.name.as_ref() != workbook_relationships
                            || !chains.contains(&target))
                    {
                        safe = false;
                    }
                    if chains.contains(&target)
                        && !(part.name.as_ref() == workbook_relationships
                            && crate::package::relationship_is(&kind, "calcChain"))
                    {
                        // Unknown consumers must not be left with dangling refs.
                        safe = false;
                    }
                    if removals.contains(part.name.as_ref()) {
                        // A chain part with outgoing relationships could own
                        // extension data: retain unchanged; reject editing.
                        safe = false;
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        remaining = remaining.saturating_sub(xml.bytes_consumed());
    }
    Ok((removals, safe))
}
fn patch_chain_metadata<R: Read + Seek, W: Write>(
    input: zip::read::ZipFile<'_, R>,
    output: PartOutput<W>,
    part: &str,
    chains: &HashSet<String>,
    limits: ResourceLimits,
) -> Result<u64> {
    let mut xml = XmlStream::new(
        BufReader::with_capacity(limits.input_buffer_bytes, input),
        part.into(),
        limits.max_metadata_bytes,
        limits,
    );
    let mut writer = Writer::new(output);
    let types = part == "[Content_Types].xml";
    let mut skip = None;
    loop {
        let frame = xml.next()?;
        check_declaration(&frame.event)?;
        if let Some(depth) = skip {
            if matches!(&frame.event, Event::End(_)) && frame.depth == depth - 1 {
                skip = None;
            }
            if matches!(&frame.event, Event::Eof) {
                return Err(invalid("Truncated removed package declaration"));
            }
            continue;
        }
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                let valid = if types {
                    frame.scope == Scope::ContentTypes
                        && e.local_name().as_ref().as_bytes() == b"Types"
                } else {
                    frame.scope == Scope::Relationships
                        && e.local_name().as_ref().as_bytes() == b"Relationships"
                };
                if !valid {
                    return Err(invalid("Invalid calculation-chain package metadata root"));
                }
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if frame.depth == 2
                    && types
                    && frame.scope == Scope::ContentTypes
                    && e.local_name().as_ref().as_bytes() == b"Override" =>
            {
                let name = attribute(&e, b"PartName")?
                    .ok_or_else(|| invalid("Content override has no part name"))?;
                if chains.contains(&crate::package::resolve_part("", &name)?) {
                    skip = Some(frame.depth);
                } else {
                    emit(&mut writer, Event::Start(e))?;
                }
            }
            Event::Start(e)
                if frame.depth == 2
                    && !types
                    && frame.scope == Scope::Relationships
                    && e.local_name().as_ref().as_bytes() == b"Relationship" =>
            {
                let kind =
                    attribute(&e, b"Type")?.ok_or_else(|| invalid("Relationship has no type"))?;
                if crate::package::relationship_is(&kind, "calcChain") {
                    skip = Some(frame.depth);
                } else {
                    emit(&mut writer, Event::Start(e))?;
                }
            }
            Event::Eof => break,
            event => emit(&mut writer, event)?,
        }
    }
    let mut output = writer.into_inner();
    output
        .flush()
        .map_err(|error| io_error("Cannot flush rewritten XML part", error))?;
    Ok(output.bytes)
}
