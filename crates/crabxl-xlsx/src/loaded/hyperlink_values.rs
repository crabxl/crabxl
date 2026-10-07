//! Normal-model display initialization over compact source declarations.
use super::*;
use std::sync::Arc;

pub(super) fn bind(
    incoming: &mut Worksheet,
    links: &crabxl_core::Hyperlinks,
    resources: ResourceLimits,
    maximum: usize,
    mut overridden: impl FnMut(CellAddress) -> bool,
) -> Result<bool> {
    let mut changed = false;
    for (owner, link) in links.iter() {
        let Some(source) = link
            .target
            .as_ref()
            .filter(|value| !value.is_empty())
            .or(link.location.as_ref())
        else {
            continue;
        };
        let area = links.covering_range(owner).unwrap_or(CellRange {
            start: owner,
            end: owner,
        });
        let mut missing = false;
        'search: for row in area.start.row.get()..=area.end.row.get() {
            for column in area.start.column.get()..=area.end.column.get() {
                let address = CellAddress::new(row, column)?;
                if !overridden(address) && needs_value(incoming, address) {
                    missing = true;
                    break 'search;
                }
            }
        }
        if !missing {
            continue;
        }
        if links.has_source_overlaps() {
            return Err(Error::new(ErrorKind::Unsupported, "Initializing empty cells from overlapping source hyperlink declarations remains unimplemented").with_cell(owner));
        }
        let bytes = if source.len() <= 32767 {
            source.len()
        } else {
            source
                .char_indices()
                .nth(32767)
                .map_or(source.len(), |(offset, _)| offset)
        };
        let scratch = bytes.saturating_mul(2).saturating_add(64);
        if scratch > maximum.saturating_sub(incoming.charged_bytes()) {
            return Err(budget().with_cell(owner));
        }
        let CellValue::Text(text) = link.initial_cell_value() else {
            continue;
        };
        if text.as_str().len() > resources.max_cell_bytes {
            return Err(Error::new(
                ErrorKind::LimitExceeded,
                "Hyperlink display exceeds configured cell byte limit",
            )
            .with_cell(owner));
        }
        let text: Arc<str> = Arc::from((*text).into_string());
        for row in area.start.row.get()..=area.end.row.get() {
            for column in area.start.column.get()..=area.end.column.get() {
                let address = CellAddress::new(row, column)?;
                if overridden(address) || !needs_value(incoming, address) {
                    continue;
                }
                incoming.set(Cell {
                    address,
                    style: incoming.style_at(address),
                    value: CellValue::shared_text(Arc::clone(&text)),
                })?;
                changed = true;
            }
        }
    }
    Ok(changed)
}

fn needs_value(incoming: &Worksheet, address: CellAddress) -> bool {
    incoming.merged_ranges().virtual_style(address).is_none()
        && incoming
            .get(address)
            .is_none_or(|cell| matches!(cell.value, CellValue::Empty))
}

impl<R: Read + Seek> LoadedWorkbook<R> {
    pub(super) fn prepare_hyperlink_values_for_save(&mut self) -> Result<()> {
        if !self.options.bind_hyperlink_values {
            return Ok(());
        }
        // Unrequested scalar sheets keep their original package-backed model.
        // Only source sheets with actual declarations need value materialization.
        for index in 0..self.sheets.len() {
            if self.sheets[index].loaded
                || self.sheets[index].original.is_none()
                || self.sheets[index].kind != crate::SheetKind::Worksheet
            {
                continue;
            }
            self.rebalance()?;
            let available = self
                .allowance
                .retained_data_bytes
                .min(self.options.workbook.max_bytes)
                .saturating_sub(self.managed_retained_bytes());
            let links = self
                .editor
                .book
                .hyperlinks_with_allowance(self.sheets[index].name.as_ref(), available)?;
            let has_links = !links.is_empty();
            drop(links);
            if has_links {
                self.sheet(self.sheets[index].id)?;
            }
        }
        for index in 0..self.sheets.len() {
            if !self.sheets[index].hyperlink_values_dirty || self.sheets[index].original.is_none() {
                continue;
            }
            if self.options.read.data_only {
                return Err(Error::new(
                    ErrorKind::Unsupported,
                    "Saving initialized data-only hyperlink values remains unimplemented",
                ));
            }
            let id = self.sheets[index].id;
            let plan = self
                .editor
                .prepare_model(self.sheets[index].name.as_ref())?;
            crate::loaded_codec::validate_model(self.bank.sheet(id)?, self.bank.style_catalog())?;
            self.reserve_workbook_patch(plan.bytes.max(self.editor.patch_bytes()))?;
            self.commit_normalized_model(plan, id);
        }
        self.rebalance()
    }
}
