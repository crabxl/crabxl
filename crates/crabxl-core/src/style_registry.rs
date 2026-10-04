//! Bounded canonical component interning for owned workbook styles.
use crate::{
    CellFormat, CellStyle, Error, ErrorKind, Fill, FillPattern, NamedStyle, NumberFormat,
    PatternFill, Protection, Result, StyleCatalog, StyleId, builtin_number_format_id,
};
use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
};

/// Managed retained style storage and actual per-table record allowances.
#[derive(Clone, Copy, Debug)]
pub struct StyleLimits {
    /// Catalog capacities/payload plus conservative hash-index storage estimate.
    /// Caller-owned input, allocator overhead and runtime baseline are additional.
    pub max_bytes: usize,
    /// Maximum actual records per component/format table.
    pub max_records: usize,
}
impl Default for StyleLimits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
            max_records: 100_000,
        }
    }
}
fn limit() -> Error {
    Error::new(
        ErrorKind::MemoryBudgetExceeded,
        "Style catalog allowance exceeded",
    )
}
fn allocation(error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::caused_by(
        ErrorKind::MemoryBudgetExceeded,
        "Cannot reserve shared style storage",
        error,
    )
}
fn fingerprint(value: &impl Hash) -> u64 {
    let mut hash = DefaultHasher::new();
    value.hash(&mut hash);
    hash.finish()
}
fn bucket_bytes(capacity: usize) -> usize {
    if capacity == 0 {
        0
    } else {
        capacity
            .saturating_add(1)
            .checked_next_power_of_two()
            .unwrap_or(usize::MAX)
            .saturating_mul(size_of::<(u64, Vec<u32>)>() + 1)
            .saturating_add(16)
    }
}
#[derive(Default)]
struct Index {
    values: HashMap<u64, Vec<u32>>,
    ids_bytes: usize,
}
impl Index {
    fn find_by(&self, hash: u64, mut equals: impl FnMut(u32) -> bool) -> Option<u32> {
        self.values
            .get(&hash)?
            .iter()
            .copied()
            .find(|id| equals(*id))
    }
    fn find<T: Hash + PartialEq>(&self, values: &[T], value: &T) -> Option<u32> {
        self.find_by(fingerprint(value), |id| values[id as usize] == *value)
    }
    fn heap_bytes(&self) -> usize {
        bucket_bytes(self.values.capacity()).saturating_add(self.ids_bytes)
    }
    fn growth(&self, hash: u64) -> usize {
        if let Some(ids) = self.values.get(&hash) {
            return usize::from(ids.len() == ids.capacity()) * size_of::<u32>();
        }
        let capacity = self.values.capacity();
        let buckets = if self.values.len() == capacity {
            if capacity == 0 {
                bucket_bytes(3)
            } else {
                bucket_bytes(capacity).saturating_mul(2)
            }
        } else {
            bucket_bytes(capacity)
        };
        buckets
            .saturating_sub(bucket_bytes(capacity))
            .saturating_add(size_of::<u32>())
    }
    fn reserve(&mut self, hash: u64) -> Result<Option<Vec<u32>>> {
        if let Some(ids) = self.values.get_mut(&hash) {
            let before = ids.capacity();
            ids.try_reserve_exact(1).map_err(allocation)?;
            self.ids_bytes = self
                .ids_bytes
                .saturating_add((ids.capacity() - before) * size_of::<u32>());
            return Ok(None);
        }
        self.values.try_reserve(1).map_err(allocation)?;
        let mut ids = Vec::new();
        ids.try_reserve_exact(1).map_err(allocation)?;
        Ok(Some(ids))
    }
    fn insert(&mut self, hash: u64, id: u32, prepared: Option<Vec<u32>>) {
        if let Some(mut ids) = prepared {
            self.ids_bytes = self
                .ids_bytes
                .saturating_add(ids.capacity() * size_of::<u32>());
            ids.push(id);
            self.values.insert(hash, ids);
        } else if let Some(ids) = self.values.get_mut(&hash) {
            ids.push(id);
        }
    }
}
fn vector_growth<T>(values: &Vec<T>, maximum: usize, geometric: bool) -> usize {
    if values.len() < values.capacity() {
        return 0;
    }
    let additional = if geometric {
        values
            .capacity()
            .max(4)
            .min(maximum.saturating_sub(values.len()))
    } else {
        1
    };
    additional.saturating_mul(size_of::<T>())
}
fn reserve<T>(values: &mut Vec<T>, maximum: usize, geometric: bool) -> Result<()> {
    if values.len() >= maximum {
        return Err(limit());
    }
    if values.len() == values.capacity() {
        let additional = vector_growth(values, maximum, geometric) / size_of::<T>();
        values.try_reserve_exact(additional).map_err(allocation)?;
    }
    Ok(())
}
#[derive(Clone, Copy, Hash, PartialEq)]
struct FormatKey<'a> {
    number_format_id: u32,
    font_id: u32,
    fill_id: u32,
    border_id: u32,
    base_format_id: Option<u32>,
    apply_number_format: Option<bool>,
    apply_font: Option<bool>,
    apply_fill: Option<bool>,
    apply_border: Option<bool>,
    apply_alignment: Option<bool>,
    apply_protection: Option<bool>,
    quote_prefix: Option<bool>,
    pivot_button: Option<bool>,
    alignment: Option<&'a crate::Alignment>,
    protection: Option<Protection>,
    unmodeled_extensions: bool,
}
fn format_key<'a>(format: &CellFormat, alignment: Option<&'a crate::Alignment>) -> FormatKey<'a> {
    FormatKey {
        number_format_id: format.number_format_id,
        font_id: format.font_id,
        fill_id: format.fill_id,
        border_id: format.border_id,
        base_format_id: format.base_format_id,
        apply_number_format: format.apply_number_format,
        apply_font: format.apply_font,
        apply_fill: format.apply_fill,
        apply_border: format.apply_border,
        apply_alignment: format.apply_alignment,
        apply_protection: format.apply_protection,
        quote_prefix: format.quote_prefix,
        pivot_button: format.pivot_button,
        alignment,
        protection: format.protection,
        unmodeled_extensions: format.unmodeled_extensions,
    }
}
#[derive(Clone, Copy)]
enum IndexKind {
    Font,
    Fill,
    Border,
    Number,
    Format,
}
/// One canonical catalog plus collision-checked indices. Styles share components,
/// not whole appearance clones. Catalog access is borrowed and immutable so
/// numeric IDs and deduplication indices cannot become stale.
pub struct StyleRegistry {
    catalog: StyleCatalog,
    fonts: Index,
    fills: Index,
    borders: Index,
    numbers: Index,
    formats: Index,
    limits: StyleLimits,
    payload_bytes: usize,
    reserved_number_ids: Vec<u32>,
    next_number_id: u64,
}
impl StyleRegistry {
    /// Create the normal style, two required fills and one named/base record.
    pub fn new(limits: StyleLimits) -> Result<Self> {
        if limits.max_bytes == 0 || limits.max_records < 2 || limits.max_records > u32::MAX as usize
        {
            return Err(limit());
        }
        let mut registry = Self {
            catalog: StyleCatalog::default(),
            fonts: Index::default(),
            fills: Index::default(),
            borders: Index::default(),
            numbers: Index::default(),
            formats: Index::default(),
            limits,
            payload_bytes: 0,
            reserved_number_ids: Vec::new(),
            next_number_id: 164,
        };
        for fill in [
            Fill::default(),
            Fill::Pattern(PatternFill {
                pattern: Some(FillPattern::Gray125),
                ..Default::default()
            }),
        ] {
            let hash = fingerprint(&fill);
            let wanted = registry
                .memory_bytes()
                .saturating_add(vector_growth(
                    &registry.catalog.fills,
                    limits.max_records,
                    false,
                ))
                .saturating_add(registry.fills.growth(hash));
            if wanted > limits.max_bytes {
                return Err(limit());
            }
            reserve(&mut registry.catalog.fills, limits.max_records, false)?;
            let prepared = registry.fills.reserve(hash)?;
            let id = registry.catalog.fills.len() as u32;
            registry.catalog.fills.push(fill);
            registry.fills.insert(hash, id, prepared);
        }
        registry
            .catalog
            .base_formats
            .try_reserve_exact(1)
            .map_err(allocation)?;
        registry
            .catalog
            .named_styles
            .try_reserve_exact(1)
            .map_err(allocation)?;
        registry.catalog.base_formats.push(CellFormat::default());
        registry.catalog.named_styles.push(NamedStyle {
            name: "Normal".into(),
            base_format_id: 0,
            builtin_id: Some(0),
            custom_builtin: None,
            hidden: None,
            outline_level: None,
        });
        registry.payload_bytes = "Normal".len();
        registry.register(CellStyle::default())?;
        Ok(registry)
    }
    /// Adopt existing tables without remapping component, format or source IDs.
    /// Duplicate components remain present; new registration reuses the first equal record.
    /// Source custom-number IDs are sparse and never size a dense allocation.
    pub fn from_catalog(mut catalog: StyleCatalog, limits: StyleLimits) -> Result<Self> {
        if limits.max_bytes == 0
            || limits.max_records == 0
            || limits.max_records > u32::MAX as usize
        {
            return Err(limit());
        }
        for count in [
            catalog.fonts.len(),
            catalog.fills.len(),
            catalog.borders.len(),
            catalog.number_formats.len(),
            catalog.cell_formats.len(),
            catalog.base_formats.len(),
            catalog.named_styles.len(),
            catalog.indexed_colors.len(),
            catalog.recent_colors.len(),
            catalog.unmodeled_sections.len(),
        ] {
            if count > limits.max_records {
                return Err(limit());
            }
        }
        if catalog.memory_bytes().saturating_add(size_of::<Self>()) > limits.max_bytes {
            return Err(limit());
        }
        catalog
            .number_formats
            .sort_unstable_by_key(NumberFormat::id);
        catalog.validate_references()?;
        let payload_bytes = catalog
            .number_formats
            .iter()
            .map(|n| n.code().len())
            .sum::<usize>()
            + catalog
                .fonts
                .iter()
                .map(|v| v.name.as_ref().map_or(0, |s| s.len()))
                .sum::<usize>()
            + catalog.fills.iter().map(Fill::heap_bytes).sum::<usize>()
            + catalog
                .base_formats
                .iter()
                .chain(&catalog.cell_formats)
                .map(CellFormat::heap_bytes)
                .sum::<usize>()
            + catalog
                .named_styles
                .iter()
                .map(|v| v.name.len())
                .sum::<usize>()
            + catalog
                .unmodeled_sections
                .iter()
                .map(|v| v.len())
                .sum::<usize>();
        let mut value = Self {
            catalog,
            fonts: Index::default(),
            fills: Index::default(),
            borders: Index::default(),
            numbers: Index::default(),
            formats: Index::default(),
            limits,
            payload_bytes,
            reserved_number_ids: Vec::new(),
            next_number_id: 164,
        };
        if value.memory_bytes().saturating_add(
            value
                .catalog
                .number_formats
                .len()
                .saturating_mul(size_of::<u32>()),
        ) > limits.max_bytes
        {
            return Err(limit());
        }
        value
            .reserved_number_ids
            .try_reserve_exact(value.catalog.number_formats.len())
            .map_err(allocation)?;
        value
            .reserved_number_ids
            .extend(value.catalog.number_formats.iter().map(NumberFormat::id));
        value.reserved_number_ids.sort_unstable();
        if value.reserved_number_ids.windows(2).any(|p| p[0] == p[1]) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Duplicate number-format identity",
            ));
        }
        for color in &value.catalog.recent_colors {
            color.validate()?;
        }
        for i in 0..value.catalog.fonts.len() {
            value.catalog.fonts[i].validate()?;
            value.index_imported(
                IndexKind::Font,
                fingerprint(&value.catalog.fonts[i]),
                i as u32,
            )?;
        }
        for i in 0..value.catalog.fills.len() {
            if let Fill::Gradient(v) = &value.catalog.fills[i] {
                if value
                    .memory_bytes()
                    .saturating_add(v.stops.len().saturating_mul(size_of::<u64>()))
                    > limits.max_bytes
                {
                    return Err(limit());
                }
            }
            value.catalog.fills[i].validate()?;
            value.index_imported(
                IndexKind::Fill,
                fingerprint(&value.catalog.fills[i]),
                i as u32,
            )?;
        }
        for i in 0..value.catalog.borders.len() {
            value.catalog.borders[i].validate()?;
            value.index_imported(
                IndexKind::Border,
                fingerprint(&value.catalog.borders[i]),
                i as u32,
            )?;
        }
        for i in 0..value.catalog.number_formats.len() {
            value.index_imported(
                IndexKind::Number,
                fingerprint(&value.catalog.number_formats[i].code()),
                i as u32,
            )?;
        }
        for i in 0..value.catalog.cell_formats.len() {
            let format = &value.catalog.cell_formats[i];
            let hash = fingerprint(&format_key(format, format.alignment.as_deref()));
            value.index_imported(IndexKind::Format, hash, i as u32)?;
        }
        Ok(value)
    }
    fn index_imported(&mut self, kind: IndexKind, hash: u64, id: u32) -> Result<()> {
        let index = match kind {
            IndexKind::Font => &self.fonts,
            IndexKind::Fill => &self.fills,
            IndexKind::Border => &self.borders,
            IndexKind::Number => &self.numbers,
            IndexKind::Format => &self.formats,
        };
        if self.memory_bytes().saturating_add(index.growth(hash)) > self.limits.max_bytes {
            return Err(limit());
        }
        let index = match kind {
            IndexKind::Font => &mut self.fonts,
            IndexKind::Fill => &mut self.fills,
            IndexKind::Border => &mut self.borders,
            IndexKind::Number => &mut self.numbers,
            IndexKind::Format => &mut self.formats,
        };
        let prepared = index.reserve(hash)?;
        index.insert(hash, id, prepared);
        if self.memory_bytes() > self.limits.max_bytes {
            return Err(limit());
        }
        Ok(())
    }
    fn available_number_id(&self) -> Result<u32> {
        let mut next = self.next_number_id;
        while next <= u64::from(u32::MAX)
            && self
                .reserved_number_ids
                .binary_search(&(next as u32))
                .is_ok()
        {
            next += 1;
        }
        u32::try_from(next).map_err(|_| limit())
    }
    /// Borrow all canonical tables with stable workbook-local identities.
    pub fn catalog(&self) -> &StyleCatalog {
        &self.catalog
    }
    /// Managed retained capacities/payload and conservative index estimate.
    pub fn memory_bytes(&self) -> usize {
        let catalog = &self.catalog;
        let capacities = catalog.number_formats.capacity() * size_of::<NumberFormat>()
            + catalog.fonts.capacity() * size_of::<crate::Font>()
            + catalog.fills.capacity() * size_of::<Fill>()
            + catalog.borders.capacity() * size_of::<crate::Border>()
            + catalog.base_formats.capacity() * size_of::<CellFormat>()
            + catalog.cell_formats.capacity() * size_of::<CellFormat>()
            + catalog.named_styles.capacity() * size_of::<NamedStyle>()
            + catalog.indexed_colors.capacity() * size_of::<u32>()
            + catalog.recent_colors.capacity() * size_of::<crate::Color>()
            + catalog.unmodeled_sections.capacity() * size_of::<Box<str>>();
        size_of::<Self>()
            .saturating_add(capacities)
            .saturating_add(self.reserved_number_ids.capacity() * size_of::<u32>())
            .saturating_add(self.payload_bytes)
            .saturating_add(self.fonts.heap_bytes())
            .saturating_add(self.fills.heap_bytes())
            .saturating_add(self.borders.heap_bytes())
            .saturating_add(self.numbers.heap_bytes())
            .saturating_add(self.formats.heap_bytes())
    }
    /// Intern a literal number-format code, respecting source built-in overrides.
    /// Returns a stable source/custom ID without dense allocation or component copies.
    pub fn register_number_format(&mut self, code: Box<str>) -> Result<u32> {
        self.register_number_format_with_limit(code, self.limits.max_bytes)
    }
    /// Intern a caller-owned code under a smaller aggregate allowance.
    pub fn register_number_format_with_limit(
        &mut self,
        code: Box<str>,
        maximum: usize,
    ) -> Result<u32> {
        let maximum = maximum.min(self.limits.max_bytes);
        if self.memory_bytes() > maximum {
            return Err(limit());
        }
        if let Some(id) = self.number_id_for_code(&code) {
            return Ok(id);
        }
        let id = self.available_number_id()?;
        let hash = fingerprint(&code);
        let retained = self
            .memory_bytes()
            .saturating_add(code.len())
            .saturating_add(self.numbers.growth(hash));
        let geometric = retained.saturating_add(vector_growth(
            &self.catalog.number_formats,
            self.limits.max_records,
            true,
        )) <= maximum;
        if retained.saturating_add(vector_growth(
            &self.catalog.number_formats,
            self.limits.max_records,
            geometric,
        )) > maximum
        {
            return Err(limit());
        }
        reserve(
            &mut self.catalog.number_formats,
            self.limits.max_records,
            geometric,
        )?;
        let prepared = self.numbers.reserve(hash)?;
        if self
            .memory_bytes()
            .saturating_add(code.len())
            .saturating_add(
                prepared
                    .as_ref()
                    .map_or(0, |v| v.capacity() * size_of::<u32>()),
            )
            > maximum
        {
            return Err(limit());
        }
        self.payload_bytes = self.payload_bytes.saturating_add(code.len());
        self.insert_number_record(hash, id, code, prepared);
        Ok(id)
    }
    fn number_id_for_code(&self, code: &str) -> Option<u32> {
        self.numbers
            .find_by(fingerprint(&code), |id| {
                self.catalog.number_formats[id as usize].code() == code
            })
            .map(|id| self.catalog.number_formats[id as usize].id())
            .or_else(|| {
                builtin_number_format_id(code).filter(|id| {
                    self.catalog
                        .declared_number_format(*id)
                        .is_none_or(|source| source == code)
                })
            })
    }
    fn insert_number_record(
        &mut self,
        hash: u64,
        id: u32,
        code: Box<str>,
        prepared: Option<Vec<u32>>,
    ) {
        let position = self
            .catalog
            .number_formats
            .partition_point(|format| format.id() < id);
        if position < self.catalog.number_formats.len() {
            for ids in self.numbers.values.values_mut() {
                for index in ids {
                    if *index as usize >= position {
                        *index += 1;
                    }
                }
            }
        }
        self.catalog
            .number_formats
            .insert(position, NumberFormat::new(id, code));
        self.numbers.insert(hash, position as u32, prepared);
        self.next_number_id = u64::from(id) + 1;
    }
    /// Intern a complete format referencing existing components without replacing source flags.
    /// Absence, explicit zero/false and unknown extension markers remain distinct.
    pub fn register_format(&mut self, format: CellFormat) -> Result<StyleId> {
        self.register_format_with_limit(format, self.limits.max_bytes)
    }
    /// Intern a raw source format under a smaller aggregate allowance.
    pub fn register_format_with_limit(
        &mut self,
        format: CellFormat,
        maximum: usize,
    ) -> Result<StyleId> {
        let maximum = maximum.min(self.limits.max_bytes);
        if self.memory_bytes() > maximum {
            return Err(limit());
        }
        self.catalog.validate_format(&format)?;
        let key = format_key(&format, format.alignment.as_deref());
        let hash = fingerprint(&key);
        if let Some(id) = self.formats.find_by(hash, |id| {
            let existing = &self.catalog.cell_formats[id as usize];
            format_key(existing, existing.alignment.as_deref()) == key
        }) {
            return Ok(StyleId::new(id));
        }
        let retained = self
            .memory_bytes()
            .saturating_add(format.heap_bytes())
            .saturating_add(self.formats.growth(hash));
        let geometric = retained.saturating_add(vector_growth(
            &self.catalog.cell_formats,
            self.limits.max_records,
            true,
        )) <= maximum;
        if retained.saturating_add(vector_growth(
            &self.catalog.cell_formats,
            self.limits.max_records,
            geometric,
        )) > maximum
        {
            return Err(limit());
        }
        reserve(
            &mut self.catalog.cell_formats,
            self.limits.max_records,
            geometric,
        )?;
        let prepared = self.formats.reserve(hash)?;
        if self
            .memory_bytes()
            .saturating_add(format.heap_bytes())
            .saturating_add(
                prepared
                    .as_ref()
                    .map_or(0, |v| v.capacity() * size_of::<u32>()),
            )
            > maximum
        {
            return Err(limit());
        }
        let id = self.catalog.cell_formats.len() as u32;
        self.payload_bytes = self.payload_bytes.saturating_add(format.heap_bytes());
        self.catalog.cell_formats.push(format);
        self.formats.insert(hash, id, prepared);
        Ok(StyleId::new(id))
    }
    /// Register with the registry's configured retained allowance.
    pub fn register(&mut self, style: CellStyle) -> Result<StyleId> {
        self.register_with_limit(style, self.limits.max_bytes)
    }
    /// Register under a smaller aggregate operation allowance. Failed validation
    /// or reservation changes no logical component/format identities; capacity
    /// reserved before an allocation failure may remain accounted and reusable.
    pub fn register_with_limit(&mut self, style: CellStyle, maximum: usize) -> Result<StyleId> {
        let maximum = maximum.min(self.limits.max_bytes);
        if self.memory_bytes() > maximum {
            return Err(limit());
        }
        if let Fill::Gradient(gradient) = &style.fill {
            if gradient
                .stops
                .len()
                .saturating_mul(size_of::<u64>())
                .saturating_add(self.memory_bytes())
                > maximum
            {
                return Err(limit());
            }
        }
        if style.heap_bytes() > maximum {
            return Err(limit());
        }
        style.validate()?;
        let font = self.fonts.find(&self.catalog.fonts, &style.font);
        let fill = self.fills.find(&self.catalog.fills, &style.fill);
        let border = self.borders.find(&self.catalog.borders, &style.borders);
        let number = self.number_id_for_code(&style.number_format);
        let number_id = match number {
            Some(id) => id,
            None => self.available_number_id()?,
        };
        let mut format = CellFormat {
            number_format_id: number_id,
            font_id: font.unwrap_or(self.catalog.fonts.len() as u32),
            fill_id: fill.unwrap_or(self.catalog.fills.len() as u32),
            border_id: border.unwrap_or(self.catalog.borders.len() as u32),
            base_format_id: (!self.catalog.base_formats.is_empty()).then_some(0),
            apply_number_format: Some(true),
            apply_font: Some(true),
            apply_fill: Some(true),
            apply_border: Some(true),
            apply_alignment: Some(true),
            apply_protection: Some(true),
            alignment: None,
            protection: Some(style.protection),
            ..Default::default()
        };
        let key = format_key(&format, Some(&style.alignment));
        let format_hash = fingerprint(&key);
        if let Some(id) = self.formats.find_by(format_hash, |id| {
            let existing = &self.catalog.cell_formats[id as usize];
            format_key(existing, existing.alignment.as_deref()) == key
        }) {
            return Ok(StyleId::new(id));
        }
        let records = self.limits.max_records;
        let mut retained =
            size_of::<crate::Alignment>().saturating_add(self.formats.growth(format_hash));
        let mut geometric_slots = vector_growth(&self.catalog.cell_formats, records, true);
        let mut exact_slots = vector_growth(&self.catalog.cell_formats, records, false);
        if font.is_none() {
            retained = retained
                .saturating_add(style.font.name.as_ref().map_or(0, |name| name.len()))
                .saturating_add(self.fonts.growth(fingerprint(&style.font)));
            geometric_slots =
                geometric_slots.saturating_add(vector_growth(&self.catalog.fonts, records, true));
            exact_slots =
                exact_slots.saturating_add(vector_growth(&self.catalog.fonts, records, false));
        }
        if fill.is_none() {
            retained = retained
                .saturating_add(style.fill.heap_bytes())
                .saturating_add(self.fills.growth(fingerprint(&style.fill)));
            geometric_slots =
                geometric_slots.saturating_add(vector_growth(&self.catalog.fills, records, true));
            exact_slots =
                exact_slots.saturating_add(vector_growth(&self.catalog.fills, records, false));
        }
        if border.is_none() {
            retained = retained.saturating_add(self.borders.growth(fingerprint(&style.borders)));
            geometric_slots =
                geometric_slots.saturating_add(vector_growth(&self.catalog.borders, records, true));
            exact_slots =
                exact_slots.saturating_add(vector_growth(&self.catalog.borders, records, false));
        }
        if number.is_none() {
            retained = retained
                .saturating_add(style.number_format.len())
                .saturating_add(self.numbers.growth(fingerprint(&style.number_format)));
            geometric_slots = geometric_slots.saturating_add(vector_growth(
                &self.catalog.number_formats,
                records,
                true,
            ));
            exact_slots = exact_slots.saturating_add(vector_growth(
                &self.catalog.number_formats,
                records,
                false,
            ));
        }
        let geometric = self
            .memory_bytes()
            .saturating_add(retained)
            .saturating_add(geometric_slots)
            <= maximum;
        if !geometric
            && self
                .memory_bytes()
                .saturating_add(retained)
                .saturating_add(exact_slots)
                > maximum
        {
            return Err(limit());
        }
        // Reserve every affected table/index before committing logical records.
        reserve(&mut self.catalog.cell_formats, records, geometric)?;
        let format_index = self.formats.reserve(format_hash)?;
        let font_hash = fingerprint(&style.font);
        let font_index = if font.is_none() {
            reserve(&mut self.catalog.fonts, records, geometric)?;
            Some(self.fonts.reserve(font_hash)?)
        } else {
            None
        };
        let fill_hash = fingerprint(&style.fill);
        let fill_index = if fill.is_none() {
            reserve(&mut self.catalog.fills, records, geometric)?;
            Some(self.fills.reserve(fill_hash)?)
        } else {
            None
        };
        let border_hash = fingerprint(&style.borders);
        let border_index = if border.is_none() {
            reserve(&mut self.catalog.borders, records, geometric)?;
            Some(self.borders.reserve(border_hash)?)
        } else {
            None
        };
        let number_hash = fingerprint(&style.number_format);
        let number_index = if number.is_none() {
            reserve(&mut self.catalog.number_formats, records, geometric)?;
            Some(self.numbers.reserve(number_hash)?)
        } else {
            None
        };
        let pending_ids_bytes = [
            font_index.as_ref(),
            fill_index.as_ref(),
            border_index.as_ref(),
            number_index.as_ref(),
        ]
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_ref())
        .map(|v| v.capacity() * size_of::<u32>())
        .sum::<usize>()
            + format_index
                .as_ref()
                .map_or(0, |v| v.capacity() * size_of::<u32>());
        let payload = size_of::<crate::Alignment>()
            + usize::from(font.is_none()) * style.font.name.as_ref().map_or(0, |name| name.len())
            + usize::from(fill.is_none()) * style.fill.heap_bytes()
            + usize::from(number.is_none()) * style.number_format.len();
        if self
            .memory_bytes()
            .saturating_add(payload)
            .saturating_add(pending_ids_bytes)
            > maximum
        {
            return Err(limit());
        }
        self.payload_bytes = self.payload_bytes.checked_add(payload).ok_or_else(limit)?;
        if let Some(index) = font_index {
            self.catalog.fonts.push(style.font);
            self.fonts.insert(font_hash, format.font_id, index);
        }
        if let Some(index) = fill_index {
            self.catalog.fills.push(style.fill);
            self.fills.insert(fill_hash, format.fill_id, index);
        }
        if let Some(index) = border_index {
            self.catalog.borders.push(style.borders);
            self.borders.insert(border_hash, format.border_id, index);
        }
        if let Some(index) = number_index {
            self.insert_number_record(number_hash, number_id, style.number_format, index);
        }
        let id = self.catalog.cell_formats.len() as u32;
        format.alignment = Some(Box::new(style.alignment));
        self.catalog.cell_formats.push(format);
        self.formats.insert(format_hash, id, format_index);
        Ok(StyleId::new(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprint_collisions_still_compare_canonical_values() -> Result<()> {
        let mut index = Index::default();
        let values = [2u32, 1u32];
        let hash = fingerprint(&1u32);
        let first = index.reserve(hash)?;
        index.insert(hash, 0, first);
        assert_eq!(index.find(&values, &1u32), None);
        let second = index.reserve(hash)?;
        index.insert(hash, 1, second);
        assert_eq!(index.find(&values, &1u32), Some(1));
        Ok(())
    }
}

#[cfg(test)]
mod accounting_tests {
    use super::*;
    #[test]
    fn incremental_storage_ledger_matches_actual_catalog_and_index_capacities() -> Result<()> {
        let mut registry = StyleRegistry::new(StyleLimits::default())?;
        for index in 0..200u32 {
            let mut style = CellStyle {
                number_format: format!("0.{index:04}").into(),
                ..Default::default()
            };
            style.font.name = Some(format!("Face {}", index % 4).into());
            registry.register(style)?;
            let indices = [
                &registry.fonts,
                &registry.fills,
                &registry.borders,
                &registry.numbers,
                &registry.formats,
            ];
            for value in indices {
                assert_eq!(
                    value.ids_bytes,
                    value
                        .values
                        .values()
                        .map(|ids| ids.capacity() * size_of::<u32>())
                        .sum::<usize>()
                );
            }
            let slow = size_of::<StyleRegistry>() + registry.catalog.memory_bytes()
                - size_of::<StyleCatalog>()
                + indices.into_iter().map(Index::heap_bytes).sum::<usize>();
            assert_eq!(registry.memory_bytes(), slow);
        }
        Ok(())
    }
}
