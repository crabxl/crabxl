//! New stylesheet/theme package identities over the existing canonical bank.
use super::*;

pub(super) enum CatalogKind {
    Styles,
    Theme,
}
pub(super) struct CatalogPart {
    pub(super) kind: CatalogKind,
    pub(super) part: String,
    target: String,
    relationship: String,
}
impl CatalogPart {
    pub(super) fn heap_bytes(&self) -> usize {
        self.part.capacity() + self.target.capacity() + self.relationship.capacity()
    }
    pub(super) fn write_graph<W: Write>(
        &self,
        writer: &mut Writer<PartOutput<W>>,
        types: bool,
    ) -> Result<()> {
        let mut element = BytesStart::new(if types { "Override" } else { "Relationship" });
        if types {
            element.push_attribute((
                "xmlns",
                "http://schemas.openxmlformats.org/package/2006/content-types",
            ));
            let name = format!("/{}", self.part);
            let name = quick_xml::escape::escape(&name);
            element.push_attribute(("PartName", name.as_ref()));
            element.push_attribute((
                "ContentType",
                match self.kind {
                    CatalogKind::Styles => {
                        "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"
                    }
                    CatalogKind::Theme => "application/vnd.openxmlformats-officedocument.theme+xml",
                },
            ));
        } else {
            element.push_attribute((
                "xmlns",
                "http://schemas.openxmlformats.org/package/2006/relationships",
            ));
            element.push_attribute(("Id", self.relationship.as_str()));
            let kind = format!(
                "{}/{}",
                crate::xml::OFFICE_REL_URI,
                match self.kind {
                    CatalogKind::Styles => "styles",
                    CatalogKind::Theme => "theme",
                }
            );
            element.push_attribute(("Type", kind.as_str()));
            element.push_attribute(("Target", self.target.as_str()));
        }
        emit(writer, Event::Empty(element))
    }
}

