//! Formats operations for the canonical StyleRegistry owner.
use super::*;

impl StyleRegistry {
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
    pub(super) fn number_id_for_code(&self, code: &str) -> Option<u32> {
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
    pub(super) fn insert_number_record(
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
    pub(super) fn number_format_variant_key(
        &self,
        base: StyleId,
        number_format_id: u32,
    ) -> Result<FormatKey<'_>> {
        let source = self
            .catalog
            .cell_format(base)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown base cell format"))?;
        if self.catalog.number_format(number_format_id).is_none() {
            return Err(Error::new(ErrorKind::InvalidData, "Unknown number format"));
        }
        let mut key = format_key(source, source.alignment.as_deref());
        key.number_format_id = number_format_id;
        key.apply_number_format = Some(true);
        Ok(key)
    }
    /// Find a number-format override without cloning shared component payloads or alignment.
    pub fn find_format_with_number_format(
        &self,
        base: StyleId,
        number_format_id: u32,
    ) -> Result<Option<StyleId>> {
        let key = self.number_format_variant_key(base, number_format_id)?;
        Ok(self
            .formats
            .find_by(fingerprint(&key), |id| {
                let existing = &self.catalog.cell_formats[id as usize];
                format_key(existing, existing.alignment.as_deref()) == key
            })
            .map(StyleId::new))
    }
    /// Replace one component without copying any unrelated component payloads.
    /// Checked interned components remain reusable if subsequent format growth fails.
    pub fn derive_component_with_limit(
        &mut self,
        base: StyleId,
        component: crate::StyleComponent,
        maximum: usize,
    ) -> Result<StyleId> {
        let maximum = maximum.min(self.limits.max_bytes);
        if self.memory_bytes() > maximum {
            return Err(limit());
        }
        self.catalog
            .cell_format(base)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown base cell format"))?;
        let change = match component {
            crate::StyleComponent::Font(value) => {
                value.validate()?;
                let heap = value.heap_bytes();
                let other = self
                    .memory_bytes()
                    .saturating_sub(self.catalog.fonts.capacity() * size_of::<crate::Font>())
                    .saturating_sub(self.fonts.heap_bytes());
                let (id, inserted) = intern_component(
                    &mut self.catalog.fonts,
                    &mut self.fonts,
                    *value,
                    heap,
                    self.limits,
                    maximum,
                    other,
                )?;
                if inserted {
                    self.payload_bytes = self.payload_bytes.saturating_add(heap);
                }
                ComponentChange::Font(id)
            }
            crate::StyleComponent::Fill(value) => {
                if value.heap_bytes() > maximum {
                    return Err(limit());
                }
                value.validate()?;
                let heap = value.heap_bytes();
                let other = self
                    .memory_bytes()
                    .saturating_sub(self.catalog.fills.capacity() * size_of::<crate::Fill>())
                    .saturating_sub(self.fills.heap_bytes());
                let (id, inserted) = intern_component(
                    &mut self.catalog.fills,
                    &mut self.fills,
                    *value,
                    heap,
                    self.limits,
                    maximum,
                    other,
                )?;
                if inserted {
                    self.payload_bytes = self.payload_bytes.saturating_add(heap);
                }
                ComponentChange::Fill(id)
            }
            crate::StyleComponent::Border(value) => {
                value.validate()?;
                let heap = value.heap_bytes();
                let other = self
                    .memory_bytes()
                    .saturating_sub(self.catalog.borders.capacity() * size_of::<crate::Border>())
                    .saturating_sub(self.borders.heap_bytes());
                let (id, inserted) = intern_component(
                    &mut self.catalog.borders,
                    &mut self.borders,
                    *value,
                    heap,
                    self.limits,
                    maximum,
                    other,
                )?;
                if inserted {
                    self.payload_bytes = self.payload_bytes.saturating_add(heap);
                }
                ComponentChange::Border(id)
            }
            crate::StyleComponent::Alignment(value) => {
                if let Some(value) = &value {
                    value.validate()?;
                }
                ComponentChange::Alignment(value)
            }
            crate::StyleComponent::Protection(value) => ComponentChange::Protection(value),
        };
        let source = self
            .catalog
            .cell_format(base)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown base cell format"))?;
        let key = change.key(source);
        if let Some(id) = self.formats.find_by(fingerprint(&key), |id| {
            let existing = &self.catalog.cell_formats[id as usize];
            format_key(existing, existing.alignment.as_deref()) == key
        }) {
            return Ok(StyleId::new(id));
        }
        let mut format = source.clone();
        change.apply(&mut format);
        self.register_format_with_limit(format, maximum)
    }
    /// Intern the canonical automatic presets using the source normal format's
    /// shared components. Default catalogs obtain the stable IDs 1 through 4;
    /// imported catalogs retain their existing IDs. Valid interned records may
    /// remain after a later budget failure.
    pub fn register_temporal_presets_with_limit(
        &mut self,
        maximum: usize,
    ) -> Result<TemporalStyleIds> {
        self.catalog.cell_format(StyleId::new(0)).ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "Normal cell format is unavailable")
        })?;
        let mut register = |kind: crate::DateKind| -> Result<StyleId> {
            let code = kind.default_number_format();
            let number = match self.number_id_for_code(code) {
                Some(id) => id,
                None => self.register_number_format_with_limit(code.into(), maximum)?,
            };
            self.register_format_with_number_format_limit(StyleId::new(0), number, maximum)
        };
        Ok(TemporalStyleIds {
            datetime: register(crate::DateKind::DateTime)?,
            time: register(crate::DateKind::Time)?,
            duration: register(crate::DateKind::Duration)?,
            date: register(crate::DateKind::Date)?,
        })
    }
    /// Resolve temporal assignment through the shared source format. Existing
    /// date/duration codes remain unchanged; other codes gain a shared variant.
    /// A failed later variant insertion may retain its valid interned code.
    pub fn register_temporal_format_with_limit(
        &mut self,
        base: StyleId,
        kind: crate::DateKind,
        maximum: usize,
    ) -> Result<StyleId> {
        let format = self
            .catalog
            .cell_format(base)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Cell format is unavailable"))?;
        if self
            .catalog
            .number_format(format.number_format_id)
            .and_then(crate::classify_number_format)
            .is_some()
        {
            if self.memory_bytes() > maximum.min(self.limits.max_bytes) {
                return Err(limit());
            }
            return Ok(base);
        }
        let code = kind.default_number_format();
        let number = match self.number_id_for_code(code) {
            Some(id) => id,
            None => self.register_number_format_with_limit(code.into(), maximum)?,
        };
        self.register_format_with_number_format_limit(base, number, maximum)
    }
    /// Intern a number-format override, retaining all other format properties and
    /// component IDs. The override explicitly applies its number format.
    pub fn register_format_with_number_format(
        &mut self,
        base: StyleId,
        number_format_id: u32,
    ) -> Result<StyleId> {
        self.register_format_with_number_format_limit(base, number_format_id, self.limits.max_bytes)
    }
    /// Intern an override under a smaller aggregate allowance. Existing variants
    /// use a borrowed collision-checked key; only a new record clones alignment.
    pub fn register_format_with_number_format_limit(
        &mut self,
        base: StyleId,
        number_format_id: u32,
        maximum: usize,
    ) -> Result<StyleId> {
        let maximum = maximum.min(self.limits.max_bytes);
        if self.memory_bytes() > maximum {
            return Err(limit());
        }
        if let Some(id) = self.find_format_with_number_format(base, number_format_id)? {
            return Ok(id);
        }
        let source = self
            .catalog
            .cell_format(base)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown base cell format"))?;
        if self.memory_bytes().saturating_add(source.heap_bytes()) > maximum {
            return Err(limit());
        }
        let mut format = source.clone();
        format.number_format_id = number_format_id;
        format.apply_number_format = Some(true);
        self.register_format_with_limit(format, maximum)
    }
}
