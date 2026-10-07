// SPDX-License-Identifier: MIT
// Workbook/relationship parsing adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

//! Streams operations.
use super::*;

impl<R: Read + Seek> WorkbookReader<R> {
    /// Explicitly load an entire sparse numeric worksheet into owned memory.
    ///
    /// Uses the same validated decoder and value restrictions as streaming.
    /// Retained vector capacities must fit `max_materialized_bytes`; one bounded
    /// current row and parser/catalog allocations are additional working memory.
    /// Useful for repeated in-memory access; first-pass decoding is not faster
    /// merely because all output is retained. Errors discard partial output.
    pub fn read_sheet(&mut self, name: &str) -> Result<SheetData> {
        self.collect_sheet(name, self.limits.max_materialized_bytes)
    }
    /// Materialize selected cells using the same rich/date/formula policies as streaming.
    /// Catalogs and one current row remain separately bounded working allocations.
    pub fn read_sheet_with_options(
        &mut self,
        name: &str,
        options: ReadOptions,
    ) -> Result<SheetData> {
        self.collect_sheet_options(name, self.limits.max_materialized_bytes, options)
    }
    pub(crate) fn collect_sheet(&mut self, name: &str, maximum: usize) -> Result<SheetData> {
        self.collect_sheet_options(name, maximum, ReadOptions::default())
    }
    pub(super) fn collect_sheet_options(
        &mut self,
        name: &str,
        maximum: usize,
        options: ReadOptions,
    ) -> Result<SheetData> {
        self.collect_sheet_with_allowance(name, maximum, options, None)
    }
    pub(crate) fn collect_sheet_with_allowance(
        &mut self,
        name: &str,
        maximum: usize,
        options: ReadOptions,
        allowance: Option<usize>,
    ) -> Result<SheetData> {
        let part = self
            .sheets
            .iter()
            .find(|sheet| sheet.name == name)
            .map(|sheet| sheet.part.clone());
        let materialization_limit = || {
            let error = Error::new(
                ErrorKind::MemoryBudgetExceeded,
                "Materialized sheet allocation exceeds the configured budget",
            );
            match &part {
                Some(part) => error.with_part(part),
                None => error,
            }
        };
        let mut stream =
            self.rows_with_catalog_allowance(name, options, allowance, None, 0, true)?;
        let mut sheet = SheetData { rows: Vec::new() };
        let mut cell_bytes = 0usize;
        while let Some(row) = stream.next_row()? {
            let row_bytes = row.memory_bytes().saturating_sub(size_of::<Row>());
            cell_bytes = cell_bytes
                .checked_add(row_bytes)
                .ok_or_else(materialization_limit)?;
            let row_allowance = maximum.min(stream.available_retained_bytes()?);
            let available_rows = row_allowance
                .saturating_sub(size_of::<SheetData>())
                .saturating_sub(cell_bytes)
                / size_of::<Row>();
            if sheet.rows.len() == sheet.rows.capacity() {
                let wanted = sheet
                    .rows
                    .capacity()
                    .saturating_mul(2)
                    .max(16)
                    .min(available_rows);
                if wanted <= sheet.rows.len() {
                    return Err(materialization_limit());
                }
                stream.set_aggregate_retained(
                    size_of::<SheetData>()
                        .saturating_add(wanted.saturating_mul(size_of::<Row>()))
                        .saturating_add(cell_bytes),
                )?;
                sheet
                    .rows
                    .try_reserve_exact(wanted - sheet.rows.len())
                    .map_err(|e| {
                        let error = Error::caused_by(
                            ErrorKind::LimitExceeded,
                            "Cannot allocate materialized sheet",
                            e,
                        );
                        match &part {
                            Some(part) => error.with_part(part),
                            None => error,
                        }
                    })?;
            }
            let retained = size_of::<SheetData>()
                .saturating_add(sheet.rows.capacity().saturating_mul(size_of::<Row>()))
                .saturating_add(cell_bytes);
            if retained > row_allowance {
                return Err(materialization_limit());
            }
            stream.set_aggregate_retained(retained)?;
            sheet.rows.push(row);
        }
        if sheet.memory_bytes() > maximum {
            return Err(materialization_limit());
        }
        Ok(sheet)
    }

