//! Lazy original-package preservation and bounded existing-cell value overlays.
use crate::encode::{RowBuffer, encode_cells, validate_value};
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
    io::{self, BufReader, Read, Seek, Write},
    path::Path,
};
use zip::{ZipWriter, write::SimpleFileOptions};

const PATCH_BYTES: usize = 256;
struct Patch {
    cell: Cell,
    insert_missing: bool,
}
type Patches = BTreeMap<(u32, u32), Patch>;

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
    /// Reject all edits when a calculation-chain part is present.
    RejectEdits,
}
/// Budgets for a lazy package editor and its owned value overlays.
#[derive(Clone, Debug)]
pub struct EditorOptions {
    /// Original archive/XML/metadata limits, also limiting rewritten XML parts.
    pub resources: ResourceLimits,
    /// Shared Auto or explicit managed operation budget, including work reserve.
    pub memory_policy: MemoryPolicy,
    /// Additional patch cap (usize::MAX by default); excludes allocator overhead.
    pub max_patch_bytes: usize,
    /// Maximum distinct pending cell replacements.
    pub max_patch_cells: usize,
    /// How to handle the original derived calculation order during edits.
    pub calculation_chain: CalculationChainPolicy,
}
impl Default for EditorOptions {
    fn default() -> Self {
        Self {
            resources: ResourceLimits::default(),
            max_patch_bytes: usize::MAX,
            memory_policy: MemoryPolicy::default(),
            max_patch_cells: 10_000_000,
            calculation_chain: CalculationChainPolicy::default(),
        }
    }
}
/// Controls CRC validation during copying. Rewritten XML is always parsed to EOF.
#[derive(Clone, Copy, Debug, Default)]
pub struct SaveOptions {
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
/// Owns the original source and a bounded overlay, keeping unknown XML/binary
/// parts on their original source. save borrows self, so repeated saves retain
/// images/macros and never consume the original or the pending edits.
///
/// This checkpoint changes existing scalar/normal-formula cells only. It keeps
/// cell styles and relationships; upsert_value also inserts missing cells.
/// Date/style registration and structural edits in existing packages remain staged.
/// Derived calculation chains are discarded on edits under the default policy. Any edited
/// workbook has worksheet formula caches invalidated and recalculation requested.
pub struct WorkbookEditor<R: Read + Seek = File> {
    book: WorkbookReader<R>,
    parts: Vec<PartInfo>,
    patches: BTreeMap<String, Patches>,
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
                    if let Some(kind) = attribute(&e, b"ContentType", frame.decoder)? {
                        signed |= kind.contains("digital-signature");
                        if e.local_name().as_ref() == b"Override"
                            && (kind.ends_with("sharedStrings+xml")
                                || kind.ends_with("calcChain+xml"))
                        {
                            let name =
                                attribute(&e, b"PartName", frame.decoder)?.ok_or_else(|| {
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
        let info = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet does not exist"))?;
        if info.kind() != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Selected sheet is not a cell worksheet",
            ));
        }
        if contains_date(&value) {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing dates requires an existing style/date catalog",
            )
            .with_cell(address));
        }
        if matches!(&value, CellValue::RichText(v) if v.phonetic_properties.is_some()) {
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
        validate_value(&value, self.options.resources.max_cell_bytes, epoch)
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
        self.patches.entry(info.part().into()).or_default().insert(
            key,
            Patch {
                cell: Cell {
                    address,
                    value,
                    style: StyleId::new(0),
                },
                insert_missing,
            },
        );
        self.patch_bytes = bytes;
        self.patch_cells = cells;
        Ok(())
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
        self.patch_bytes = 0;
        self.patch_cells = 0;
    }
    /// Save to a caller-owned fresh/truncated sink; failures may leave partial
    /// sink bytes. Caller ownership can be retained by passing &mut W.
    /// Unchanged entries preserve compressed payloads; edited workbooks rewrite
    /// all worksheets to remove old formula caches, plus calculation properties.
    pub fn save<W: Write + Seek>(
        &mut self,
        output: W,
        options: SaveOptions,
    ) -> Result<(W, SaveStats)> {
        let dirty = self.is_dirty();
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
            let worksheet = dirty
                && self.book.sheets().iter().any(|sheet| {
                    sheet.kind() == SheetKind::Worksheet && sheet.part() == part.name.as_ref()
                });
            let workbook = dirty && part.name.as_ref() == self.book.workbook_part;
            let shared_strings = dirty && self.shared_string_parts.contains(part.name.as_ref());
            if worksheet || workbook || shared_strings || chain_metadata {
                let file = self.book.archive.by_index(index).map_err(|error| {
                    zip_error("Cannot read affected XML part", error).with_part(part.name.as_ref())
                })?;
                // Edited parts use deflate; unknown parts retain original compression.
                zip.start_file(
                    part.name.as_ref(),
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated)
                        .large_file(self.options.resources.max_part_bytes >= u64::from(u32::MAX)),
                )
                .map_err(|error| {
                    zip_error("Cannot start affected XML part", error).with_part(part.name.as_ref())
                })?;
                let budget = PartOutput {
                    inner: &mut zip,
                    bytes: 0,
                    maximum: self.options.resources.max_part_bytes,
                };
                let written = if worksheet {
                    patch_worksheet(
                        file,
                        budget,
                        &part.name,
                        self.patches.get(part.name.as_ref()),
                        self.options.resources,
                    )
                } else if workbook {
                    patch_workbook(file, budget, &part.name, self.options.resources)
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
        temporary
            .persist(path)
            .map_err(|error| io_error("Cannot replace edited workbook target", error.error))?;
        Ok(stats)
    }
    /// Release this editor and return its original owned source.
    pub fn into_source(self) -> R {
        self.book.into_inner()
    }
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
    if let Event::Decl(declaration) = event {
        if let Some(encoding) = declaration.encoding() {
            let encoding = encoding.map_err(|error| {
                Error::caused_by(ErrorKind::Xml, "Invalid XML encoding declaration", error)
            })?;
            if !encoding.eq_ignore_ascii_case(b"UTF-8") {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Rewriting non-UTF-8 XML is not supported",
                ));
            }
        }
    }
    Ok(())
}
fn unsigned_attribute(
    e: &BytesStart<'_>,
    name: &[u8],
    decoder: quick_xml::encoding::Decoder,
) -> Result<Option<u32>> {
    let Some(attribute) = e
        .try_get_attribute(name)
        .map_err(|error| Error::caused_by(ErrorKind::Xml, "Invalid position attribute", error))?
    else {
        return Ok(None);
    };
    let value = attribute
        .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, decoder)
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
        if matches!(attribute.key.as_ref(), b"cm" | b"vm") {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing cell metadata references requires typed metadata support",
            ));
        }
        if !matches!(attribute.key.as_ref(), b"t" | b"xmlns") {
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
) -> Result<()> {
    encode_cells(
        buffer,
        cell.address.row,
        std::slice::from_ref(cell).iter(),
        limits.max_cell_bytes,
        1,
        &[CellStyle::default()],
        DateEpoch::Windows1900,
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
    write_body(writer, &patch.cell, buffer, limits)?;
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
        if attribute.key.as_ref() != b"r" && !(omit_spans && attribute.key.as_ref() == b"spans") {
            start.push_attribute(attribute);
        }
    }
    start.push_attribute(("r", position));
    Ok(start)
}
fn expanded_dimension(
    e: &BytesStart<'_>,
    decoder: quick_xml::encoding::Decoder,
    patches: &Patches,
) -> Result<BytesStart<'static>> {
    let reference = attribute(e, b"ref", decoder)?
        .ok_or_else(|| invalid("Worksheet dimension has no reference"))?;
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
        if attribute.key.as_ref() != b"ref" {
            start.push_attribute(attribute);
        }
    }
    start.push_attribute(("ref", reference.as_str()));
    Ok(start)
}
fn patch_worksheet<R: Read + Seek, W: Write>(
    input: zip::read::ZipFile<'_, R>,
    output: PartOutput<W>,
    part: &str,
    patches: Option<&Patches>,
    limits: ResourceLimits,
) -> Result<u64> {
    let mut xml = XmlStream::new(
        BufReader::with_capacity(limits.input_buffer_bytes, input),
        part.into(),
        limits.max_part_bytes,
        limits,
    );
    let mut writer = Writer::new(output);
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
    let mut formula = false;
    let mut seen_v = false;
    let mut buffer = RowBuffer {
        data: Vec::new(),
        maximum: limits.max_row_bytes,
    };
    loop {
        let frame = xml.next()?;
        check_declaration(&frame.event)?;
        if matches!(&frame.event,Event::Start(e) if e.local_name().as_ref()==b"AlternateContent") {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing markup-compatibility alternatives requires typed branch handling",
            ));
        }
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                if frame.scope != Scope::Spreadsheet || e.local_name().as_ref() != b"worksheet" {
                    return Err(invalid("Affected part is not a worksheet"));
                }
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref() == b"dimension"
                    && patches.is_some() =>
            {
                let start = expanded_dimension(
                    &e,
                    frame.decoder,
                    patches.ok_or_else(|| invalid("Missing overlays"))?,
                )?;
                emit(&mut writer, Event::Start(start))?;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 3
                    && e.local_name().as_ref() == b"mergeCell"
                    && patches.is_some() =>
            {
                let reference = attribute(&e, b"ref", frame.decoder)?
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
                    && e.local_name().as_ref() == b"sheetData" =>
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
                    && e.local_name().as_ref() == b"row" =>
            {
                in_row = true;
                row = unsigned_attribute(&e, b"r", frame.decoder)?
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
                        write_inserted_cell(&mut writer, patch, uri, &mut buffer, limits)?;
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
                    && e.local_name().as_ref() == b"extLst" =>
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
                    write_inserted_cell(&mut writer, patch, uri, &mut buffer, limits)?;
                    found += 1;
                }
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if in_row
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 4
                    && e.local_name().as_ref() == b"c" =>
            {
                if row_tail {
                    return Err(invalid("Cell follows row extension list"));
                }
                in_cell = true;
                formula = false;
                seen_v = false;
                let replacement = if selected_row {
                    let address = attribute(&e, b"r", frame.decoder)?
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
                        write_inserted_cell(&mut writer, patch, uri, &mut buffer, limits)?;
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
                    let name = start.name().as_ref().to_vec();
                    // Validate the old cell before replacing its body.
                    loop {
                        let old = xml.next()?;
                        match &old.event {
                            Event::End(end)
                                if old.depth == 3 && end.local_name().as_ref() == b"c" =>
                            {
                                break;
                            }
                            Event::Start(child)
                                if old.scope == Scope::Spreadsheet
                                    && old.depth == 5
                                    && child.local_name().as_ref() == b"is" =>
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
                                        child.local_name().as_ref(),
                                        b"v" | b"is" | b"t" | b"f"
                                    )
                                {
                                    return Err(Error::new(ErrorKind::Unsupported,"Replacing unknown or rich cell content requires typed support").with_cell(cell.address));
                                }
                                if child.local_name().as_ref() == b"f" {
                                    for attribute in child.attributes() {
                                        let attribute = attribute.map_err(|error| {
                                            Error::caused_by(
                                                ErrorKind::Xml,
                                                "Invalid formula metadata",
                                                error,
                                            )
                                        })?;
                                        if attribute.key.as_ref() != b"t"
                                            || attribute.value.as_ref() != b"normal"
                                        {
                                            return Err(Error::new(ErrorKind::Unsupported,"Replacing non-normal formula metadata is not supported").with_cell(cell.address));
                                        }
                                    }
                                }
                            }
                            Event::Eof => return Err(invalid("Unexpected end of replaced cell")),
                            _ => {}
                        }
                    }
                    emit(&mut writer, Event::Start(start))?;
                    write_body(&mut writer, cell, &mut buffer, limits)?;
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
                    && e.local_name().as_ref() == b"f" =>
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
                    && e.local_name().as_ref() == b"v" =>
            {
                seen_v = true;
                if formula {
                    loop {
                        let frame = xml.next()?;
                        if matches!(&frame.event,Event::End(end) if frame.depth==4 && end.local_name().as_ref()==b"v")
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
                    && e.local_name().as_ref() == b"c" =>
            {
                in_cell = false;
                emit(&mut writer, Event::End(e))?;
            }
            Event::End(e)
                if in_row
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref() == b"row" =>
            {
                while pending
                    .peek()
                    .is_some_and(|patch| patch.cell.address.row.get() == row)
                {
                    let patch = pending
                        .next()
                        .ok_or_else(|| invalid("Missing pending cell"))?;
                    let uri = data_uri.ok_or_else(|| invalid("Worksheet namespace is missing"))?;
                    write_inserted_cell(&mut writer, patch, uri, &mut buffer, limits)?;
                    found += 1;
                }
                in_row = false;
                emit(&mut writer, Event::End(e))?;
            }
            Event::End(e)
                if in_data
                    && frame.scope == Scope::Spreadsheet
                    && frame.depth == 1
                    && e.local_name().as_ref() == b"sheetData" =>
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
                        write_inserted_cell(&mut writer, patch, uri, &mut buffer, limits)?;
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
    Ok(writer.into_inner().bytes)
}
fn patch_workbook<R: Read + Seek, W: Write>(
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
    let mut seen = false;
    let mut uri = None;
    loop {
        let frame = xml.next()?;
        check_declaration(&frame.event)?;
        if matches!(&frame.event,Event::Start(e) if e.local_name().as_ref()==b"AlternateContent") {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Editing markup-compatibility alternatives requires typed branch handling",
            ));
        }
        match frame.event {
            Event::Start(e) if frame.depth == 1 => {
                if frame.scope != Scope::Spreadsheet || e.local_name().as_ref() != b"workbook" {
                    return Err(invalid("Affected part is not a workbook"));
                }
                uri = frame.spreadsheet_uri;
                emit(&mut writer, Event::Start(e))?;
            }
            Event::Start(e)
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && e.local_name().as_ref() == b"calcPr" =>
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
                        attribute.key.as_ref(),
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
                if frame.scope == Scope::Spreadsheet
                    && frame.depth == 2
                    && matches!(
                        e.local_name().as_ref(),
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
                    && e.local_name().as_ref() == b"workbook" =>
            {
                if !seen {
                    emit_calculation(&mut writer, uri)?;
                    seen = true;
                }
                emit(&mut writer, Event::End(e))?;
            }
            Event::Eof => break,
            event => emit(&mut writer, event)?,
        }
    }
    if !seen {
        return Err(invalid("Workbook calculation properties were not written"));
    }
    Ok(writer.into_inner().bytes)
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
                if frame.scope != Scope::Spreadsheet || e.local_name().as_ref() != b"sst" {
                    return Err(invalid("Shared string part has an invalid root"));
                }
                let mut start = e.to_owned();
                start.clear_attributes();
                for attribute in e.attributes() {
                    let attribute = attribute.map_err(|error| {
                        Error::caused_by(ErrorKind::Xml, "Invalid shared string attribute", error)
                    })?;
                    if attribute.key.as_ref() != b"count" {
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
    Ok(writer.into_inner().bytes)
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
                && matches!(&frame.event, Event::Start(e) if e.local_name().as_ref()==b"AlternateContent")
            {
                safe = false;
            }
            match frame.event {
                Event::Start(e) if frame.depth == 1 => {
                    if frame.scope != Scope::Relationships
                        || e.local_name().as_ref() != b"Relationships"
                    {
                        safe = false;
                    }
                }
                Event::Start(e)
                    if frame.depth == 2
                        && frame.scope == Scope::Relationships
                        && e.local_name().as_ref() == b"Relationship" =>
                {
                    let kind = attribute(&e, b"Type", frame.decoder)?
                        .ok_or_else(|| invalid("Relationship has no type"))?;
                    let target = attribute(&e, b"Target", frame.decoder)?
                        .ok_or_else(|| invalid("Relationship has no target"))?;
                    if attribute(&e, b"TargetMode", frame.decoder)?.as_deref() == Some("External") {
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
                    frame.scope == Scope::ContentTypes && e.local_name().as_ref() == b"Types"
                } else {
                    frame.scope == Scope::Relationships
                        && e.local_name().as_ref() == b"Relationships"
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
                    && e.local_name().as_ref() == b"Override" =>
            {
                let name = attribute(&e, b"PartName", frame.decoder)?
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
                    && e.local_name().as_ref() == b"Relationship" =>
            {
                let kind = attribute(&e, b"Type", frame.decoder)?
                    .ok_or_else(|| invalid("Relationship has no type"))?;
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
    Ok(writer.into_inner().bytes)
}
