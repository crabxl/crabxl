//! Worksheet and workbook feature edits in an existing package: inline
//! elements, table/comment parts, relationships, content types, workbook
//! names/protection and document properties.
use super::*;
use crate::features::package::{
    OutputPart, OutputRelationship, PartNumbers, SheetOutput, build_sheet_output,
};
use crabxl_core::{SheetFeatures, WorkbookFeatures};
use std::collections::HashMap;

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
    pub(super) overrides: Vec<(String, Box<str>)>,
    /// Add `<Default Extension="vml">` when missing.
    pub(super) vml_default: bool,
    /// Workbook names and protection for `workbook.xml`.
    pub(super) workbook: Option<Box<WorkbookFeatures>>,
    pub(super) pivot_caches: Vec<crate::features::package::PivotCacheOutput>,
    pub(super) external_refs: Option<Vec<String>>,
}
impl FeatureSave {
    pub(super) fn types_changed(&self) -> bool {
        !self.remove.is_empty() || !self.overrides.is_empty() || self.vml_default
    }
    pub(super) fn new_parts(&self) -> usize {
        self.parts.len() + self.replace.len()
    }
}

impl<R: Read + Seek> WorkbookEditor<R> {}

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
    rewrite_relationships_with_extra(bytes, remove, add, &[])
}
fn rewrite_relationships_with_extra(
    bytes: &[u8],
    remove: &HashSet<String>,
    add: &[OutputRelationship],
    extra: &[u8],
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
                writer.get_mut().extend_from_slice(extra);
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
    let bytes = writer.into_inner();
    crate::features::package::validate_relationship_ids(&bytes)?;
    Ok(bytes)
}

impl<R: Read + Seek> WorkbookEditor<R> {
    /// Remove only graph members with no consumers outside the replaced owner.
    pub(super) fn remove_feature_graph(
        &mut self,
        root: &str,
        owner: &str,
        id: &str,
        strict: bool,
        save: &mut FeatureSave,
    ) -> Result<()> {
        let mut remaining = self.options.resources.max_metadata_bytes;
        let mut edges = HashMap::<String, Vec<String>>::new();
        let mut queue = vec![root.to_owned()];
        while let Some(part) = queue.pop() {
            if edges.contains_key(&part) {
                continue;
            }
            // Deleting a dangling part still removes its owned relationships and overrides.
            let rels = crate::package::relationship_part(&part);
            let mut targets = Vec::new();
            if self.book.archive.index_for_name(&rels).is_some() {
                for rel in crate::package::read_relationships(
                    &mut self.book.archive,
                    &rels,
                    self.options.resources,
                    &mut remaining,
                )?
                .values()
                {
                    if !rel.external {
                        targets.push(crate::package::resolve_part(&part, &rel.target)?);
                    }
                }
            }
            queue.extend(targets.iter().cloned());
            edges.insert(part, targets);
        }
        let mut keep = HashSet::new();
        for info in &self.parts {
            let Some(source) = crate::package::relationship_source(&info.name) else {
                continue;
            };
            if edges.contains_key(&source)
                || save.remove.contains(&source)
                || self
                    .membership
                    .as_deref()
                    .is_some_and(|membership| membership.removes_part(&source))
            {
                continue;
            }
            for (rid, rel) in crate::package::read_relationships(
                &mut self.book.archive,
                &info.name,
                self.options.resources,
                &mut remaining,
            )? {
                if rel.external
                    || source == owner && rid == id
                    || source == self.book.workbook_part
                        && self
                            .membership
                            .as_deref()
                            .is_some_and(|membership| membership.removes_relationship(&rid))
                {
                    continue;
                }
                let target = crate::package::resolve_part(&source, &rel.target)?;
                if edges.contains_key(&target) {
                    keep.insert(target);
                }
            }
        }
        if strict && keep.contains(root) {
            // Another part still needs the project, so it stays.
            return Ok(());
        }
        let mut queue: Vec<_> = keep.iter().cloned().collect();
        while let Some(source) = queue.pop() {
            if let Some(targets) = edges.get(&source) {
                for target in targets {
                    if keep.insert(target.clone()) {
                        queue.push(target.clone());
                    }
                }
            }
        }
        for part in edges.keys().filter(|part| !keep.contains(*part)) {
            save.remove.insert(part.clone());
            let rels = crate::package::relationship_part(part);
            if self.book.archive.index_for_name(&rels).is_some() {
                save.remove.insert(rels);
            }
        }
        Ok(())
    }