    /// Stream all present numeric/empty cells in a worksheet.
    pub fn rows(&mut self, name: &str) -> Result<Rows<'_, R>> {
        self.rows_with_options(name, ReadOptions::default())
    }
    /// Stream selected sparse rows and columns, without decoding excluded cells.
    pub fn rows_with_options(&mut self, name: &str, options: ReadOptions) -> Result<Rows<'_, R>> {
        self.rows_with_allowance(name, options, None)
    }
    pub(crate) fn rows_with_allowance(
        &mut self,
        name: &str,
        options: ReadOptions,
        allowance: Option<usize>,
    ) -> Result<Rows<'_, R>> {
        self.rows_with_catalog_allowance(name, options, allowance, None, 0, false)
    }
    pub(crate) fn rows_with_catalog_allowance<'a>(
        &'a mut self,
        name: &str,
        options: ReadOptions,
        allowance: Option<usize>,
        catalog: Option<&'a crabxl_core::StyleCatalog>,
        retained: usize,
        share_values: bool,
    ) -> Result<Rows<'a, R>> {
        if self.styles_transferred && self.imported_styles.is_some() && catalog.is_none() {
            return Err(invalid(
                "Loaded row decoding requires its canonical bank styles",
            ));
        }
        if options.rows.as_ref().is_some_and(|r| r.start() > r.end())
            || options
                .columns
                .as_ref()
                .is_some_and(|r| r.start() > r.end())
        {
            return Err(invalid("Projection range is reversed"));
        }
        let sheet = self.sheets.iter().find(|s| s.name == name).ok_or_else(|| {
            Error::new(
                ErrorKind::SheetNotFound,
                format!("Worksheet not found: {name}"),
            )
        })?;
        if sheet.kind != SheetKind::Worksheet {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "Only cell worksheets support row streaming",
            ));
        }
        let part = sheet.part.clone();
        let style_allowance = allowance
            .map(|maximum| {
                maximum
                    .checked_sub(
                        self.catalog_memory_bytes()
                            .saturating_sub(self.style_memory_bytes()),
                    )
                    .ok_or_else(|| {
                        Error::new(
                            ErrorKind::MemoryBudgetExceeded,
                            "Package catalogs exceed aggregate allowance",
                        )
                    })
            })
            .transpose()?;
        self.prepare_styles_with_allowance(style_allowance)?;
        let fixed_bytes = self.catalog_memory_bytes();
        let pool = allowance
            .map(|maximum| {
                maximum
                    .checked_sub(fixed_bytes)
                    .map(|pool_bytes| crate::aggregate::ReadPool {
                        fixed_bytes,
                        pool_bytes,
                        retained_bytes: retained,
                    })
                    .ok_or_else(|| {
                        Error::new(
                            ErrorKind::MemoryBudgetExceeded,
                            "Prepared catalogs exceed aggregate allowance",
                        )
                    })
            })
            .transpose()?;
        if self.shared_strings.as_ref().is_some_and(|s| {
            (options.rich_text && !s.stats().rich_text_preserved)
                || share_values != s.shares_values()
        }) {
            // Rebuild when changing metadata preservation or text ownership.
            self.shared_strings = None;
        }
        if self.shared_strings.is_none()
            && let Some(string_part) = &self.shared_string_part
        {
            let file = self.archive.by_name(string_part).map_err(|e| {
                Error::caused_by(ErrorKind::Archive, "Cannot open shared-string part", e)
                    .with_part(string_part)
            })?;
            if file.size() > self.limits.max_part_bytes {
                return Err(limit("Shared-string part size limit exceeded").with_part(string_part));
            }
            let mut string_options = self.shared_string_options.clone();
            if let Some(pool) = &pool {
                let details = crate::memory_allowance(string_options.memory_policy, self.limits)?;
                let retained = details.retained_data_bytes.min(pool.available(0)?);
                string_options.memory_policy = crabxl_core::MemoryPolicy::Budget(
                    details
                        .working_reserve_bytes
                        .checked_add(retained)
                        .ok_or_else(|| invalid("Aggregate shared-string allowance overflows"))?,
                );
            }
            let strings = SharedStrings::parse(
                BufReader::with_capacity(self.limits.input_buffer_bytes, file),
                string_part.clone(),
                self.limits,
                &string_options,
                options.rich_text,
                share_values,
            )?;
            self.shared_strings = Some(strings);
        }
        if let (Some(pool), Some(strings)) = (&pool, &mut self.shared_strings) {
            strings.limit_or_spill(&self.shared_string_options, pool.available(0)?)?;
        }
        let file = self.archive.by_name(&part).map_err(|e| {
            Error::caused_by(ErrorKind::Archive, "Cannot open worksheet part", e)
                .with_part(part.clone())
        })?;
        if file.size() > self.limits.max_part_bytes {
            return Err(limit("Worksheet part size limit exceeded").with_part(part));
        }
        Rows::new(
            BufReader::with_capacity(self.limits.input_buffer_bytes, file),
            part,
            self.limits,
            options,
            self.shared_strings.as_mut(),
            self.imported_styles
                .as_ref()
                .map(|styles| crate::style_reader::StyleRead {
                    catalog: catalog.unwrap_or(&styles.catalog),
                    imported: styles,
                }),
            if self.date_1904 {
                crabxl_core::DateEpoch::Mac1904
            } else {
                crabxl_core::DateEpoch::Windows1900
            },
        )?
        .with_read_pool(pool, &self.shared_string_options)
    }
}