impl<R: Read + Seek> WorkbookEditor<R> {
    pub(super) fn prepare_catalog_parts(
        &mut self,
        bank: Option<&crabxl_core::Workbook>,
        extra: usize,
    ) -> Result<Vec<CatalogPart>> {
        let style = self.styles_dirty && self.book.style_part.is_none();
        let theme = self.theme_dirty && self.book.theme_part.is_none();
        if !style && !theme {
            return Ok(Vec::new());
        }
        if self.signed {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Signed catalog graph creation remains unimplemented",
            ));
        }
        if self
            .book
            .archive
            .index_for_name(&self.workbook_relationships)
            .is_none()
        {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Creating a missing workbook relationship root remains unimplemented",
            ));
        }
        let maximum = self
            .allowance
            .retained_data_bytes
            .saturating_sub(self.retained_package_bytes())
            .saturating_sub(bank.map_or(0, crabxl_core::Workbook::charged_bytes))
            .saturating_sub(extra)
            .min(
                self.options
                    .resources
                    .max_metadata_bytes
                    .min(usize::MAX as u64) as usize,
            );
        let mut remaining = maximum as u64;
        let relationships = crate::package::read_relationships(
            &mut self.book.archive,
            &self.workbook_relationships,
            self.options.resources,
            &mut remaining,
        )?;
        let directory = self
            .book
            .workbook_part
            .rsplit_once('/')
            .map_or("", |(directory, _)| directory);
        let file = self
            .book
            .archive
            .by_name("[Content_Types].xml")
            .map_err(|cause| zip_error("Cannot inspect catalog content types", cause))?;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(self.options.resources.input_buffer_bytes, file),
            "[Content_Types].xml".into(),
            remaining,
            self.options.resources,
        );
        let mut declared = HashSet::new();
        let mut charged = 0usize;
        let mut root = false;
        loop {
            let frame = xml.next()?;
            check_declaration(&frame.event)?;
            match frame.event {
                Event::Start(e) if frame.depth == 1 => {
                    if frame.scope != Scope::ContentTypes
                        || e.local_name().as_ref().as_bytes() != b"Types"
                    {
                        return Err(invalid("Invalid catalog content-type root"));
                    }
                    if root {
                        return Err(invalid("Duplicate catalog content-type root"));
                    }
                    root = true;
                }
                Event::Start(e)
                    if frame.depth == 2
                        && frame.scope == Scope::ContentTypes
                        && e.local_name().as_ref().as_bytes() == b"Override" =>
                {
                    if let Some(name) = attribute(&e, b"PartName")? {
                        let name = name.strip_prefix('/').unwrap_or(&name);
                        let target = if directory.is_empty() {
                            Some(name)
                        } else {
                            name.strip_prefix(directory)
                                .and_then(|suffix| suffix.strip_prefix('/'))
                        };
                        let relevant = target.is_some_and(|target| {
                            style
                                && (target == "styles.xml"
                                    || target.starts_with("stylesCrabXL")
                                        && target.ends_with(".xml"))
                                || theme
                                    && (target == "theme/theme1.xml"
                                        || target.starts_with("theme/themeCrabXL")
                                            && target.ends_with(".xml"))
                        });
                        if !relevant || declared.contains(name) {
                            continue;
                        }
                        charged = charged
                            .saturating_add(name.len())
                            .saturating_add(METADATA_ENTRY_BYTES);
                        if charged as u64 > remaining {
                            return Err(Error::new(
                                ErrorKind::MemoryBudgetExceeded,
                                "Catalog content-type declarations exceed save allowance",
                            ));
                        }
                        declared.insert(name.to_owned());
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        if !root {
            return Err(invalid("Missing catalog content-type root"));
        }
        drop(xml);
        let mut parts = Vec::new();
        parts
            .try_reserve_exact(usize::from(style) + usize::from(theme))
            .map_err(|cause| {
                Error::caused_by(
                    ErrorKind::MemoryBudgetExceeded,
                    "Cannot reserve catalog graph additions",
                    cause,
                )
            })?;
        let mut next_id = 1_u64;
        for kind in [
            style.then_some(CatalogKind::Styles),
            theme.then_some(CatalogKind::Theme),
        ]
        .into_iter()
        .flatten()
        {
            let mut suffix = 0_u64;
            let (part, target) = loop {
                let target = match (&kind, suffix) {
                    (CatalogKind::Styles, 0) => "styles.xml".to_owned(),
                    (CatalogKind::Styles, n) => format!("stylesCrabXL{n}.xml"),
                    (CatalogKind::Theme, 0) => "theme/theme1.xml".to_owned(),
                    (CatalogKind::Theme, n) => format!("theme/themeCrabXL{n}.xml"),
                };
                let part = if directory.is_empty() {
                    target.clone()
                } else {
                    format!("{directory}/{target}")
                };
                if !declared.contains(&part) && self.book.archive.index_for_name(&part).is_none() {
                    break (part, target);
                }
                suffix = suffix
                    .checked_add(1)
                    .ok_or_else(|| invalid("Catalog part identity space exhausted"))?;
            };
            let relationship = loop {
                let id = format!("rIdCrabXLCatalog{next_id}");
                next_id = next_id
                    .checked_add(1)
                    .ok_or_else(|| invalid("Catalog relationship identity space exhausted"))?;
                if !relationships.contains_key(&id) {
                    break id;
                }
            };
            let added = CatalogPart {
                kind,
                part,
                target,
                relationship,
            };
            charged = charged
                .saturating_add(added.heap_bytes())
                .saturating_add(size_of::<CatalogPart>());
            if charged as u64 > remaining {
                return Err(Error::new(
                    ErrorKind::MemoryBudgetExceeded,
                    "Catalog graph identities exceed save allowance",
                ));
            }
            parts.push(added);
        }
        if self.parts.len().saturating_add(parts.len()) > self.options.resources.max_archive_entries
        {
            return Err(limit(
                "Created catalog parts exceed configured archive count",
            ));
        }
        Ok(parts)
    }
}
