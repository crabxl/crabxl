//! Styles operations for the canonical LoadedWorkbook owner.
use super::*;

impl<R: Read + Seek> LoadedWorkbook<R> {
    /// Derive a source cell's number format while retaining its other style fields.
    /// Unknown style extensions and signed packages reject before registration.
    /// Successfully interned formats remain reusable if a later cell edit fails.
    pub fn set_number_format(
        &mut self,
        id: SheetId,
        address: CellAddress,
        code: Box<str>,
    ) -> Result<()> {
        if self.options.read.data_only {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Data-only style editing remains unimplemented",
            ));
        }
        crate::encode::validate_xml_text(&code)?;
        let catalog = self.bank.style_catalog().ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                "Adding a missing source stylesheet remains unimplemented",
            )
        })?;
        self.editor.validate_style_edit(catalog)?;
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if self.sheets[index].original.is_some()
            && !self.editor.model_is_dirty(&self.sheets[index].name)
        {
            self.editor.prepare_model(&self.sheets[index].name)?;
        }
        let style = self.sheet(id)?.style_at(address);
        self.rebalance()?;
        let result = self.bank.derive_number_format(style, code);
        if result.is_ok() {
            self.editor.styles_changed();
        }
        self.rebalance()?;
        self.set_style(id, address, result?)
    }
    /// Borrowed-byte theme access returns shared ownership, not a payload copy.
    pub fn theme(&mut self) -> Result<Option<crabxl_core::Theme>> {
        if self.editor.theme_is_dirty() {
            return Ok(self.bank.theme().cloned());
        }
        self.rebalance()?;
        let theme = self.editor.book.theme()?.cloned();
        self.rebalance()?;
        Ok(theme)
    }
    /// Replace exact theme bytes at the existing relationship target.
    /// Unknown DrawingML sections remain intact; None requests the standard theme.
    /// Missing graph creation remains explicit.
    pub fn set_theme(&mut self, theme: Option<crabxl_core::Theme>) -> Result<()> {
        self.editor.validate_theme_edit()?;
        if theme.as_ref().is_some_and(|theme| {
            theme.bytes().len() > self.options.resources.max_theme_bytes
                || theme.bytes().len() as u64 > self.options.resources.max_part_bytes
        }) {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                "Theme payload exceeds configured limit",
            ));
        }
        // Validate the original entry's CRC before committing its replacement.
        self.editor.book.theme()?;
        self.rebalance()?;
        let result = self.bank.set_theme(theme);
        if result.is_ok() {
            self.editor.theme_changed();
        }
        self.rebalance()?;
        result
    }
    /// Derive one source appearance component without assigning any cell.
    pub fn derive_style_component(
        &mut self,
        style: crabxl_core::StyleId,
        component: crabxl_core::StyleComponent,
    ) -> Result<crabxl_core::StyleId> {
        if let crabxl_core::StyleComponent::Font(font) = &component
            && let Some(name) = &font.name
        {
            crate::encode::validate_xml_text(name)?;
        }
        let catalog = self.bank.style_catalog().ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                "Adding a missing source stylesheet remains unimplemented",
            )
        })?;
        self.editor.validate_style_edit(catalog)?;
        self.rebalance()?;
        let result = self.bank.derive_style_component(style, component);
        if result.is_ok() {
            self.editor.styles_changed();
        }
        self.rebalance()?;
        result
    }
    /// Derive a literal number format without assigning a source cell.
    pub fn derive_number_format(
        &mut self,
        style: crabxl_core::StyleId,
        code: Box<str>,
    ) -> Result<crabxl_core::StyleId> {
        crate::encode::validate_xml_text(&code)?;
        let catalog = self.bank.style_catalog().ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                "Adding a missing source stylesheet remains unimplemented",
            )
        })?;
        self.editor.validate_style_edit(catalog)?;
        self.rebalance()?;
        let result = self.bank.derive_number_format(style, code);
        if result.is_ok() {
            self.editor.styles_changed();
        }
        self.rebalance()?;
        result
    }
    /// Register a named style without changing source cells or component identities.
    pub fn register_named_style(
        &mut self,
        name: Box<str>,
        style: crabxl_core::StyleId,
        options: crabxl_core::NamedStyleOptions,
    ) -> Result<crabxl_core::StyleId> {
        crate::encode::validate_xml_text(&name)?;
        let catalog = self.bank.style_catalog().ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                "Adding a missing source stylesheet remains unimplemented",
            )
        })?;
        self.editor.validate_style_edit(catalog)?;
        self.rebalance()?;
        let result = self.bank.register_named_style(name, style, options);
        if result.is_ok() {
            self.editor.styles_changed();
        }
        self.rebalance()?;
        result
    }
    /// Rename/update source named-style metadata without touching source cell IDs.
    pub fn update_named_metadata(
        &mut self,
        name: &str,
        new_name: Box<str>,
        options: crabxl_core::NamedStyleOptions,
    ) -> Result<()> {
        crate::encode::validate_xml_text(&new_name)?;
        let catalog = self.bank.style_catalog().ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                "Adding a missing source stylesheet remains unimplemented",
            )
        })?;
        self.editor.validate_style_edit(catalog)?;
        self.rebalance()?;
        let result = self.bank.update_named_metadata(name, new_name, options);
        if result.is_ok() {
            self.editor.styles_changed();
        }
        self.rebalance()?;
        result
    }
    /// Update a named source appearance without changing already assigned cell IDs.
    pub fn update_named_style(
        &mut self,
        name: &str,
        style: crabxl_core::StyleId,
    ) -> Result<crabxl_core::StyleId> {
        let catalog = self.bank.style_catalog().ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                "Adding a missing source stylesheet remains unimplemented",
            )
        })?;
        self.editor.validate_style_edit(catalog)?;
        self.rebalance()?;
        let result = self.bank.update_named_style(name, style);
        if result.is_ok() {
            self.editor.styles_changed();
        }
        self.rebalance()?;
        result
    }
    /// Resolve a named source style into a workbook-local cell format.
    pub fn named_style_format(&mut self, name: &str) -> Result<crabxl_core::StyleId> {
        let catalog = self.bank.style_catalog().ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                "Adding a missing source stylesheet remains unimplemented",
            )
        })?;
        self.editor.validate_style_edit(catalog)?;
        self.rebalance()?;
        let result = self.bank.named_style_format(name);
        if result.is_ok() {
            self.editor.styles_changed();
        }
        self.rebalance()?;
        result
    }
    /// Replace one source style component while retaining unrelated fields.
    /// Source graph and signature guards run before registration.
    pub fn set_style_component(
        &mut self,
        id: SheetId,
        address: CellAddress,
        component: crabxl_core::StyleComponent,
    ) -> Result<()> {
        if self.options.read.data_only {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Data-only style editing remains unimplemented",
            ));
        }
        if let crabxl_core::StyleComponent::Font(font) = &component
            && let Some(name) = &font.name
        {
            crate::encode::validate_xml_text(name)?;
        }
        let catalog = self.bank.style_catalog().ok_or_else(|| {
            Error::new(
                ErrorKind::Unsupported,
                "Adding a missing source stylesheet remains unimplemented",
            )
        })?;
        self.editor.validate_style_edit(catalog)?;
        let index = self
            .sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if self.sheets[index].original.is_some()
            && !self.editor.model_is_dirty(&self.sheets[index].name)
        {
            self.editor.prepare_model(&self.sheets[index].name)?;
        }
        let style = self.sheet(id)?.style_at(address);
        self.rebalance()?;
        let result = self.bank.derive_style_component(style, component);
        if result.is_ok() {
            self.editor.styles_changed();
        }
        self.rebalance()?;
        self.assign_style(id, address, result?, false)
    }
    /// Assign an existing workbook-local format without copying the cell value.
    /// Supported source worksheets materialize once and use the canonical save
    /// path. Affected unmodeled graphs reject before mutation. Unknown format
    /// identities reject before materialization; a missing coordinate becomes an
    /// empty styled cell under the same aggregate allowance.
    pub fn set_style(
        &mut self,
        id: SheetId,
        address: CellAddress,
        style: crabxl_core::StyleId,
    ) -> Result<()> {
        self.assign_style(id, address, style, true)
    }
    pub(super) fn assign_style(
        &mut self,
        id: SheetId,
        address: CellAddress,
        style: crabxl_core::StyleId,
        explicit_temporal: bool,
    ) -> Result<()> {
        let valid = self
            .bank
            .style_catalog()
            .map_or(style.get() == 0, |catalog| {
                catalog.cell_format(style).is_some()
            });
        if !valid {
            return Err(
                Error::new(ErrorKind::InvalidData, "Unknown workbook style identity")
                    .with_cell(address),
            );
        }
        if self.options.read.data_only {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Data-only style editing remains unimplemented",
            ));
        }
        let source = self
            .sheets
            .iter()
            .find(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))?;
        if source.original.is_none() || self.editor.model_is_dirty(&source.name) {
            let created = source.original.is_none();
            let result = if explicit_temporal {
                self.bank.sheet_mut(id)?.set_style(address, style)
            } else {
                self.bank
                    .sheet_mut(id)?
                    .set_appearance_style(address, style)
            };
            if result.is_ok() && created {
                self.editor.created_values_dirty(id);
            }
            self.rebalance()?;
            return result;
        }
        self.edit_structure_when(
            id,
            |sheet| {
                let changed = sheet.get(address).is_none_or(|cell| cell.style != style || (explicit_temporal && matches!(&cell.value, CellValue::DateTime(date) if !date.requires_serial_encoding())));
                if explicit_temporal {sheet.set_style(address,style)?;} else {sheet.set_appearance_style(address,style)?;}
                Ok(changed)
            },
            |changed| *changed,
        )?;
        Ok(())
    }
}
