//! Worksheet and workbook features of a loaded workbook. Features load on
//! first access, independently of cell materialization.
use super::*;
use crabxl_core::{SheetFeatures, WorkbookFeatures};

impl<R: Read + Seek> LoadedWorkbook<R> {
    fn feature_index(&self, id: SheetId) -> Result<usize> {
        self.sheets
            .iter()
            .position(|sheet| sheet.id == id)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Unknown loaded sheet identity"))
    }
    fn ensure_sheet_features(&mut self, index: usize) -> Result<()> {
        if self.sheets[index].features.is_some() {
            return Ok(());
        }
        if self.sheets[index].kind != crate::SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Features require a cell worksheet",
            ));
        }
        let source = match self.sheets[index].original {
            Some(original) => {
                let name = self.editor.book.sheets()[original].name().to_owned();
                self.editor.book.sheet_features(&name)?
            }
            None => SheetFeatures::default(),
        };
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
        if let Some(state) = &mut self.sheets[index].features {
            state.current = features;
        }
        Ok(())
    }
    fn ensure_workbook_features(&mut self) -> Result<()> {
        if self.workbook_features.is_none() {
            let source = self.editor.book.workbook_features()?;
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
        if let Some(state) = &mut self.workbook_features {
            state.current = features;
        }
        Ok(())
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