    fn prepare_complex_features(
        &mut self,
        features: &WorkbookFeatures,
        numbers: &mut PartNumbers,
        save: &mut FeatureSave,
    ) -> Result<()> {
        use crabxl_core::features::{MacroPolicy, WorkbookFormat};
        if features.metadata.is_none()
            && features.external_links.is_none()
            && features.format.is_none()
            && matches!(features.macros, MacroPolicy::Preserve)
        {
            return Ok(());
        }
        let workbook = self.book.workbook_part.clone();
        let rels_part = self.workbook_relationships.clone();
        let base = match save.replace.get(&rels_part) {
            Some(bytes) => bytes.clone(),
            None => self.part_bytes(&rels_part)?,
        };
        let mut remaining = self.options.resources.max_metadata_bytes;
        let relationships = crate::package::read_relationships(
            &mut self.book.archive,
            &rels_part,
            self.options.resources,
            &mut remaining,
        )?;
        let had_macros = relationships
            .values()
            .any(|rel| rel.kind.rsplit('/').next() == Some("vbaProject"));
        let macros = match features.macros {
            MacroPolicy::Preserve => had_macros,
            MacroPolicy::Remove => false,
            MacroPolicy::Replace(_) => true,
        };
        let source_format = self.book.workbook_format();
        let default_format = if matches!(features.macros, MacroPolicy::Preserve) {
            source_format
        } else {
            match (source_format.is_template(), macros) {
                (false, false) => WorkbookFormat::Xlsx,
                (false, true) => WorkbookFormat::Xlsm,
                (true, false) => WorkbookFormat::Xltx,
                (true, true) => WorkbookFormat::Xltm,
            }
        };
        let format = features.format.unwrap_or(default_format);
        if macros && !format.allows_macros() {
            return Err(invalid("Removing VBA requires an explicit Remove policy"));
        }
        if format != source_format {
            save.overrides
                .push((workbook.clone(), format.content_type().into()));
        }
        let mut remove = HashSet::new();
        for (id, rel) in &relationships {
            let role = rel.kind.rsplit('/').next().unwrap_or("");
            if features.metadata.is_some() && crate::complex::metadata_relationship(&rel.kind)
                || features.external_links.is_some() && role == "externalLink"
                || !matches!(features.macros, MacroPolicy::Preserve) && role == "vbaProject"
            {
                if rel.external {
                    return Err(invalid("Workbook feature root cannot be external"));
                }
                let target = crate::package::resolve_part(&workbook, &rel.target)?;
                self.remove_feature_graph(&target, &workbook, id, role == "vbaProject", save)?;
                remove.insert(id.clone());
            }
        }
        let mut taken = relationship_ids(&base)?;
        let mut next = 0u64;
        let output = crate::complex::build_output(features, &workbook, numbers, |_| {
            loop {
                next += 1;
                let id = format!("rIdComplex{next}");
                if taken.insert(id.clone()) {
                    break id;
                }
            }
        })
        .map_err(|cause| io_error("Cannot encode workbook feature graphs", cause))?;
        if features.external_links.is_some() {
            save.external_refs = Some(output.external_refs);
        }
        save.replace.insert(
            rels_part,
            rewrite_relationships(&base, &remove, &output.relationships)?,
        );
        for part in output.parts {
            save.overrides
                .push((part.name.clone(), part.content_type.clone()));
            save.parts.push(part);
        }
        Ok(())
    }
    fn next_pivot_cache_id(&mut self) -> Result<u64> {
        let part = self.book.workbook_part.clone();
        let mut xml = self.book.part_stream(&part)?;
        let mut next = 0u64;
        loop {
            let frame = xml.next()?;
            match frame.event {
                Event::Start(e)
                    if frame.depth == 3
                        && frame.scope == Scope::Spreadsheet
                        && e.local_name().as_ref() == "pivotCache" =>
                {
                    let id: u32 = crate::xml::required_attribute(&e, b"cacheId")?
                        .parse()
                        .map_err(|_| invalid("Invalid pivot cache id"))?;
                    next = next.max(u64::from(id) + 1);
                }
                Event::Eof => return Ok(next),
                _ => {}
            }
        }
    }
    fn prepare_pivot_cache_relationships(&mut self, save: &mut FeatureSave) -> Result<()> {
        if save.pivot_caches.is_empty() {
            return Ok(());
        }
        let part = self.workbook_relationships.clone();
        let bytes = match save.replace.get(&part) {
            Some(bytes) => bytes.clone(),
            None => self.part_bytes(&part)?,
        };
        let mut taken = relationship_ids(&bytes)?;
        let mut added = Vec::new();
        for cache in &mut save.pivot_caches {
            while !taken.insert(cache.relationship.clone()) {
                cache.relationship.push('_');
            }
            added.push(OutputRelationship {
                id: cache.relationship.clone(),
                kind: crate::pivot::REL_CACHE.into(),
                target: crate::features::package::relative_target(
                    &self.book.workbook_part,
                    &cache.part,
                ),
            });
        }
        save.replace.insert(
            part,
            rewrite_relationships(&bytes, &HashSet::new(), &added)?,
        );
        Ok(())
    }
    fn worksheet_part(&self, sheet: &str) -> Result<String> {
        let info = self
            .book
            .sheets()
            .iter()
            .find(|info| info.name() == sheet)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet not found"))?;
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
        let mut overrides = std::collections::HashMap::new();
        let mut defaults = std::collections::HashMap::new();
        {
            let mut xml = self.book.part_stream("[Content_Types].xml")?;
            loop {
                let frame = xml.next()?;
                match frame.event {
                    Event::Start(e) if frame.depth == 2 && frame.scope == Scope::ContentTypes => {
                        match e.local_name().as_ref() {
                            "Override" => {
                                overrides.insert(
                                    crate::package::resolve_part(
                                        "",
                                        &crate::xml::required_attribute(&e, b"PartName")?,
                                    )?,
                                    crate::xml::required_attribute(&e, b"ContentType")?,
                                );
                            }
                            "Default" => {
                                defaults.insert(
                                    crate::xml::required_attribute(&e, b"Extension")?
                                        .to_ascii_lowercase(),
                                    crate::xml::required_attribute(&e, b"ContentType")?,
                                );
                            }
                            _ => {}
                        }
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
        }
        let names: Vec<String> = self
            .parts
            .iter()
            .filter(|part| {
                overrides
                    .get(part.name.as_ref())
                    .or_else(|| {
                        defaults.get(
                            &part
                                .name
                                .rsplit('.')
                                .next()
                                .unwrap_or("")
                                .to_ascii_lowercase(),
                        )
                    })
                    .is_some_and(|kind| kind == crate::features::CT_TABLE)
            })
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
                        if frame.scope != Scope::Spreadsheet || e.local_name().as_ref() != "table" {
                            return Err(invalid("Invalid table part root"));
                        }
                        let id: u32 = crate::xml::required_attribute(&e, b"id")?
                            .parse()
                            .map_err(|_| invalid("Invalid table id"))?;
                        maximum = maximum.max(id);
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
        let removed = self
            .membership
            .as_deref()
            .map(|membership| membership.removed_roots())
            .unwrap_or_default();
        let workbook = self.book.workbook_part.clone();
        for (part, id) in &removed {
            self.remove_feature_graph(part, &workbook, id, false, &mut save)?;
        }
        if !self.has_feature_patches() {
            return Ok(save);
        }
        let existing: Vec<String> = self
            .parts
            .iter()
            .map(|part| part.name.to_string())
            .collect();
        let mut numbers = PartNumbers::avoiding(existing.iter().map(String::as_str));
        numbers.pivot_cache_id = self.next_pivot_cache_id()?;
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
            if self
                .membership
                .as_deref()
                .is_some_and(|membership| membership.removes_part(&part))
            {
                continue;
            }
            let (index, name) = self
                .book
                .sheets()
                .iter()
                .enumerate()
                .find(|(_, info)| info.part() == part)
                .map(|(index, info)| (index, info.name().to_owned()))
                .ok_or_else(|| invalid("Feature patch sheet is missing"))?;
            let (source, sources) = self.book.sheet_features_with_sources(&name)?;
            if let Some(root) = features.chartsheet.as_ref().or(source.chartsheet.as_ref()) {
                crate::drawing::validate_chartsheet_copy(root, features.chartsheet_parts.as_ref())?;
            }
            let chart_parts_changed = source.chartsheet_parts != features.chartsheet_parts;
            let printer_changed = source.printer_settings != features.printer_settings;
            if printer_changed {
                let printing_id = self
                    .print_patches
                    .get(&part)
                    .map(|settings| settings.setup.printer_relationship.as_deref())
                    .unwrap_or_else(|| {
                        source
                            .printer_settings
                            .as_ref()
                            .map(|settings| settings.relationship.as_ref())
                    });
                if printing_id
                    != features
                        .printer_settings
                        .as_ref()
                        .map(|settings| settings.relationship.as_ref())
                {
                    return Err(invalid(
                        "Printer settings must match the page-setup relationship",
                    ));
                }
            }
            let tables_changed = source.tables != features.tables;
            let comments_changed = source.comments != features.comments;
            let images_changed = if let Some(images) = &features.images {
                *images != self.book.sheet_images(&name)?
            } else {
                false
            };
            let charts_changed = if let Some(charts) = &features.charts {
                *charts != self.book.sheet_charts(&name)?
            } else {
                false
            };
            let shapes_changed = if let Some(shapes) = &features.shapes {
                *shapes != self.book.sheet_shapes(&name)?
            } else {
                false
            };
            let drawings_changed = images_changed || charts_changed || shapes_changed;
            let pivots_changed = if let Some(pivots) = &features.pivots {
                *pivots != self.book.sheet_pivots(&name)?
            } else {
                false
            };
            if !chart_parts_changed
                && !printer_changed
                && !tables_changed
                && !comments_changed
                && !drawings_changed
                && !pivots_changed
            {
                let mut output = SheetOutput::default();
                output.refs.pivots = sources.pivots.iter().map(|(id, _)| id.clone()).collect();
                output.refs.drawing = sources.drawing.as_ref().map(|(id, _)| id.clone());
                output.refs.tables = sources.tables.iter().map(|(id, _)| id.clone()).collect();
                output.refs.legacy_drawing = sources.vml.as_ref().map(|(id, _)| id.clone());
                save.sheets.insert(part, FeaturePlan { features, output });
                continue;
            }
            // Rebuild only the changed part group; keep the other group's parts.
            let mut removed_ids = HashSet::new();
            if chart_parts_changed && let Some(graph) = &source.chartsheet_parts {
                for rel in &graph.relationships {
                    removed_ids.insert(rel.id.to_string());
                    if !rel.external {
                        self.remove_feature_graph(&rel.target, &part, &rel.id, false, &mut save)?;
                    }
                }
            }
            if printer_changed && let Some((id, name)) = &sources.printer {
                self.remove_feature_graph(name, &part, id, false, &mut save)?;
                removed_ids.insert(id.clone());
            }
            if tables_changed {
                for (id, name) in &sources.tables {
                    removed_ids.insert(id.clone());
                    self.remove_feature_graph(name, &part, id, false, &mut save)?;
                }
            }
            if comments_changed {
                for (id, name) in sources.comments.iter().chain(sources.vml.iter()) {
                    removed_ids.insert(id.clone());
                    self.remove_feature_graph(name, &part, id, false, &mut save)?;
                }
            }
            if drawings_changed && let Some((id, name)) = &sources.drawing {
                self.remove_feature_graph(name, &part, id, false, &mut save)?;
                removed_ids.insert(id.clone());
            }
            if pivots_changed {
                for (id, name) in &sources.pivots {
                    self.remove_feature_graph(name, &part, id, false, &mut save)?;
                    removed_ids.insert(id.clone());
                }
            }
            let rebuilt = SheetFeatures {
                chartsheet_parts: if chart_parts_changed {
                    features.chartsheet_parts.clone()
                } else {
                    None
                },
                shapes: if drawings_changed {
                    Some(match &features.shapes {
                        Some(shapes) => shapes.clone(),
                        None => self.book.sheet_shapes(&name)?,
                    })
                } else {
                    None
                },
                printer_settings: if printer_changed {
                    features.printer_settings.clone()
                } else {
                    None
                },
                pivots: if pivots_changed {
                    features.pivots.clone()
                } else {
                    None
                },
                images: if drawings_changed {
                    Some(match &features.images {
                        Some(images) => images.clone(),
                        None => self.book.sheet_images(&name)?,
                    })
                } else {
                    None
                },
                charts: if drawings_changed {
                    Some(match &features.charts {
                        Some(charts) => charts.clone(),
                        None => self.book.sheet_charts(&name)?,
                    })
                } else {
                    None
                },
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
            if let Some(settings) = &rebuilt.printer_settings {
                if taken.contains(settings.relationship.as_ref())
                    && !removed_ids.contains(settings.relationship.as_ref())
                {
                    return Err(invalid(
                        "Printer relationship collides with an existing owner",
                    ));
                }
                taken.insert(settings.relationship.to_string());
            }
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
            if !pivots_changed {
                output.refs.pivots = sources.pivots.iter().map(|(id, _)| id.clone()).collect();
            }
            if !drawings_changed {
                output.refs.drawing = sources.drawing.as_ref().map(|(id, _)| id.clone());
            }
            if !tables_changed {
                output.refs.tables = sources.tables.iter().map(|(id, _)| id.clone()).collect();
            }
            if !comments_changed {
                output.refs.legacy_drawing = sources.vml.as_ref().map(|(id, _)| id.clone());
            }
            let rels = rewrite_relationships_with_extra(
                &base,
                &removed_ids,
                &output.relationships,
                &output.extra_relationships,
            )?;
            save.replace.insert(rels_part, rels);
            save.pivot_caches.append(&mut output.pivot_caches);
            for part in output.parts.drain(..) {
                if part.content_type.as_ref() == crate::features::CT_VML {
                    save.vml_default = true;
                } else {
                    save.overrides
                        .push((part.name.clone(), part.content_type.clone()));
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
            if let Some(root) = &features.chartsheet {
                crate::drawing::validate_chartsheet_copy(root, features.chartsheet_parts.as_ref())?;
            }
            let index = self.book.sheets().len() + save.sheets.len();
            let rels_part = crate::package::relationship_part(&part);
            let base = hyperlink_relationships
                .get(&rels_part)
                .map_or_else(|| EMPTY_RELS.to_vec(), |bytes| bytes.to_vec());
            let mut taken = relationship_ids(&base)?;
            if let Some(settings) = &features.printer_settings
                && !taken.insert(settings.relationship.to_string())
            {
                return Err(invalid(
                    "Printer relationship collides with an existing owner",
                ));
            }
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
            if !output.relationships.is_empty() || !output.extra_relationships.is_empty() {
                let rels = rewrite_relationships_with_extra(
                    &base,
                    &HashSet::new(),
                    &output.relationships,
                    &output.extra_relationships,
                )?;
                save.replace.insert(rels_part, rels);
            }
            save.pivot_caches.append(&mut output.pivot_caches);
            for part in output.parts.drain(..) {
                if part.content_type.as_ref() == crate::features::CT_VML {
                    save.vml_default = true;
                } else {
                    save.overrides
                        .push((part.name.clone(), part.content_type.clone()));
                }
                save.parts.push(part);
            }
            save.sheets.insert(part, FeaturePlan { features, output });
        }
        if let Some(features) = self.workbook_feature_patch.clone() {
            self.prepare_workbook_features(&features, &mut save)?;
            self.prepare_complex_features(&features, &mut numbers, &mut save)?;
            save.workbook = Some(features);
        }
        self.prepare_pivot_cache_relationships(&mut save)?;
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
                kind: kind.into(),
                target: default_name.to_owned(),
            });
            save.overrides
                .push((default_name.to_owned(), content_type.into()));
            save.parts.push(OutputPart {
                name: default_name.to_owned(),
                content_type: content_type.into(),
                bytes: bytes.into(),
            });
        }
        if !added.is_empty() {
            let rels = rewrite_relationships(&base, &HashSet::new(), &added)?;
            save.replace.insert(root.to_owned(), rels);
        }
        Ok(())
    }
}

pub(super) fn write_pivot_cache_refs(
    out: &mut impl Write,
    caches: &[crate::features::package::PivotCacheOutput],
    uri: Option<&str>,
) -> io::Result<()> {
    for cache in caches {
        out.write_all(b"<pivotCache")?;
        crate::printing::namespace(out, uri)?;
        let rel = if uri == Some(crate::xml::STRICT_MAIN_URI) {
            crate::xml::STRICT_OFFICE_REL_URI
        } else {
            crate::xml::OFFICE_REL_URI
        };
        write!(
            out,
            " xmlns:r=\"{rel}\" cacheId=\"{}\" r:id=\"{}\"/>",
            cache.id, cache.relationship
        )?;
    }
    Ok(())
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

pub(super) fn workbook_rank(name: &[u8]) -> usize {
    WORKBOOK_ORDER
        .iter()
        .position(|item| *item == name)
        .unwrap_or(WORKBOOK_ORDER.len())
}

/// Streaming replacement of `workbookProtection` and `definedNames`.
pub(super) struct WorkbookFeatureRewrite<'a> {
    features: &'a WorkbookFeatures,
    next: usize,
    invalidate_caches: bool,
    active: Option<crabxl_core::ActiveViewSelection>,
    date_1904: bool,
}
impl<'a> WorkbookFeatureRewrite<'a> {
    pub(super) fn new(
        features: &'a WorkbookFeatures,
        invalidate_caches: bool,
        active: Option<crabxl_core::ActiveViewSelection>,
        date_1904: bool,
    ) -> Self {
        Self {
            features,
            next: 0,
            invalidate_caches,
            active,
            date_1904,
        }
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
            Some(b"workbookPr") => {
                if let Some(properties) = &self.features.workbook_properties {
                    crate::features::workbook_properties::write(
                        out,
                        properties,
                        uri,
                        Some(self.date_1904),
                    )?;
                }
            }
            Some(b"bookViews") => {
                if let Some(views) = &self.features.views {
                    crate::features::book_views::write(out, views, uri, self.active)?;
                }
            }
            Some(b"calcPr") => {
                if let Some(calculation) = &self.features.calculation {
                    crate::features::calculation::write(
                        out,
                        calculation,
                        uri,
                        self.invalidate_caches,
                    )?;
                }
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
        let owned = matches!(name, b"workbookProtection" | b"definedNames" | b"calcPr")
            || (name == b"bookViews" && self.features.views.is_some())
            || (name == b"workbookPr" && self.features.workbook_properties.is_some());
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
