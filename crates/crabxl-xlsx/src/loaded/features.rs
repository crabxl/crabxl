//! Worksheet and workbook features of a loaded workbook. Features load on
//! first access, independently of cell materialization.
use super::*;

fn retarget_chart_ranges(
    element: &mut crabxl_core::XmlElement,
    source: &str,
    quoted_source: &str,
    destination: &str,
) {
    use crabxl_core::XmlContent;
    match element.local_name() {
        "extLst" => return,
        "numRef" | "strRef" | "multiLvlStrRef" => {
            if let Some(formula) = element.child_mut("f")
                && let [XmlContent::Text(text)] = formula.children.as_mut_slice()
                && let Some((sheet, range)) = text.rsplit_once('!')
                && (sheet.eq_ignore_ascii_case(source) || sheet.eq_ignore_ascii_case(quoted_source))
                && range
                    .replace('$', "")
                    .parse::<crabxl_core::CellRange>()
                    .is_ok()
            {
                *text = format!("{destination}!{range}").into();
            }
        }
        _ => {}
    }
    for child in &mut element.children {
        if let XmlContent::Element(child) = child {
            retarget_chart_ranges(child, source, quoted_source, destination);
        }
    }
}

#[cfg(test)]
#[test]
fn copied_chart_redirects_only_local_sheet_ranges() {
    use crabxl_core::{XmlContent, XmlElement};
    let mut chart = XmlElement::new("c:chartSpace");
    for source in [
        "'O''Brien'!$A$1:$A$3",
        "'[Other.xlsx]O''Brien'!$A$1",
        "Other!$A$1",
    ] {
        let mut reference = XmlElement::new("c:numRef");
        let mut formula = XmlElement::new("c:f");
        formula.children.push(XmlContent::Text(source.into()));
        reference.children.push(XmlContent::Element(formula));
        chart.children.push(XmlContent::Element(reference));
    }
    retarget_chart_ranges(&mut chart, "O'Brien", "'O''Brien'", "'Copy'");
    let formulas: Vec<_> = chart
        .children
        .iter()
        .filter_map(|node| match node {
            XmlContent::Element(reference) => match reference.child("f")?.children.as_slice() {
                [XmlContent::Text(text)] => Some(text.as_ref()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        formulas,
        [
            "'Copy'!$A$1:$A$3",
            "'[Other.xlsx]O''Brien'!$A$1",
            "Other!$A$1"
        ]
    );
}
use crabxl_core::{SheetFeatures, WorkbookFeatures};

impl<R: Read + Seek> LoadedWorkbook<R> {
    fn reserve_feature_replacement(&mut self, old: usize, incoming: usize) -> Result<()> {
        let maximum = self
            .allowance
            .retained_data_bytes
            .min(self.options.workbook.max_bytes);
        let mapping = self
            .mapping_bytes()
            .saturating_sub(old)
            .saturating_add(incoming);
        // Old snapshots and the caller's replacement coexist until commit.
        let peak = self
            .mapping_bytes()
            .saturating_add(incoming)
            .saturating_add(self.bank.charged_bytes())
            .saturating_add(self.package_extra_bytes());
        self.editor
            .book
            .rebalance_strings_for_retained(peak, maximum)?;
        let available = maximum
            .checked_sub(mapping.saturating_add(self.editor.retained_package_bytes()))
            .ok_or_else(budget)?;
        self.bank.set_memory_allowance(available)
    }
    fn feature_index(&self, id: SheetId) -> Result<usize> {
        self.sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))
    }
    /// Read the worksheet's source views without materializing its cells.
    pub fn sheet_views(&mut self, id: SheetId) -> Result<crabxl_core::SheetViews> {
        let index = self.feature_index(id)?;
        if let Some(original) = self.sheets[index].original {
            let name = self.editor.book.sheets()[original].name().to_owned();
            if let Some(views) = self.editor.pending_sheet_views(&name) {
                return Ok(views.clone());
            }
            return self.editor.sheet_views(&name);
        }
        Ok(self
            .bank
            .sheet(id)?
            .sheet_views()
            .cloned()
            .unwrap_or_default())
    }

    /// Replace worksheet views while retaining unrelated original parts.
    pub fn set_sheet_views(&mut self, id: SheetId, views: crabxl_core::SheetViews) -> Result<()> {
        let index = self.feature_index(id)?;
        if let Some(original) = self.sheets[index].original {
            let name = self.editor.book.sheets()[original].name().to_owned();
            self.editor.set_sheet_views(&name, views)?;
        } else {
            self.bank.sheet_mut(id)?.set_sheet_views(Some(views))?;
        }
        Ok(())
    }

    /// Read canonical source or pending printing metadata without loading cells.
    pub fn print_settings(&mut self, id: SheetId) -> Result<crabxl_core::PrintSettings> {
        let index = self.feature_index(id)?;
        if let Some(original) = self.sheets[index].original {
            let name = self.editor.book.sheets()[original].name().to_owned();
            if let Some(settings) = self.editor.pending_print_settings(&name) {
                return Ok(settings.clone());
            }
            return self.editor.print_settings(&name);
        }
        Ok(self
            .bank
            .sheet(id)?
            .print_settings()
            .cloned()
            .unwrap_or_default())
    }

    /// Replace printing settings through the preserving package editor.
    pub fn set_print_settings(
        &mut self,
        id: SheetId,
        settings: crabxl_core::PrintSettings,
    ) -> Result<()> {
        let index = self.feature_index(id)?;
        if let Some(original) = self.sheets[index].original {
            let name = self.editor.book.sheets()[original].name().to_owned();
            self.editor.set_print_settings(&name, settings)?;
        } else {
            self.bank
                .sheet_mut(id)?
                .set_print_settings(Some(settings))?;
        }
        Ok(())
    }

    fn ensure_sheet_features(&mut self, index: usize) -> Result<()> {
        if self.sheets[index].features.is_some() {
            return Ok(());
        }
        let source = match self.sheets[index].original {
            Some(original) => {
                let name = self.editor.book.sheets()[original].name().to_owned();
                self.editor.book.sheet_features(&name)?
            }
            None => SheetFeatures::default(),
        };
        self.reserve_feature_replacement(
            0,
            size_of::<FeatureState<SheetFeatures>>() + source.heap_bytes().saturating_mul(2),
        )?;
        self.sheets[index].features = Some(Box::new(FeatureState {
            current: source.clone(),
            source,
        }));
        Ok(())
    }
    /// Tables, filters, rules, comments, protection and header/footer of a
    /// sheet. Cells are not materialized.
    pub fn sheet_features(&mut self, id: SheetId) -> Result<&SheetFeatures> {
        let index = self.feature_index(id)?;
        self.ensure_sheet_features(index)?;
        self.sheets[index]
            .features
            .as_ref()
            .map(|state| &state.current)
            .ok_or_else(|| Error::new(ErrorKind::InvalidState, "Missing sheet features"))
    }
    /// Replace the features of a sheet. Save writes them when they differ
    /// from the source.
    pub fn set_sheet_features(&mut self, id: SheetId, features: SheetFeatures) -> Result<()> {
        let index = self.feature_index(id)?;
        self.ensure_sheet_features(index)?;
        if self.sheets[index].kind == crate::SheetKind::Worksheet && features.chartsheet.is_some() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Chartsheet properties require a chartsheet",
            ));
        }
        if self.sheets[index].kind == crate::SheetKind::ChartSheet {
            let mut inline = features.clone();
            inline.chartsheet = None;
            inline.chartsheet_parts = None;
            inline.images = None;
            inline.charts = None;
            inline.shapes = None;
            if !inline.is_empty() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Edit chartsheet properties through its XML property tree",
                ));
            }
        }
        let old = self.sheets[index]
            .features
            .as_ref()
            .map_or(0, |state| state.current.heap_bytes());
        self.reserve_feature_replacement(old, features.heap_bytes())?;
        if let Some(state) = &mut self.sheets[index].features {
            state.current = features;
        }
        Ok(())
    }
    /// Pictures and anchors of a worksheet; cells stay lazy.
    pub fn sheet_images(&mut self, id: SheetId) -> Result<&[crabxl_core::Image]> {
        let index = self.feature_index(id)?;
        self.ensure_sheet_features(index)?;
        if self.sheets[index]
            .features
            .as_ref()
            .is_some_and(|state| state.current.images.is_none())
        {
            let images = match self.sheets[index].original {
                Some(original) => {
                    let name = self.editor.book.sheets()[original].name().to_owned();
                    self.editor.book.sheet_images(&name)?
                }
                None => Vec::new(),
            };
            let bytes = images
                .iter()
                .map(|item| size_of::<crabxl_core::Image>() + item.heap_bytes())
                .sum::<usize>();
            self.reserve_feature_replacement(0, bytes.saturating_mul(2))?;
            if let Some(state) = &mut self.sheets[index].features {
                state.source.images = Some(images.clone());
                state.current.images = Some(images);
            }
        }
        Ok(self.sheets[index]
            .features
            .as_ref()
            .and_then(|state| state.current.images.as_deref())
            .unwrap_or(&[]))
    }
    /// Replace worksheet pictures. Existing non-picture drawings reject on save.
    pub fn set_sheet_images(&mut self, id: SheetId, images: Vec<crabxl_core::Image>) -> Result<()> {
        for image in &images {
            image.anchor.validate()?;
        }
        self.sheet_images(id)?;
        let mut features = self.sheet_features(id)?.clone();
        features.images = Some(images);
        self.set_sheet_features(id, features)
    }
    /// Complete chart properties and placement; cells stay lazy.
    pub fn sheet_charts(&mut self, id: SheetId) -> Result<&[crabxl_core::drawing::Chart]> {
        let index = self.feature_index(id)?;
        self.ensure_sheet_features(index)?;
        if self.sheets[index]
            .features
            .as_ref()
            .is_some_and(|state| state.current.charts.is_none())
        {
            let charts = match self.sheets[index].original {
                Some(original) => {
                    let name = self.editor.book.sheets()[original].name().to_owned();
                    self.editor.book.sheet_charts(&name)?
                }
                None => Vec::new(),
            };
            let bytes = charts
                .iter()
                .map(|item| size_of::<crabxl_core::drawing::Chart>() + item.heap_bytes())
                .sum::<usize>();
            self.reserve_feature_replacement(0, bytes.saturating_mul(2))?;
            if let Some(state) = &mut self.sheets[index].features {
                state.source.charts = Some(charts.clone());
                state.current.charts = Some(charts);
            }
        }
        Ok(self.sheets[index]
            .features
            .as_ref()
            .and_then(|state| state.current.charts.as_deref())
            .unwrap_or(&[]))
    }
    /// Replace worksheet charts while retaining existing pictures.
    pub fn set_sheet_charts(
        &mut self,
        id: SheetId,
        charts: Vec<crabxl_core::drawing::Chart>,
    ) -> Result<()> {
        for chart in &charts {
            chart.anchor.validate()?;
        }
        self.sheet_charts(id)?;
        let mut features = self.sheet_features(id)?.clone();
        features.charts = Some(charts);
        self.set_sheet_features(id, features)
    }
    /// Shapes, connectors, grouped objects and placement; cells stay lazy.
    pub fn sheet_shapes(&mut self, id: SheetId) -> Result<&[crabxl_core::drawing::Shape]> {
        let index = self.feature_index(id)?;
        self.ensure_sheet_features(index)?;
        if self.sheets[index]
            .features
            .as_ref()
            .is_some_and(|state| state.current.shapes.is_none())
        {
            let shapes = match self.sheets[index].original {
                Some(original) => {
                    let name = self.editor.book.sheets()[original].name().to_owned();
                    self.editor.book.sheet_shapes(&name)?
                }
                None => Vec::new(),
            };
            let bytes = shapes
                .iter()
                .map(|item| size_of::<crabxl_core::drawing::Shape>() + item.heap_bytes())
                .sum::<usize>();
            self.reserve_feature_replacement(0, bytes.saturating_mul(2))?;
            if let Some(state) = &mut self.sheets[index].features {
                state.source.shapes = Some(shapes.clone());
                state.current.shapes = Some(shapes);
            }
        }
        Ok(self.sheets[index]
            .features
            .as_ref()
            .and_then(|state| state.current.shapes.as_deref())
            .unwrap_or(&[]))
    }
    /// Replace worksheet shapes while retaining existing pictures.
    pub fn set_sheet_shapes(
        &mut self,
        id: SheetId,
        shapes: Vec<crabxl_core::drawing::Shape>,
    ) -> Result<()> {
        for shape in &shapes {
            shape.anchor.validate()?;
        }
        self.sheet_shapes(id)?;
        let mut features = self.sheet_features(id)?.clone();
        features.shapes = Some(shapes);
        self.set_sheet_features(id, features)
    }
    /// Pivot tables and shared cache metadata, loaded independently of cells.
    pub fn sheet_pivots(&mut self, id: SheetId) -> Result<&[crabxl_core::features::PivotTable]> {
        let index = self.feature_index(id)?;
        self.ensure_sheet_features(index)?;
        if self.sheets[index]
            .features
            .as_ref()
            .is_some_and(|state| state.current.pivots.is_none())
        {
            let pivots = match self.sheets[index].original {
                Some(original) => {
                    let name = self.editor.book.sheets()[original].name().to_owned();
                    self.editor.book.sheet_pivots(&name)?
                }
                None => Vec::new(),
            };
            let bytes = pivots
                .iter()
                .map(|item| size_of::<crabxl_core::features::PivotTable>() + item.heap_bytes())
                .sum::<usize>();
            self.reserve_feature_replacement(0, bytes.saturating_mul(2))?;
            if let Some(state) = &mut self.sheets[index].features {
                state.source.pivots = Some(pivots.clone());
                state.current.pivots = Some(pivots);
            }
        }
        Ok(self.sheets[index]
            .features
            .as_ref()
            .and_then(|state| state.current.pivots.as_deref())
            .unwrap_or(&[]))
    }
    /// Replace pivot layouts and caches; cache records remain encoded sources.
    pub fn set_sheet_pivots(
        &mut self,
        id: SheetId,
        pivots: Vec<crabxl_core::features::PivotTable>,
    ) -> Result<()> {
        self.sheet_pivots(id)?;
        let mut features = self.sheet_features(id)?.clone();
        features.pivots = Some(pivots);
        self.set_sheet_features(id, features)
    }
    pub(super) fn copied_features(&mut self, id: SheetId, name: &str) -> Result<SheetFeatures> {
        let mut copy = self.sheet_features(id)?.clone();
        copy.header_footer = None;
        copy.images = Some(self.sheet_images(id)?.to_vec());
        let mut charts = self.sheet_charts(id)?.to_vec();
        let source = self.bank.sheet(id)?.name();
        let quoted_source = crabxl_core::features::quote_sheet_name(source);
        let quoted_name = crabxl_core::features::quote_sheet_name(name);
        for chart in &mut charts {
            retarget_chart_ranges(&mut chart.space, source, &quoted_source, &quoted_name);
        }
        copy.charts = Some(charts);
        copy.shapes = Some(self.sheet_shapes(id)?.to_vec());
        if self.sheets[self.feature_index(id)?].kind == crate::SheetKind::Worksheet {
            copy.pivots = Some(self.sheet_pivots(id)?.to_vec());
        }
        if copy.object_names().next().is_none() {
            return Ok(copy);
        }
        let ids: Vec<_> = self
            .sheets
            .iter()
            .filter(|sheet| sheet.kind == crate::SheetKind::Worksheet)
            .map(|sheet| sheet.id)
            .collect();
        let mut taken = std::collections::HashSet::new();
        for id in ids {
            self.sheet_pivots(id)?;
            taken.extend(
                self.sheet_features(id)?
                    .object_names()
                    .map(str::to_lowercase),
            );
        }
        self.ensure_workbook_features()?;
        if let Some(state) = &self.workbook_features {
            taken.extend(
                state
                    .current
                    .defined_names
                    .iter()
                    .map(|name| name.name.to_lowercase()),
            );
        }
        copy.rename_for_copy(&mut taken);
        Ok(copy)
    }
    fn ensure_workbook_features(&mut self) -> Result<()> {
        if self.workbook_features.is_none() {
            let source = self.editor.book.workbook_features()?;
            self.reserve_feature_replacement(
                0,
                size_of::<FeatureState<WorkbookFeatures>>() + source.heap_bytes().saturating_mul(2),
            )?;
            self.workbook_features = Some(Box::new(FeatureState {
                current: source.clone(),
                source,
            }));
        }
        Ok(())
    }
    /// Defined names, workbook protection and document properties.
    pub fn workbook_features(&mut self) -> Result<&WorkbookFeatures> {
        self.ensure_workbook_features()?;
        self.workbook_features
            .as_ref()
            .map(|state| &state.current)
            .ok_or_else(|| Error::new(ErrorKind::InvalidState, "Missing workbook features"))
    }
    /// Replace workbook features. Save writes them when they differ from the source.
    pub fn set_workbook_features(&mut self, features: WorkbookFeatures) -> Result<()> {
        self.ensure_workbook_features()?;
        let old = self
            .workbook_features
            .as_ref()
            .map_or(0, |state| state.current.heap_bytes());
        self.reserve_feature_replacement(old, features.heap_bytes())?;
        if let Some(state) = &mut self.workbook_features {
            state.current = features;
        }
        Ok(())
    }
    /// Ordered external links, loaded independently of worksheet cells.
    pub fn external_links(&mut self) -> Result<&[crabxl_core::features::ExternalLink]> {
        self.ensure_workbook_features()?;
        if self
            .workbook_features
            .as_ref()
            .is_some_and(|state| state.current.external_links.is_none())
        {
            let links = self.editor.book.external_links()?;
            let bytes = links
                .iter()
                .map(|item| size_of::<crabxl_core::features::ExternalLink>() + item.heap_bytes())
                .sum::<usize>();
            self.reserve_feature_replacement(0, bytes.saturating_mul(2))?;
            if let Some(state) = &mut self.workbook_features {
                state.source.external_links = Some(links.clone());
                state.current.external_links = Some(links);
            }
        }
        Ok(self
            .workbook_features
            .as_ref()
            .and_then(|state| state.current.external_links.as_deref())
            .unwrap_or(&[]))
    }
    /// Replace external links in reference order. Formula text is not rewritten.
    pub fn set_external_links(
        &mut self,
        links: Vec<crabxl_core::features::ExternalLink>,
    ) -> Result<()> {
        let mut features = self.workbook_features()?.clone();
        features.external_links = Some(links);
        self.set_workbook_features(features)
    }
    /// Drop external link graphs. As in openpyxl, formulas and names keep their text.
    pub fn remove_external_links(&mut self) -> Result<()> {
        self.set_external_links(Vec::new())
    }
    /// XLSX-family format declared by the original package.
    pub fn source_workbook_format(&self) -> crabxl_core::features::WorkbookFormat {
        self.editor.book.workbook_format()
    }
    /// Set an explicit XLSX-family format for the next save.
    pub fn set_workbook_format(
        &mut self,
        format: crabxl_core::features::WorkbookFormat,
    ) -> Result<()> {
        let mut features = self.workbook_features()?.clone();
        features.format = Some(format);
        self.set_workbook_features(features)
    }
    /// Change the date epoch and ISO date storage. Date cells written after this
    /// use the new settings; cells that are not written again keep their serials.
    pub fn set_date_options(&mut self, epoch: DateEpoch, iso_dates: bool) -> Result<()> {
        if epoch != self.bank.epoch() {
            // The epoch is saved as `workbookPr/@date1904`, so the element must exist.
            let mut features = self.workbook_features()?.clone();
            features
                .workbook_properties
                .get_or_insert_with(|| crabxl_core::XmlElement::new("workbookPr"))
                .set_attribute(
                    "date1904",
                    if epoch == DateEpoch::Mac1904 {
                        "1"
                    } else {
                        "0"
                    },
                );
            self.set_workbook_features(features)?;
        }
        self.bank.set_epoch(epoch);
        self.editor.set_date_options(epoch, iso_dates);
        Ok(())
    }
    /// Preserve, remove or replace the original VBA project graph.
    pub fn set_macro_policy(&mut self, policy: crabxl_core::features::MacroPolicy) -> Result<()> {
        let mut features = self.workbook_features()?.clone();
        features.macros = policy;
        self.set_workbook_features(features)
    }
    /// Metadata and rich-value graph roots, loaded without worksheet cells.
    pub fn workbook_metadata(&mut self) -> Result<&[crabxl_core::features::WorkbookMetadata]> {
        self.ensure_workbook_features()?;
        if self
            .workbook_features
            .as_ref()
            .is_some_and(|state| state.current.metadata.is_none())
        {
            let roots = self.editor.book.workbook_metadata()?;
            let bytes = roots
                .iter()
                .map(|item| {
                    size_of::<crabxl_core::features::WorkbookMetadata>() + item.heap_bytes()
                })
                .sum::<usize>();
            self.reserve_feature_replacement(0, bytes.saturating_mul(2))?;
            if let Some(state) = &mut self.workbook_features {
                state.source.metadata = Some(roots.clone());
                state.current.metadata = Some(roots);
            }
        }
        Ok(self
            .workbook_features
            .as_ref()
            .and_then(|state| state.current.metadata.as_deref())
            .unwrap_or(&[]))
    }
    /// Replace workbook metadata roots. Existing cell indices must remain valid.
    pub fn set_workbook_metadata(
        &mut self,
        metadata: Vec<crabxl_core::features::WorkbookMetadata>,
    ) -> Result<()> {
        let mut features = self.workbook_features()?.clone();
        features.metadata = Some(metadata);
        self.set_workbook_features(features)
    }
    /// Read the original encoded VBA project without executing it.
    pub fn vba_project(&mut self) -> Result<Option<crabxl_core::features::VbaProject>> {
        if let Some(state) = &self.workbook_features {
            match &state.current.macros {
                crabxl_core::features::MacroPolicy::Remove => return Ok(None),
                crabxl_core::features::MacroPolicy::Replace(project) => {
                    return Ok(Some(project.clone()));
                }
                _ => {}
            }
        }
        self.editor.book.vba_project()
    }
    /// Add a differential style (`dxfId`) for conditional formats and tables.
    /// Equal styles share one index; the stylesheet is rewritten on save.
    pub fn register_differential_style(
        &mut self,
        style: crabxl_core::DifferentialStyle,
    ) -> Result<u32> {
        let before = self
            .bank
            .style_catalog()
            .map_or(0, |catalog| catalog.differential_styles.len());
        let id = self.bank.register_differential_style(style)?;
        let after = self
            .bank
            .style_catalog()
            .map_or(0, |catalog| catalog.differential_styles.len());
        if after != before {
            self.editor.styles_changed();
        }
        Ok(id)
    }
    /// Differential styles of the workbook, indexed by `dxfId`.
    pub fn differential_styles(&self) -> &[crabxl_core::DifferentialStyle] {
        self.bank
            .style_catalog()
            .map_or(&[], |catalog| catalog.differential_styles.as_slice())
    }
    pub(super) fn prepare_features_for_save(&mut self) -> Result<()> {
        for sheet in &self.sheets {
            let Some(state) = &sheet.features else {
                continue;
            };
            if state.current == state.source {
                continue;
            }
            match sheet.original {
                Some(original) => self
                    .editor
                    .set_sheet_features_at(original, state.current.clone())?,
                None => self
                    .editor
                    .set_created_sheet_features(sheet.id, state.current.clone()),
            }
        }
        self.save_workbook_features()
    }
    /// Keep `localSheetId` of defined names on the same sheets after sheets
    /// were created, removed or moved. Names of removed sheets are dropped.
    fn save_workbook_features(&mut self) -> Result<()> {
        let source_count = self.editor.book.sheets().len();
        let mut position = vec![None; source_count];
        for (index, (id, _)) in self.bank.sheets().enumerate() {
            if let Some(original) = self
                .sheets
                .iter()
                .find(|sheet| sheet.id == id)
                .and_then(|sheet| sheet.original)
                && let Some(slot) = position.get_mut(original)
            {
                *slot = Some(index as u32);
            }
        }
        let identity = position
            .iter()
            .enumerate()
            .all(|(index, slot)| *slot == Some(index as u32));
        if !identity {
            self.ensure_workbook_features()?;
        }
        let Some(state) = &self.workbook_features else {
            return Ok(());
        };
        let mut effective = state.current.clone();
        if !identity {
            // Names from the source use source positions; names the caller
            // added already use the current positions.
            effective.defined_names = state
                .current
                .defined_names
                .iter()
                .filter_map(|name| match name.local_sheet_id {
                    Some(old) if state.source.defined_names.contains(name) => {
                        position.get(old as usize).copied().flatten().map(|new| {
                            let mut name = name.clone();
                            name.local_sheet_id = Some(new);
                            name
                        })
                    }
                    _ => Some(name.clone()),
                })
                .collect();
        }
        if effective != state.source {
            self.editor.set_workbook_features(effective);
        }
        Ok(())
    }
}
