// SPDX-License-Identifier: MIT
// Sequential spooling, scalar XML layouts and packaging adapted from rust_xlsxwriter,
// Copyright 2022-2026 John McNamara. Source provenance: third_party/ports.json.

//! Finish operations for the single workbook writer owner.
use super::*;

impl WorkbookWriter {
    /// Return progress and peak logical temporary storage.
    pub fn stats(&self) -> WriteStats {
        self.stats
    }
    /// Set the ZIP level for packaging without changing worksheet spools.
    pub fn set_compression_level(&mut self, level: Option<u8>) -> Result<()> {
        self.ensure_open()?;
        validate_compression_level(level)?;
        self.options.compression_level = level;
        Ok(())
    }
    /// Set the active display sheet before packaging. Finish validates the index.
    pub fn set_active_sheet(&mut self, index: usize) -> Result<()> {
        self.ensure_open()?;
        self.options.active_sheet = index;
        self.view_index = None;
        Ok(())
    }
    /// Request a relative, hidden or out-of-range view for compatible output.
    /// This does not change which temporary worksheet receives new rows.
    pub fn set_active_view_index(&mut self, index: i64) -> Result<()> {
        self.ensure_open()?;
        self.view_index = Some(index);
        Ok(())
    }
    /// Preflight a deferred view against stored, paused and active sheet states.
    /// No temporary files are packaged and no output sink is touched.
    pub fn active_view_selection(&self) -> Result<crabxl_core::ActiveViewSelection> {
        self.ensure_open()?;
        crabxl_core::normalize_active_view(
            self.view_index.unwrap_or(self.options.active_sheet as i64),
            self.next_sheet,
            |index| {
                self.sheets
                    .iter()
                    .find(|sheet| sheet.id == index)
                    .map(|sheet| sheet.visibility)
                    .or_else(|| {
                        self.paused
                            .iter()
                            .find(|sheet| sheet.id == index)
                            .map(|sheet| sheet.visibility)
                    })
                    .or_else(|| {
                        self.active
                            .as_ref()
                            .filter(|sheet| sheet.id == index)
                            .map(|sheet| sheet.visibility)
                    })
                    .unwrap_or(crabxl_core::SheetVisibility::Visible)
            },
        )
    }
    /// Set temporal storage before any rows are committed. Already serialized
    /// values cannot be reinterpreted by changing the epoch or ISO policy.
    pub fn set_temporal_options(&mut self, date_1904: bool, iso_dates: bool) -> Result<()> {
        self.ensure_open()?;
        if self.stats.rows != 0 {
            return Err(state(
                "Temporal options cannot change after rows are written",
            ));
        }
        self.options.date_1904 = date_1904;
        self.options.iso_dates = iso_dates;
        Ok(())
    }
    /// Bytes currently retained in owned temporary worksheets, including buffers.
    pub fn temporary_bytes(&self) -> u64 {
        self.temporary_bytes
    }
    /// Remove all owned temporary files without producing a workbook. Idempotent;
    /// attempts every cleanup even if one fails and retains failed paths for retry.
    /// Caller output has not been opened.
    pub fn abort(&mut self) -> Result<()> {
        self.aborted = true;
        let mut first_error = None;
        let mut remaining = Vec::new();
        for path in self.cleanup_paths.drain(..) {
            if let Err(error) = std::fs::remove_file(&path)
                && error.kind() != io::ErrorKind::NotFound
            {
                if first_error.is_none() {
                    first_error = Some(io_error(
                        "Cannot retry worksheet temporary-file cleanup",
                        error,
                    ));
                }
                remaining.push(path);
            }
        }
        let mut cleanup = |file: NamedTempFile| {
            let path = file.path().to_owned();
            if let Err(error) = file.close()
                && error.kind() != io::ErrorKind::NotFound
            {
                if first_error.is_none() {
                    first_error = Some(io_error("Cannot remove writer temporary file", error));
                }
                remaining.push(path);
            }
        };
        for active in self.active.take().into_iter().chain(self.paused.drain(..)) {
            let (file, _) = active.output.into_parts();
            cleanup(file);
            if let Some(events) = active.live_events {
                cleanup(events.into_file());
            }
            for file in active
                .link_spool
                .into_iter()
                .flat_map(hyperlinks::LinkSpool::into_files)
            {
                cleanup(file);
            }
        }
        for sheet in self.sheets.drain(..) {
            cleanup(sheet.file);
            if let Some(events) = sheet.live_events {
                cleanup(events.into_file());
            }
            if let Some(file) = sheet.relationship_spool {
                cleanup(file);
            }
        }
        if let Some(store) = self.live_links.take() {
            for file in store.into_files() {
                cleanup(file);
            }
        }
        self.cleanup_paths = remaining;
        self.sheets = Vec::new();
        self.paused = Vec::new();
        self.styles = None;
        self.options.theme = crate::ThemeWritePolicy::Omit;
        self.row_buffer.data = Vec::new();
        self.temporary_bytes = 0;
        first_error.map_or(Ok(()), Err)
    }
    /// Package completed worksheets and return the output sink. An I/O failure
    /// may leave partial bytes in caller output; abort/Drop never imply save.
    pub fn finish<W: Write + Seek>(self, output: W) -> Result<W> {
        self.finish_with_hyperlink_ids(output, |_, _| Ok(()))
    }
    /// Package live metadata and report writer-local group output identities in
    /// worksheet creation and owner order. Apply public IDs only after success.
    pub fn finish_with_hyperlink_ids<W: Write + Seek>(
        mut self,
        output: W,
        mut identity: impl FnMut(u64, Option<&str>) -> Result<()>,
    ) -> Result<W> {
        self.close_sheet()?;
        while let Some(active) = self.paused.pop() {
            self.active = Some(active);
            self.close_sheet()?;
        }
        self.sheets.sort_by_key(|sheet| sheet.id);
        if self.sheets.is_empty() {
            return Err(state("A workbook requires at least one worksheet"));
        }
        let active_view = if self.view_index.is_some() {
            Some(self.active_view_selection()?)
        } else {
            None
        };
        if active_view.is_none() && self.options.active_sheet >= self.sheets.len() {
            return Err(state("Active sheet index is outside the completed catalog"));
        }
        let first_visible = self
            .sheets
            .iter()
            .position(|sheet| sheet.visibility == crabxl_core::SheetVisibility::Visible)
            .ok_or_else(|| state("A workbook requires at least one visible sheet"))?;
        if active_view.is_none()
            && self.sheets[self.options.active_sheet].visibility
                != crabxl_core::SheetVisibility::Visible
        {
            self.options.active_sheet = self
                .sheets
                .iter()
                .enumerate()
                .skip(self.options.active_sheet)
                .find(|(_, sheet)| sheet.visibility == crabxl_core::SheetVisibility::Visible)
                .map_or(first_visible, |(index, _)| index);
        }
        let mut zip = ZipWriter::new(output);
        let options = compression_options(self.options.compression_level);
        let live_maximum = self
            .options
            .max_metadata_bytes
            .saturating_sub(self.catalog_bytes())
            .saturating_sub(self.style_memory_bytes());
        for (index, sheet) in self.sheets.iter_mut().enumerate() {
            let part = format!("xl/worksheets/sheet{}.xml", index + 1);
            let size = sheet
                .file
                .as_file()
                .metadata()
                .map_err(|error| {
                    io_error("Cannot inspect worksheet temporary file", error).with_part(&part)
                })?
                .len();
            start_part(
                &mut zip,
                &part,
                options.large_file(sheet.live_events.is_some() || size >= u64::from(u32::MAX)),
            )?;
            sheet.file.rewind().map_err(|error| {
                io_error("Cannot rewind worksheet temporary file", error).with_part(&part)
            })?;
            let external = if let Some(events) = &mut sheet.live_events {
                use std::io::Read;
                let body = size
                    .checked_sub(FOOTER.len() as u64)
                    .ok_or_else(|| state("Invalid deferred hyperlink worksheet footer"))?;
                io::copy(&mut sheet.file.as_file_mut().take(body), &mut zip)
                    .map_err(|cause| io_error("Cannot package live hyperlink worksheet", cause))?;
                zip.write_all(b"</sheetData>")
                    .map_err(|cause| io_error("Cannot close worksheet cells", cause))?;
                let maximum = live_maximum;
                let store = self
                    .live_links
                    .as_mut()
                    .ok_or_else(|| state("Missing live hyperlink store"))?;
                let external = live_hyperlinks::write_events(
                    &mut zip,
                    events,
                    store,
                    maximum,
                    false,
                    self.options
                        .max_sheet_bytes
                        .saturating_sub(body + FOOTER.len() as u64),
                    &mut identity,
                )?;
                zip.write_all(b"</worksheet>")
                    .map_err(|cause| io_error("Cannot close live worksheet", cause))?;
                external
            } else {
                io::copy(
                    &mut BufReader::with_capacity(
                        self.options.buffer_bytes,
                        sheet.file.as_file_mut(),
                    ),
                    &mut zip,
                )
                .map_err(|error| {
                    io_error("Cannot package worksheet temporary file", error).with_part(&part)
                })?;
                false
            };
            if external {
                let part = format!("xl/worksheets/_rels/sheet{}.xml.rels", index + 1);
                start_part(&mut zip, &part, options.large_file(true))?;
                let maximum = live_maximum;
                live_hyperlinks::write_events(
                    &mut zip,
                    sheet
                        .live_events
                        .as_mut()
                        .ok_or_else(|| state("Missing live hyperlink events"))?,
                    self.live_links
                        .as_mut()
                        .ok_or_else(|| state("Missing live hyperlink store"))?,
                    maximum,
                    true,
                    self.options.max_sheet_bytes,
                    |_, _| Ok(()),
                )?;
            }
            if let Some(file) = &mut sheet.relationship_spool {
                let part = format!("xl/worksheets/_rels/sheet{}.xml.rels", index + 1);
                let size = file
                    .as_file()
                    .metadata()
                    .map_err(|cause| io_error("Cannot inspect hyperlink spool", cause))?
                    .len();
                start_part(
                    &mut zip,
                    &part,
                    options.large_file(sheet.live_events.is_some() || size >= u64::from(u32::MAX)),
                )?;
                file.rewind()
                    .map_err(|cause| io_error("Cannot rewind hyperlink relationships", cause))?;
                io::copy(file.as_file_mut(), &mut zip).map_err(|cause| {
                    io_error("Cannot package hyperlink relationships", cause).with_part(&part)
                })?;
            }
            if let Some(relationships) = &sheet.relationships {
                let relationship_part = format!("xl/worksheets/_rels/sheet{}.xml.rels", index + 1);
                start_part(&mut zip, &relationship_part, options)?;
                zip.write_all(relationships).map_err(|error| {
                    io_error("Cannot package hyperlink relationships", error)
                        .with_part(relationship_part)
                })?;
            }
        }
        package_metadata(
            &mut zip,
            &self.sheets,
            self.styles
                .as_ref()
                .ok_or_else(|| state("Writer style catalog is released"))?
                .catalog(),
            &self.options,
            options,
            active_view,
        )?;
        let mut output = zip
            .finish()
            .map_err(|error| zip_error("Cannot finalize XLSX ZIP", error))?;
        output
            .flush()
            .map_err(|error| io_error("Cannot flush XLSX output", error))?;
        self.abort()?;
        Ok(output)
    }
}
