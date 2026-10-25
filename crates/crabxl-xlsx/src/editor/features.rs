//! Worksheet and workbook feature edits in an existing package: inline
//! elements, table/comment parts, relationships, content types, workbook
//! names/protection and document properties.
use super::*;
use crate::features::package::{
    OutputPart, OutputRelationship, PartNumbers, SheetOutput, build_sheet_output,
};
use crabxl_core::{SheetFeatures, WorkbookFeatures};

/// Planned worksheet rewrite.
pub(super) struct FeaturePlan {
    pub(super) features: Box<SheetFeatures>,
    pub(super) output: SheetOutput,
}

/// Everything a save needs to apply feature edits.
#[derive(Default)]
pub(super) struct FeatureSave {
    /// Worksheet rewrites by worksheet part.
    pub(super) sheets: BTreeMap<String, FeaturePlan>,
    /// Source parts to drop.
    pub(super) remove: HashSet<String>,
    /// New parts.
    pub(super) parts: Vec<OutputPart>,
    /// Replacement content for existing parts, or content of new
    /// relationship parts (`*.rels`).
    pub(super) replace: BTreeMap<String, Vec<u8>>,
    /// Content-type overrides to add.
    pub(super) overrides: Vec<(String, &'static str)>,
    /// Add `<Default Extension="vml">` when missing.
    pub(super) vml_default: bool,
    /// Workbook names and protection for `workbook.xml`.
    pub(super) workbook: Option<Box<WorkbookFeatures>>,
}
impl FeatureSave {
    pub(super) fn types_changed(&self) -> bool {
        !self.remove.is_empty() || !self.overrides.is_empty() || self.vml_default
    }
    pub(super) fn new_parts(&self) -> usize {
        self.parts.len() + self.replace.len()
    }
}

const EMPTY_RELS: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"></Relationships>";

/// Relationship ids in a relationships part.
fn relationship_ids(bytes: &[u8]) -> Result<HashSet<String>> {
    let mut ids = HashSet::new();
    let mut xml = XmlStream::new(
        io::Cursor::new(bytes),
        "relationships".into(),
        bytes.len() as u64,
        ResourceLimits::default(),
    );
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e)
                if frame.depth == 2 && e.local_name().as_ref().as_bytes() == b"Relationship" =>
            {
                if let Some(id) = attribute(&e, b"Id")? {
                    ids.insert(id);
                }
            }
            Event::Eof => return Ok(ids),
            _ => {}
        }
    }
}

/// Drop relationships with ids in `remove` and append `add`.
fn rewrite_relationships(
    bytes: &[u8],
    remove: &HashSet<String>,
    add: &[OutputRelationship],
) -> Result<Vec<u8>> {
    let mut xml = XmlStream::new(
        io::Cursor::new(bytes),
        "relationships".into(),
        bytes.len() as u64,
        ResourceLimits::default(),
    );
    let mut writer = Writer::new(Vec::with_capacity(bytes.len() + add.len() * 128));
    let mut skip = false;
    loop {
        let frame = xml.next()?;
        match frame.event {
            Event::Start(e)
                if frame.depth == 2 && e.local_name().as_ref().as_bytes() == b"Relationship" =>
            {
                let id = attribute(&e, b"Id")?.unwrap_or_default();
                if remove.contains(&id) {
                    skip = true;
                    continue;
                }
                writer
                    .write_event(Event::Start(e))
                    .map_err(|cause| io_error("Cannot write relationships", cause))?;
            }
            Event::End(e) if skip && frame.depth == 1 => {
                skip = false;
                drop(e);
            }
            _ if skip => {}
            Event::End(e) if frame.depth == 0 => {
                for relationship in add {
                    relationship
                        .write(writer.get_mut())
                        .map_err(|cause| io_error("Cannot write relationships", cause))?;
                }
                writer
                    .write_event(Event::End(e))
                    .map_err(|cause| io_error("Cannot write relationships", cause))?;
            }
            Event::Eof => break,
            event => writer
                .write_event(event)
                .map_err(|cause| io_error("Cannot write relationships", cause))?,
        }
    }
    Ok(writer.into_inner())
}

