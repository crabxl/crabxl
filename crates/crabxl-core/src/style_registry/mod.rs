//! Bounded canonical component interning for owned workbook styles.
mod formats;
mod import;
mod named;

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
fn validate_table_limits(catalog: &StyleCatalog, limits: StyleLimits) -> Result<()> {
    if limits.max_bytes == 0 || limits.max_records == 0 || limits.max_records > u32::MAX as usize {
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
        catalog.differential_styles.len(),
        catalog.table_styles.as_ref().map_or(0, |v| v.styles.len()),
    ] {
        if count > limits.max_records {
            return Err(limit());
        }
    }
    if catalog.table_styles.as_ref().is_some_and(|tables| {
        tables
            .styles
            .iter()
            .any(|style| style.elements.len() > limits.max_records)
    }) {
        return Err(limit());
    }
    Ok(())
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
    pub(super) fn find_by(&self, hash: u64, mut equals: impl FnMut(u32) -> bool) -> Option<u32> {
        self.values
            .get(&hash)?
            .iter()
            .copied()
            .find(|id| equals(*id))
    }
    pub(super) fn find<T: Hash + PartialEq>(&self, values: &[T], value: &T) -> Option<u32> {
        self.find_by(fingerprint(value), |id| values[id as usize] == *value)
    }
    pub(super) fn heap_bytes(&self) -> usize {
        bucket_bytes(self.values.capacity()).saturating_add(self.ids_bytes)
    }
    pub(super) fn growth(&self, hash: u64) -> usize {
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
    pub(super) fn reserve(&mut self, hash: u64) -> Result<Option<Vec<u32>>> {
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
    pub(super) fn insert(&mut self, hash: u64, id: u32, prepared: Option<Vec<u32>>) {
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
// Intern one changed component while retaining the existing shared tables.
fn intern_component<T: Hash + PartialEq>(
    values: &mut Vec<T>,
    index: &mut Index,
    value: T,
    heap: usize,
    limits: StyleLimits,
    maximum: usize,
    other: usize,
) -> Result<(u32, bool)> {
    if let Some(id) = index.find(values, &value) {
        return Ok((id, false));
    }
    let hash = fingerprint(&value);
    let retained = other
        .saturating_add(values.capacity() * size_of::<T>())
        .saturating_add(index.heap_bytes())
        .saturating_add(heap)
        .saturating_add(index.growth(hash));
    let geometric =
        retained.saturating_add(vector_growth(values, limits.max_records, true)) <= maximum;
    if retained.saturating_add(vector_growth(values, limits.max_records, geometric)) > maximum {
        return Err(limit());
    }
    reserve(values, limits.max_records, geometric)?;
    let prepared = index.reserve(hash)?;
    let actual = other
        .saturating_add(values.capacity() * size_of::<T>())
        .saturating_add(index.heap_bytes())
        .saturating_add(heap)
        .saturating_add(
            prepared
                .as_ref()
                .map_or(0, |ids| ids.capacity() * size_of::<u32>()),
        );
    if actual > maximum {
        return Err(limit());
    }
    let id = values.len() as u32;
    values.push(value);
    index.insert(hash, id, prepared);
    Ok((id, true))
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
    Named,
}
/// Canonical workbook-local temporal presets. IDs are resolved from each
/// source catalog rather than assuming that imported records occupy 1..=4.
#[derive(Clone, Copy, Debug)]
pub struct TemporalStyleIds {
    /// Calendar datetime preset.
    pub datetime: StyleId,
    /// Clock preset.
    pub time: StyleId,
    /// Elapsed duration preset.
    pub duration: StyleId,
    /// Calendar date preset.
    pub date: StyleId,
}
impl TemporalStyleIds {
    /// Resolve one temporal kind to its workbook-local style identity.
    pub const fn for_kind(self, kind: crate::DateKind) -> StyleId {
        match kind {
            crate::DateKind::Date => self.date,
            crate::DateKind::DateTime => self.datetime,
            crate::DateKind::Time => self.time,
            crate::DateKind::Duration => self.duration,
        }
    }
}
enum ComponentChange {
    Font(u32),
    Fill(u32),
    Border(u32),
    Alignment(Option<Box<crate::Alignment>>),
    Protection(Option<crate::Protection>),
}
impl ComponentChange {
    pub(super) fn key<'a>(&'a self, source: &'a CellFormat) -> FormatKey<'a> {
        let mut key = format_key(source, source.alignment.as_deref());
        match self {
            Self::Font(id) => {
                key.font_id = *id;
                key.apply_font = Some(true);
            }
            Self::Fill(id) => {
                key.fill_id = *id;
                key.apply_fill = Some(true);
            }
            Self::Border(id) => {
                key.border_id = *id;
                key.apply_border = Some(true);
            }
            Self::Alignment(value) => {
                key.alignment = value.as_deref();
                key.apply_alignment = Some(true);
            }
            Self::Protection(value) => {
                key.protection = *value;
                key.apply_protection = Some(true);
            }
        }
        key
    }
    pub(super) fn apply(self, format: &mut CellFormat) {
        match self {
            Self::Font(id) => {
                format.font_id = id;
                format.apply_font = Some(true);
            }
            Self::Fill(id) => {
                format.fill_id = id;
                format.apply_fill = Some(true);
            }
            Self::Border(id) => {
                format.border_id = id;
                format.apply_border = Some(true);
            }
            Self::Alignment(value) => {
                format.alignment = value;
                format.apply_alignment = Some(true);
            }
            Self::Protection(value) => {
                format.protection = value;
                format.apply_protection = Some(true);
            }
        }
    }
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
    names: Index,
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
            names: Index::default(),
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
        registry.index_imported(IndexKind::Named, fingerprint(&"Normal"), 0)?;
        let normal = registry.register_with_limit(
            CellStyle::default(),
            limits
                .max_bytes
                .saturating_sub(size_of::<crate::Alignment>()),
        )?;
        let source = registry
            .catalog
            .cell_format(normal)
            .ok_or_else(|| Error::new(ErrorKind::InvalidState, "Missing normal format"))?;
        let heap = source.heap_bytes();
        if registry.memory_bytes().saturating_add(heap) > limits.max_bytes {
            return Err(limit());
        }
        let mut base = source.clone();
        base.base_format_id = None;
        registry.catalog.base_formats[0] = base;
        registry.payload_bytes = registry.payload_bytes.saturating_add(heap);
        Ok(registry)
    }
    /// Change future registration allowances without reallocating or changing IDs.
    /// Existing retained storage and each actual table must fit before the update.
    pub fn set_limits(&mut self, limits: StyleLimits) -> Result<()> {
        validate_table_limits(&self.catalog, limits)?;
        if self.memory_bytes() > limits.max_bytes {
            return Err(limit());
        }
        self.limits = limits;
        Ok(())
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
            + catalog.indexed_colors.capacity() * size_of::<crate::ArgbLiteral>()
            + catalog.recent_colors.capacity() * size_of::<crate::Color>()
            + catalog.unmodeled_sections.capacity() * size_of::<Box<str>>()
            + catalog.differential_styles.capacity() * size_of::<crate::DifferentialStyle>()
            + catalog
                .table_styles
                .as_ref()
                .map_or(0, |_| size_of::<crate::TableStyleCatalog>());
        size_of::<Self>()
            .saturating_add(capacities)
            .saturating_add(self.reserved_number_ids.capacity() * size_of::<u32>())
            .saturating_add(self.payload_bytes)
            .saturating_add(self.fonts.heap_bytes())
            .saturating_add(self.fills.heap_bytes())
            .saturating_add(self.borders.heap_bytes())
            .saturating_add(self.numbers.heap_bytes())
            .saturating_add(self.formats.heap_bytes())
            .saturating_add(self.names.heap_bytes())
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
        if let Fill::Gradient(gradient) = &style.fill
            && gradient
                .stops
                .len()
                .saturating_mul(size_of::<u64>())
                .saturating_add(self.memory_bytes())
                > maximum
        {
            return Err(limit());
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
                .saturating_add(style.font.heap_bytes())
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
            retained = retained
                .saturating_add(style.borders.heap_bytes())
                .saturating_add(self.borders.growth(fingerprint(&style.borders)));
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
            + usize::from(font.is_none()) * style.font.heap_bytes()
            + usize::from(fill.is_none()) * style.fill.heap_bytes()
            + usize::from(border.is_none()) * style.borders.heap_bytes()
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
    pub(super) fn fingerprint_collisions_still_compare_canonical_values() -> Result<()> {
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
    pub(super) fn incremental_storage_ledger_matches_actual_catalog_and_index_capacities()
    -> Result<()> {
        let mut registry = StyleRegistry::new(StyleLimits::default())?;
        for index in 0..200u32 {
            let mut style = CellStyle {
                number_format: format!("0.{index:04}").into(),
                ..Default::default()
            };
            style.font.name = Some(format!("Face {}", index % 4).into());
            let base = registry.register(style)?;
            let components = [
                crate::StyleComponent::Font(Box::new(crate::Font {
                    name: Some(format!("Replacement {}", index % 3).into()),
                    ..Default::default()
                })),
                crate::StyleComponent::Fill(Box::default()),
                crate::StyleComponent::Border(Box::default()),
                crate::StyleComponent::Alignment(Some(Box::new(crate::Alignment {
                    wrap_text: Some(true),
                    ..Default::default()
                }))),
                crate::StyleComponent::Protection(Some(crate::Protection {
                    locked: Some(false),
                    hidden: Some(true),
                })),
            ];
            for component in components {
                let source = registry
                    .catalog
                    .cell_format(base)
                    .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Missing registered base"))?
                    .clone();
                let derived =
                    registry.derive_component_with_limit(base, component.clone(), usize::MAX)?;
                let bytes = registry.memory_bytes();
                assert_eq!(
                    registry.derive_component_with_limit(base, component, usize::MAX,)?,
                    derived
                );
                assert_eq!(registry.memory_bytes(), bytes);
                let result = registry
                    .catalog
                    .cell_format(derived)
                    .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Missing derived format"))?;
                assert_eq!(source.number_format_id, result.number_format_id);
                assert_eq!(source.base_format_id, result.base_format_id);
            }
            let indices = [
                &registry.fonts,
                &registry.fills,
                &registry.borders,
                &registry.numbers,
                &registry.formats,
                &registry.names,
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
