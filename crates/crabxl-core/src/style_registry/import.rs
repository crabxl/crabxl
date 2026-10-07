//! Import operations for the canonical StyleRegistry owner.
use super::*;

impl StyleRegistry {
    /// Adopt existing tables without remapping component, format or source IDs.
    /// Duplicate components remain present; new registration reuses the first equal record.
    /// Source custom-number IDs are sparse and never size a dense allocation.
    pub fn from_catalog(mut catalog: StyleCatalog, limits: StyleLimits) -> Result<Self> {
        validate_table_limits(&catalog, limits)?;
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
                .map(crate::Font::heap_bytes)
                .sum::<usize>()
            + catalog.fills.iter().map(Fill::heap_bytes).sum::<usize>()
            + catalog
                .borders
                .iter()
                .map(crate::Border::heap_bytes)
                .sum::<usize>()
            + catalog
                .recent_colors
                .iter()
                .map(crate::Color::heap_bytes)
                .sum::<usize>()
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
                .sum::<usize>()
            + catalog
                .differential_styles
                .iter()
                .map(crate::DifferentialStyle::heap_bytes)
                .sum::<usize>()
            + catalog.table_styles.as_ref().map_or(0, |v| v.heap_bytes());
        let mut value = Self {
            catalog,
            fonts: Index::default(),
            fills: Index::default(),
            borders: Index::default(),
            numbers: Index::default(),
            formats: Index::default(),
            names: Index::default(),
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
        for style in &value.catalog.differential_styles {
            let scratch = match style.fill.as_deref() {
                Some(Fill::Gradient(gradient)) => {
                    gradient.stops.len().saturating_mul(size_of::<u64>())
                }
                _ => 0,
            };
            if value.memory_bytes().saturating_add(scratch) > limits.max_bytes {
                return Err(limit());
            }
            style.validate()?;
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
            if let Fill::Gradient(v) = &value.catalog.fills[i]
                && value
                    .memory_bytes()
                    .saturating_add(v.stops.len().saturating_mul(size_of::<u64>()))
                    > limits.max_bytes
            {
                return Err(limit());
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
        for i in 0..value.catalog.named_styles.len() {
            value.index_imported(
                IndexKind::Named,
                fingerprint(&value.catalog.named_styles[i].name.as_ref()),
                i as u32,
            )?;
        }
        Ok(value)
    }
    pub(super) fn index_imported(&mut self, kind: IndexKind, hash: u64, id: u32) -> Result<()> {
        let index = match kind {
            IndexKind::Font => &self.fonts,
            IndexKind::Fill => &self.fills,
            IndexKind::Border => &self.borders,
            IndexKind::Number => &self.numbers,
            IndexKind::Format => &self.formats,
            IndexKind::Named => &self.names,
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
            IndexKind::Named => &mut self.names,
        };
        let prepared = index.reserve(hash)?;
        index.insert(hash, id, prepared);
        if self.memory_bytes() > self.limits.max_bytes {
            return Err(limit());
        }
        Ok(())
    }
    pub(super) fn available_number_id(&self) -> Result<u32> {
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
}
