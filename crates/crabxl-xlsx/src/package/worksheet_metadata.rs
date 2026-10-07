// SPDX-License-Identifier: MIT
// Workbook/relationship parsing adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

//! Worksheet metadata operations.
use super::*;

impl<R: Read + Seek> WorkbookReader<R> {
    /// Read the declared worksheet dimension without loading cells. Missing
    /// dimensions are returned as None; callers can stream to calculate them.
    /// Stops at sheetData and does not validate unread worksheet bytes or CRC.
    pub fn worksheet_dimension(&mut self, name: &str) -> Result<Option<crabxl_core::CellRange>> {
        let sheet = self
            .sheets
            .iter()
            .find(|sheet| sheet.name == name)
            .ok_or_else(|| Error::new(ErrorKind::SheetNotFound, "Worksheet not found"))?;
        if sheet.kind != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Only worksheets have cell dimensions",
            ));
        }
        let part = sheet.part.clone();
        let file = self.archive.by_name(&part).map_err(|error| {
            Error::caused_by(ErrorKind::Archive, "Cannot open worksheet", error).with_part(&part)
        })?;
        let mut xml = XmlStream::new(
            BufReader::with_capacity(self.limits.input_buffer_bytes, file),
            part,
            self.limits.max_part_bytes,
            self.limits,
        );
        loop {
            let frame = xml.next()?;
            match frame.event {
                Event::Start(element)
                    if frame.scope == Scope::Spreadsheet
                        && element.local_name().as_ref().as_bytes() == b"dimension"
                        && frame.depth == 2 =>
                {
                    return required_attribute(&element, b"ref")?.parse().map(Some);
                }
                Event::Start(element)
                    if frame.scope == Scope::Spreadsheet
                        && element.local_name().as_ref().as_bytes() == b"sheetData"
                        && frame.depth == 2 =>
                {
                    return Ok(None);
                }
                Event::Eof => return Ok(None),
                _ => {}
            }
        }
    }
    /// Read bounded worksheet viewport metadata without materializing cells.
    /// Stops at the views container or sheetData: the unconsumed payload and CRC
    /// are not validated. Original-package save can validate every affected part.
    pub fn sheet_views(&mut self, name: &str) -> Result<crabxl_core::SheetViews> {
        self.sheet_views_with_allowance(name, usize::MAX)
    }
    pub(crate) fn sheet_views_with_allowance(
        &mut self,
        name: &str,
        allowance: usize,
    ) -> Result<crabxl_core::SheetViews> {
        let info = self
            .sheets
            .iter()
            .find(|sheet| sheet.name() == name)
            .ok_or_else(|| {
                Error::new(ErrorKind::SheetNotFound, "Worksheet view source not found")
            })?;
        if info.kind() != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Worksheet views require a cell worksheet",
            ));
        }
        let part = info.part().to_owned();
        let file = self.archive.by_name(&part).map_err(|error| {
            Error::caused_by(
                ErrorKind::Archive,
                "Cannot open worksheet view source",
                error,
            )
            .with_part(part.clone())
        })?;
        let maximum = usize::try_from(self.limits.max_metadata_bytes)
            .unwrap_or(usize::MAX)
            .min(allowance);
        let bytes = file
            .size()
            .min(self.limits.max_metadata_bytes)
            .min(self.limits.max_part_bytes);
        let mut xml = XmlStream::new(
            BufReader::with_capacity(self.limits.input_buffer_bytes, file),
            part,
            bytes,
            self.limits,
        );
        crate::worksheet_view::read_header(&mut xml, maximum)
    }
    /// Read sparse column declarations from the worksheet header without cells.
    /// Stops at sheetData; unread payload and ZIP CRC are not validated.
    pub fn column_dimensions(&mut self, name: &str) -> Result<crabxl_core::SheetDimensions> {
        self.column_dimensions_with_allowance(name, usize::MAX)
    }
    pub(crate) fn column_dimensions_with_allowance(
        &mut self,
        name: &str,
        allowance: usize,
    ) -> Result<crabxl_core::SheetDimensions> {
        let info = self
            .sheets
            .iter()
            .find(|sheet| sheet.name() == name)
            .ok_or_else(|| {
                Error::new(ErrorKind::SheetNotFound, "Worksheet view source not found")
            })?;
        if info.kind() != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Worksheet views require a cell worksheet",
            ));
        }
        let part = info.part().to_owned();
        let file = self.archive.by_name(&part).map_err(|error| {
            Error::caused_by(
                ErrorKind::Archive,
                "Cannot open worksheet view source",
                error,
            )
            .with_part(part.clone())
        })?;
        let maximum = usize::try_from(self.limits.max_metadata_bytes)
            .unwrap_or(usize::MAX)
            .min(allowance);
        let bytes = file
            .size()
            .min(self.limits.max_metadata_bytes)
            .min(self.limits.max_part_bytes);
        let mut xml = XmlStream::new(
            BufReader::with_capacity(self.limits.input_buffer_bytes, file),
            part,
            bytes,
            self.limits,
        );
        crate::dimension_codec::read_columns(&mut xml, maximum)
    }
    /// Read printing metadata through worksheet EOF/CRC without materializing cells.
    /// Full decompression is necessary because printing elements follow sheetData.
    pub fn print_settings(&mut self, name: &str) -> Result<crabxl_core::PrintSettings> {
        self.print_settings_with_allowance(name, usize::MAX)
    }
    pub(crate) fn print_settings_with_allowance(
        &mut self,
        name: &str,
        allowance: usize,
    ) -> Result<crabxl_core::PrintSettings> {
        let info = self
            .sheets
            .iter()
            .find(|sheet| sheet.name() == name)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::SheetNotFound,
                    "Printing worksheet source not found",
                )
            })?;
        if info.kind() != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Printing metadata requires a cell worksheet",
            ));
        }
        let part = info.part().to_owned();
        let file = self.archive.by_name(&part).map_err(|error| {
            Error::caused_by(
                ErrorKind::Archive,
                "Cannot open printing worksheet source",
                error,
            )
            .with_part(part.clone())
        })?;
        let maximum = usize::try_from(self.limits.max_metadata_bytes)
            .unwrap_or(usize::MAX)
            .min(allowance);
        let mut xml = XmlStream::new(
            BufReader::with_capacity(self.limits.input_buffer_bytes, file),
            part,
            self.limits.max_part_bytes,
            self.limits,
        );
        crate::printing::read(&mut xml, maximum)
    }
}
