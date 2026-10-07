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

mod catalog;
mod catalog_edits;
mod graph;
mod metadata_edits;
mod output;
mod overlays;
mod save;
mod workbook_xml;
mod worksheet_xml;

use graph::*;
use output::*;
use workbook_xml::*;
use worksheet_xml::*;

const PATCH_BYTES: usize = 256;
// Box large metadata values so sparse BTree nodes retain pointer-sized slots.
// Conservative per-part node allowance; model payload/capacities are additional.
const METADATA_ENTRY_BYTES: usize = 1024;
struct Patch {
    cell: Cell,
    insert_missing: bool,
}
type Patches = BTreeMap<(u32, u32), Patch>;
pub(crate) struct ModelPlan {
    sheet: usize,
    pub(crate) bytes: usize,
}
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
struct SheetDeclaration {
    start: BytesStart<'static>,
    relationship: Option<Box<str>>,
    namespace: &'static str,
}
struct CatalogOrder {
    positions: Vec<usize>,
    entries: Vec<SheetDeclaration>,
    charged: usize,
}
fn catalog_order_bytes(positions: &Vec<usize>, entries: &Vec<SheetDeclaration>) -> usize {
    PATCH_BYTES
        .saturating_add(positions.capacity().saturating_mul(size_of::<usize>()))
        .saturating_add(
            entries
                .capacity()
                .saturating_mul(size_of::<SheetDeclaration>()),
        )
        .saturating_add(
            entries
                .iter()
                .map(|entry| {
                    entry.start.as_ref().len()
                        + entry.relationship.as_ref().map_or(0, |id| id.len())
                })
                .sum::<usize>(),
        )
}
pub(crate) struct OrderPlan {
    positions: Vec<usize>,
    entries: Option<Vec<SheetDeclaration>>,
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
    /// New worksheet parts created from borrowed canonical models.
    pub created_parts: usize,
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
    model_patches: BTreeMap<usize, crabxl_core::SheetId>,
    structural_plain_strings: bool,
    pub(crate) structural_rich_text: bool,
    pub(crate) structural_inline_rich_text: bool,
    pub(crate) model_font_count: Option<usize>,
    styles_dirty: bool,
    theme_dirty: bool,
    membership: Option<Box<catalog::Membership>>,
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
        let (chain_removals, chain_safe) = catalog_part_removal(
            &mut book,
            &parts,
            &calc_chain_parts,
            &workbook_relationships,
            options.resources,
            &mut bytes,
            GraphOwner::CalculationChain,
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
            model_patches: BTreeMap::new(),
            styles_dirty: false,
            theme_dirty: false,
            structural_plain_strings: false,
            structural_rich_text: false,
            structural_inline_rich_text: false,
            model_font_count: None,
            membership: None,
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
        self.styles_dirty
            || self.theme_dirty
            || self.patch_cells != 0
            || self.membership.is_some()
            || !self.model_patches.is_empty()
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
    /// Release this editor and return its original owned source.
    pub fn into_source(self) -> R {
        self.book.into_inner()
    }
}