impl<R: Read + Seek> WorkbookEditor<R> {
    fn worksheet_part(&self, sheet: &str) -> Result<String> {
        let info = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet not found"))?;
        if info.kind() != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Features require a cell worksheet",
            ));
        }
        Ok(info.part().to_owned())
    }

    /// Current worksheet features: the pending replacement, or the source.
    pub fn sheet_features(&mut self, sheet: &str) -> Result<SheetFeatures> {
        let part = self.worksheet_part(sheet)?;
        if let Some(features) = self.feature_patches.get(&part) {
            return Ok((**features).clone());
        }
        self.book.sheet_features(sheet)
    }

    /// Replace all features of a worksheet on the next save. Table and
    /// comment parts are rebuilt only when tables or comments change.
    pub fn set_sheet_features(&mut self, sheet: &str, features: SheetFeatures) -> Result<()> {
        let part = self.worksheet_part(sheet)?;
        self.feature_patches.insert(part, Box::new(features));
        Ok(())
    }

    pub(crate) fn set_sheet_features_at(
        &mut self,
        index: usize,
        features: SheetFeatures,
    ) -> Result<()> {
        let part = self
            .book
            .sheets()
            .get(index)
            .ok_or_else(|| invalid("Unknown source sheet index"))?
            .part()
            .to_owned();
        self.feature_patches.insert(part, Box::new(features));
        Ok(())
    }
    /// Current workbook features: the pending replacement, or the source.
    pub fn workbook_features(&mut self) -> Result<WorkbookFeatures> {
        if let Some(features) = &self.workbook_feature_patch {
            return Ok((**features).clone());
        }
        self.book.workbook_features()
    }

    /// Replace defined names, workbook protection and document properties on
    /// the next save.
    pub fn set_workbook_features(&mut self, features: WorkbookFeatures) {
        self.workbook_feature_patch = Some(Box::new(features));
    }

    pub(super) fn has_feature_patches(&self) -> bool {
        !self.feature_patches.is_empty()
            || !self.created_feature_patches.is_empty()
            || self.workbook_feature_patch.is_some()
    }
    /// Features of a sheet created in this editing session (by model id).
    pub(crate) fn set_created_sheet_features(
        &mut self,
        id: crabxl_core::SheetId,
        features: SheetFeatures,
    ) {
        self.created_feature_patches
            .retain(|(existing, _)| *existing != id);
        self.created_feature_patches.push((id, Box::new(features)));
    }

    fn part_bytes(&mut self, part: &str) -> Result<Vec<u8>> {
        let maximum = self.options.resources.max_part_bytes;
        let file = self
            .book
            .archive
            .by_name(part)
            .map_err(|cause| zip_error("Cannot read package part", cause).with_part(part))?;
        let mut bytes = Vec::new();
        file.take(maximum.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|cause| io_error("Cannot read package part", cause).with_part(part))?;
        if bytes.len() as u64 > maximum {
            return Err(limit("Package part exceeds configured limit").with_part(part));
        }
        Ok(bytes)
    }

    fn table_ids(&mut self) -> Result<u32> {
        let names: Vec<String> = self
            .parts
            .iter()
            .filter(|part| part.name.starts_with("xl/tables/") && part.name.ends_with(".xml"))
            .map(|part| part.name.to_string())
            .collect();
        let mut maximum = 0u32;
        for name in names {
            let bytes = self.part_bytes(&name)?;
            let mut xml = XmlStream::new(
                io::Cursor::new(bytes.as_slice()),
                name.clone(),
                bytes.len() as u64,
                self.options.resources,
            );
            loop {
                let frame = xml.next()?;
                match frame.event {
                    Event::Start(e) if frame.depth == 1 => {
                        if let Some(id) = attribute(&e, b"id")?.and_then(|id| id.parse().ok()) {
                            maximum = maximum.max(id);
                        }
                        break;
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
        }
        Ok(maximum)
    }

    /// Plan feature edits. `hyperlink_relationships` are worksheet
    /// relationship parts that the hyperlink save already rewrites.
    pub(super) fn prepare_feature_save(
        &mut self,
        hyperlink_relationships: &BTreeMap<String, &[u8]>,
    ) -> Result<FeatureSave> {
        let mut save = FeatureSave::default();
        if !self.has_feature_patches() {
            return Ok(save);
        }
        let existing: Vec<String> = self
            .parts
            .iter()
            .map(|part| part.name.to_string())
            .collect();
        let mut numbers = PartNumbers::avoiding(existing.iter().map(String::as_str));
        let patches: Vec<(String, Box<SheetFeatures>)> = self
            .feature_patches
            .iter()
            .map(|(part, features)| (part.clone(), features.clone()))
            .collect();
        let patches_empty = patches.is_empty();
        if !patches_empty {
            let maximum = self.table_ids()?;
            numbers.reserve_table_id(maximum);
        }
        for (part, features) in patches {
            let (index, name) = self
                .book
                .sheets()
                .iter()
                .enumerate()
                .find(|(_, info)| info.part() == part)
                .map(|(index, info)| (index, info.name().to_owned()))
                .ok_or_else(|| invalid("Feature patch sheet is missing"))?;
            let (source, sources) = self.book.sheet_features_with_sources(&name)?;
            let tables_changed = source.tables != features.tables;
            let comments_changed = source.comments != features.comments;
            if !tables_changed && !comments_changed {
                let mut output = SheetOutput::default();
                output.refs.tables = sources.tables.iter().map(|(id, _)| id.clone()).collect();
                output.refs.legacy_drawing = sources.vml.as_ref().map(|(id, _)| id.clone());
                save.sheets.insert(part, FeaturePlan { features, output });
                continue;
            }
            // Rebuild only the changed part group; keep the other group's parts.
            let mut removed_ids = HashSet::new();
            if tables_changed {
                for (id, name) in &sources.tables {
                    removed_ids.insert(id.clone());
                    save.remove.insert(name.clone());
                }
            }
            if comments_changed {
                for (id, name) in sources.comments.iter().chain(sources.vml.iter()) {
                    removed_ids.insert(id.clone());
                    save.remove.insert(name.clone());
                }
            }
            let rebuilt = SheetFeatures {
                tables: if tables_changed {
                    features.tables.clone()
                } else {
                    Vec::new()
                },
                comments: if comments_changed {
                    features.comments.clone()
                } else {
                    Vec::new()
                },
                ..SheetFeatures::default()
            };
            let rels_part = crate::package::relationship_part(&part);
            let base = if let Some(bytes) = hyperlink_relationships.get(&rels_part) {
                bytes.to_vec()
            } else if self.book.archive.index_for_name(&rels_part).is_some() {
                self.part_bytes(&rels_part)?
            } else {
                EMPTY_RELS.to_vec()
            };
            let mut taken = relationship_ids(&base)?;
            let mut next = 0u32;
            let mut output = build_sheet_output(&rebuilt, index, &part, &mut numbers, |_| {
                loop {
                    next += 1;
                    let id = format!("rIdF{next}");
                    if taken.insert(id.clone()) {
                        break id;
                    }
                }
            })
            .map_err(|cause| io_error("Cannot encode worksheet feature parts", cause))?;
            if !tables_changed {
                output.refs.tables = sources.tables.iter().map(|(id, _)| id.clone()).collect();
            }
            if !comments_changed {
                output.refs.legacy_drawing = sources.vml.as_ref().map(|(id, _)| id.clone());
            }
            let rels = rewrite_relationships(&base, &removed_ids, &output.relationships)?;
            save.replace.insert(rels_part, rels);
            for part in output.parts.drain(..) {
                if part.content_type == crate::features::CT_VML {
                    save.vml_default = true;
                } else {
                    save.overrides.push((part.name.clone(), part.content_type));
                }
                save.parts.push(part);
            }
            save.sheets.insert(part, FeaturePlan { features, output });
        }
        let created: Vec<(String, Box<SheetFeatures>)> = self
            .membership
            .as_deref()
            .map(|membership| {
                membership
                    .created
                    .values()
                    .filter_map(|created| {
                        let id = created.bank_id?;
                        self.created_feature_patches
                            .iter()
                            .find(|(existing, _)| *existing == id)
                            .map(|(_, features)| (created.part.clone(), features.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !created.is_empty() && patches_empty {
            let maximum = self.table_ids()?;
            numbers.reserve_table_id(maximum);
        }
        for (part, features) in created {
            let index = self.book.sheets().len() + save.sheets.len();
            let rels_part = crate::package::relationship_part(&part);
            let base = hyperlink_relationships
                .get(&rels_part)
                .map_or_else(|| EMPTY_RELS.to_vec(), |bytes| bytes.to_vec());
            let mut taken = relationship_ids(&base)?;
            let mut next = 0u32;
            let mut output = build_sheet_output(&features, index, &part, &mut numbers, |_| {
                loop {
                    next += 1;
                    let id = format!("rIdF{next}");
                    if taken.insert(id.clone()) {
                        break id;
                    }
                }
            })
            .map_err(|cause| io_error("Cannot encode worksheet feature parts", cause))?;
            if !output.relationships.is_empty() {
                let rels = rewrite_relationships(&base, &HashSet::new(), &output.relationships)?;
                save.replace.insert(rels_part, rels);
            }
            for part in output.parts.drain(..) {
                if part.content_type == crate::features::CT_VML {
                    save.vml_default = true;
                } else {
                    save.overrides.push((part.name.clone(), part.content_type));
                }
                save.parts.push(part);
            }
            save.sheets.insert(part, FeaturePlan { features, output });
        }
        if let Some(features) = self.workbook_feature_patch.clone() {
            self.prepare_workbook_features(&features, &mut save)?;
            save.workbook = Some(features);
        }
        Ok(save)
    }

    fn prepare_workbook_features(
        &mut self,
        features: &WorkbookFeatures,
        save: &mut FeatureSave,
    ) -> Result<()> {
        use crate::features::package as fp;
        let props = &features.properties;
        if *props == crabxl_core::features::DocumentProperties::default() {
            return Ok(());
        }
        let root = "_rels/.rels";
        let base = if self.book.archive.index_for_name(root).is_some() {
            self.part_bytes(root)?
        } else {
            EMPTY_RELS.to_vec()
        };
        let mut remaining = self.options.resources.max_metadata_bytes;
        let relationships = if self.book.archive.index_for_name(root).is_some() {
            crate::package::read_relationships(
                &mut self.book.archive,
                root,
                self.options.resources,
                &mut remaining,
            )?
        } else {
            Default::default()
        };
        let find = |suffix: &str| -> Result<Option<String>> {
            for relationship in relationships.values() {
                if !relationship.external && relationship.kind.rsplit('/').next() == Some(suffix) {
                    return crate::package::resolve_part("", &relationship.target).map(Some);
                }
            }
            Ok(None)
        };
        let mut payloads: Vec<(&str, &str, &'static str, &'static str, Vec<u8>)> = Vec::new();
        let mut core = Vec::new();
        crate::features::properties::write_core(&mut core, props)
            .map_err(|cause| io_error("Cannot encode core properties", cause))?;
        payloads.push((
            "core-properties",
            "docProps/core.xml",
            fp::REL_CORE,
            fp::CT_CORE,
            core,
        ));
        let names: Vec<&str> = self.book.sheets().iter().map(SheetInfo::name).collect();
        let mut app = Vec::new();
        crate::features::properties::write_app(&mut app, props, &names)
            .map_err(|cause| io_error("Cannot encode extended properties", cause))?;
        payloads.push((
            "extended-properties",
            "docProps/app.xml",
            fp::REL_APP,
            fp::CT_APP,
            app,
        ));
        if crate::features::properties::has_custom(props) {
            let mut custom = Vec::new();
            crate::features::properties::write_custom(&mut custom, props)
                .map_err(|cause| io_error("Cannot encode custom properties", cause))?;
            payloads.push((
                "custom-properties",
                "docProps/custom.xml",
                fp::REL_CUSTOM,
                fp::CT_CUSTOM,
                custom,
            ));
        }
        let mut taken = relationship_ids(&base)?;
        let mut added = Vec::new();
        for (suffix, default_name, kind, content_type, bytes) in payloads {
            if let Some(part) = find(suffix)?
                && self.book.archive.index_for_name(&part).is_some()
            {
                save.replace.insert(part, bytes);
                continue;
            }
            let mut next = 0u32;
            let id = loop {
                next += 1;
                let id = format!("rIdP{next}");
                if taken.insert(id.clone()) {
                    break id;
                }
            };
            added.push(OutputRelationship {
                id,
                kind,
                target: default_name.to_owned(),
            });
            save.overrides.push((default_name.to_owned(), content_type));
            save.parts.push(OutputPart {
                name: default_name.to_owned(),
                content_type,
                bytes,
            });
        }
        if !added.is_empty() {
            let rels = rewrite_relationships(&base, &HashSet::new(), &added)?;
            save.replace.insert(root.to_owned(), rels);
        }
        Ok(())
    }
}

/// Child order of `CT_Workbook`.
const WORKBOOK_ORDER: &[&[u8]] = &[
    b"fileVersion",
    b"fileSharing",
    b"workbookPr",
    b"workbookProtection",
    b"bookViews",
    b"sheets",
    b"functionGroups",
    b"externalReferences",
    b"definedNames",
    b"calcPr",
    b"oleSize",
    b"customWorkbookViews",
    b"pivotCaches",
    b"smartTagPr",
    b"smartTagTypes",
    b"webPublishing",
    b"fileRecoveryPr",
    b"webPublishObjects",
    b"extLst",
];

fn workbook_rank(name: &[u8]) -> usize {
    WORKBOOK_ORDER
        .iter()
        .position(|item| *item == name)
        .unwrap_or(WORKBOOK_ORDER.len())
}

/// Streaming replacement of `workbookProtection` and `definedNames`.
pub(super) struct WorkbookFeatureRewrite<'a> {
    features: &'a WorkbookFeatures,
    next: usize,
}
impl<'a> WorkbookFeatureRewrite<'a> {
    pub(super) fn new(features: &'a WorkbookFeatures) -> Self {
        Self { features, next: 0 }
    }
    fn write_rank(&self, out: &mut impl Write, rank: usize, uri: Option<&str>) -> io::Result<()> {
        match WORKBOOK_ORDER.get(rank).copied() {
            Some(b"workbookProtection") => {
                if let Some(protection) = &self.features.protection {
                    crate::features::protection::write_workbook(out, protection, uri)?;
                }
            }
            Some(b"definedNames") => {
                crate::features::defined_name::write(out, &self.features.defined_names, uri)?;
            }
            _ => {}
        }
        Ok(())
    }
    /// Returns true when the source element must be skipped.
    pub(super) fn before_start(
        &mut self,
        out: &mut impl Write,
        name: &[u8],
        uri: Option<&str>,
    ) -> io::Result<bool> {
        let rank = workbook_rank(name);
        let owned = matches!(name, b"workbookProtection" | b"definedNames");
        if rank < self.next {
            return Ok(owned);
        }
        for pending in self.next..rank.min(WORKBOOK_ORDER.len()) {
            self.write_rank(out, pending, uri)?;
        }
        self.next = rank.min(WORKBOOK_ORDER.len());
        if owned {
            self.write_rank(out, rank, uri)?;
            self.next = rank + 1;
        }
        Ok(owned)
    }
    pub(super) fn finish(&mut self, out: &mut impl Write, uri: Option<&str>) -> io::Result<()> {
        for pending in self.next..WORKBOOK_ORDER.len() {
            self.write_rank(out, pending, uri)?;
        }
        self.next = WORKBOOK_ORDER.len();
        Ok(())
    }
}
