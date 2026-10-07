//! Named operations for the canonical StyleRegistry owner.
use super::*;

impl StyleRegistry {
    /// Resolve a named-style declaration without copying its name or components.
    pub fn named_style(&self, name: &str) -> Option<&NamedStyle> {
        let id = self.names.find_by(fingerprint(&name), |id| {
            self.catalog.named_styles[id as usize].name.as_ref() == name
        })?;
        self.catalog.named_styles.get(id as usize)
    }
    /// Resolve a name into a shared cell format, retaining the original base identity.
    pub fn named_style_format_with_limit(&mut self, name: &str, maximum: usize) -> Result<StyleId> {
        let named = self
            .named_style(name)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown named style"))?;
        let base = named.base_format_id;
        let mut format = self
            .catalog
            .base_formats
            .get(base as usize)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Missing named base format"))?
            .clone();
        format.base_format_id = Some(base);
        self.register_format_with_limit(format, maximum)
    }
    /// Rename or update named-style metadata without touching appearance tables.
    pub fn update_named_metadata_with_limit(
        &mut self,
        name: &str,
        new_name: Box<str>,
        options: crate::NamedStyleOptions,
        maximum: usize,
    ) -> Result<()> {
        let old_hash = fingerprint(&name);
        let id = self
            .names
            .find_by(old_hash, |id| {
                self.catalog.named_styles[id as usize].name.as_ref() == name
            })
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown named style"))?;
        let renamed = new_name.as_ref() != name;
        if renamed && self.named_style(&new_name).is_some() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Named style already exists",
            ));
        }
        if new_name.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Named style requires a nonempty name",
            ));
        }
        let new_hash = fingerprint(&new_name.as_ref());
        let old_bytes = self.catalog.named_styles[id as usize].name.len();
        let new_bytes = new_name.len();
        let delta = new_bytes.saturating_sub(old_bytes);
        let maximum = maximum.min(self.limits.max_bytes);
        let reindex = renamed && old_hash != new_hash;
        let growth = if reindex {
            self.names.growth(new_hash)
        } else {
            0
        };
        if self
            .memory_bytes()
            .saturating_add(delta)
            .saturating_add(growth)
            > maximum
        {
            return Err(limit());
        }
        let prepared = if reindex {
            self.names.reserve(new_hash)?
        } else {
            None
        };
        let pending = prepared
            .as_ref()
            .map_or(0, |ids| ids.capacity() * size_of::<u32>());
        if self
            .memory_bytes()
            .saturating_add(delta)
            .saturating_add(pending)
            > maximum
        {
            return Err(limit());
        }
        if reindex {
            let empty = if let Some(ids) = self.names.values.get_mut(&old_hash) {
                ids.retain(|value| *value != id);
                ids.is_empty()
            } else {
                false
            };
            if empty && let Some(ids) = self.names.values.remove(&old_hash) {
                self.names.ids_bytes = self
                    .names
                    .ids_bytes
                    .saturating_sub(ids.capacity() * size_of::<u32>());
            }
        }
        let named = &mut self.catalog.named_styles[id as usize];
        if renamed {
            named.name = new_name;
            self.payload_bytes = self
                .payload_bytes
                .saturating_sub(old_bytes)
                .saturating_add(new_bytes);
        }
        named.builtin_id = options.builtin_id;
        named.custom_builtin = options.custom_builtin;
        named.hidden = options.hidden;
        named.outline_level = options.outline_level;
        if reindex {
            self.names.insert(new_hash, id, prepared);
        }
        Ok(())
    }
    /// Replace a named style's base appearance; existing cell formats keep their IDs.
    pub fn update_named_style_with_limit(
        &mut self,
        name: &str,
        style: StyleId,
        maximum: usize,
    ) -> Result<StyleId> {
        let base_id = self
            .named_style(name)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown named style"))?
            .base_format_id;
        let source = self
            .catalog
            .cell_format(style)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown named appearance format"))?;
        if source.unmodeled_extensions {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Unmodeled named style extensions require source preservation",
            ));
        }
        let mut base = source.clone();
        base.base_format_id = None;
        let old_bytes = self
            .catalog
            .base_formats
            .get(base_id as usize)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Missing named base format"))?
            .heap_bytes();
        let new_bytes = base.heap_bytes();
        let reserve = new_bytes.saturating_sub(old_bytes);
        let maximum = maximum.min(self.limits.max_bytes);
        if self.memory_bytes().saturating_add(reserve) > maximum {
            return Err(limit());
        }
        let mut format = source.clone();
        format.base_format_id = Some(base_id);
        let id = self.register_format_with_limit(format, maximum.saturating_sub(reserve))?;
        self.catalog.base_formats[base_id as usize] = base;
        self.payload_bytes = self
            .payload_bytes
            .saturating_sub(old_bytes)
            .saturating_add(new_bytes);
        Ok(id)
    }
    /// Register a unique name and base-format link without duplicating appearance tables.
    /// Allocation failures retain charged capacities but roll back logical named records.
    pub fn register_named_style_with_limit(
        &mut self,
        name: Box<str>,
        style: StyleId,
        options: crate::NamedStyleOptions,
        maximum: usize,
    ) -> Result<StyleId> {
        if self.named_style(&name).is_some() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Named style already exists",
            ));
        }
        let maximum = maximum.min(self.limits.max_bytes);
        let source = self
            .catalog
            .cell_format(style)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Unknown named appearance format"))?;
        if source.unmodeled_extensions {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Unmodeled named style extensions require source preservation",
            ));
        }
        let mut base = source.clone();
        base.base_format_id = None;
        let base_id = self.catalog.base_formats.len() as u32;
        let named_id = self.catalog.named_styles.len() as u32;
        let mut format = source.clone();
        format.base_format_id = Some(base_id);
        let hash = fingerprint(&name.as_ref());
        let payload = name.len().saturating_add(base.heap_bytes());
        let growth = vector_growth(&self.catalog.base_formats, self.limits.max_records, false)
            .saturating_add(vector_growth(
                &self.catalog.named_styles,
                self.limits.max_records,
                false,
            ))
            .saturating_add(self.names.growth(hash));
        if self
            .memory_bytes()
            .saturating_add(payload)
            .saturating_add(growth)
            > maximum
        {
            return Err(limit());
        }
        reserve(
            &mut self.catalog.base_formats,
            self.limits.max_records,
            false,
        )?;
        reserve(
            &mut self.catalog.named_styles,
            self.limits.max_records,
            false,
        )?;
        let prepared = self.names.reserve(hash)?;
        let pending = prepared
            .as_ref()
            .map_or(0, |ids| ids.capacity() * size_of::<u32>());
        if self
            .memory_bytes()
            .saturating_add(payload)
            .saturating_add(pending)
            > maximum
        {
            return Err(limit());
        }
        self.catalog.base_formats.push(base);
        self.catalog.named_styles.push(NamedStyle {
            name,
            base_format_id: base_id,
            builtin_id: options.builtin_id,
            custom_builtin: options.custom_builtin,
            hidden: options.hidden,
            outline_level: options.outline_level,
        });
        self.payload_bytes = self.payload_bytes.saturating_add(payload);
        match self.register_format_with_limit(format, maximum.saturating_sub(pending)) {
            Ok(id) => {
                self.names.insert(hash, named_id, prepared);
                Ok(id)
            }
            Err(error) => {
                self.catalog.base_formats.pop();
                self.catalog.named_styles.pop();
                self.payload_bytes = self.payload_bytes.saturating_sub(payload);
                Err(error)
            }
        }
    }
}
