// SPDX-License-Identifier: MIT
// Cell stream/value decoding adapted from calamine, Copyright 2016-2026 Johann Tuffe.
// Source provenance and changes: third_party/ports.json.

//! Rows operations.
use super::*;

impl<'a, R: Read + Seek> Rows<'a, R> {
    /// Fill a caller-owned sparse row buffer, retaining its allocation for reuse.
    ///
    /// Returns false at EOF. On error, clears partial cells and terminates the
    /// stream; a caller may start another reader from the workbook afterwards.
    pub fn read_row_into(&mut self, row: &mut Row) -> Result<bool> {
        row.cells.clear();
        self.row_payload_bytes = 0;
        if let Some(pending) = self.pending.take() {
            *row = pending;
            return Ok(true);
        }
        if self.exhausted {
            return Ok(false);
        }
        if self.options.stop_after_last_row
            && self
                .options
                .rows
                .as_ref()
                .is_some_and(|range| self.last_row.is_some_and(|last| last >= *range.end()))
        {
            self.exhausted = true;
            return Ok(false);
        }
        let result = self
            .read_row_impl(row)
            .map_err(|error| error.with_part(self.xml.part()));
        if result.is_err() {
            self.exhausted = true;
            row.cells.clear();
        }
        result
    }

    /// Read the next owned row, or None after the stream is exhausted.
    pub fn next_row(&mut self) -> Result<Option<Row>> {
        let mut row = Row::new(RowIndex::new(0)?);
        self.read_row_into(&mut row)
            .map(|present| present.then_some(row))
    }

    /// Read an owned batch bounded by row count and estimated allocation bytes.
    ///
    /// At most one additional bounded row is retained as lookahead if it does
    /// not fit this batch. A single row larger than the batch budget is an error.
    pub fn read_batch(&mut self) -> Result<Option<RowBatch>> {
        let previous = self
            .aggregate
            .as_ref()
            .map_or(0, |pool| pool.retained_bytes);
        let result = self.read_batch_impl();
        if let Some(pool) = &mut self.aggregate {
            pool.retained_bytes = previous;
        }
        result
    }
    pub(super) fn read_batch_impl(&mut self) -> Result<Option<RowBatch>> {
        let maximum = self
            .limits
            .max_batch_bytes
            .min(self.available_retained_bytes()?);
        let outer = maximum
            .checked_sub(size_of::<RowBatch>())
            .ok_or_else(|| self.limit("Batch byte limit is too small"))?;
        let row_slots = if self.aggregate.is_some() {
            size_of::<Row>().saturating_add(16 * size_of::<Cell>())
        } else {
            size_of::<Row>()
        };
        let capacity = self.limits.max_batch_rows.min(outer / row_slots);
        if capacity == 0 {
            return Err(self.limit("Batch byte limit is too small"));
        }
        self.set_aggregate_retained(
            size_of::<RowBatch>().saturating_add(capacity.saturating_mul(size_of::<Row>())),
        )?;
        let mut batch = RowBatch { rows: Vec::new() };
        batch.rows.try_reserve_exact(capacity).map_err(|e| {
            Error::caused_by(ErrorKind::LimitExceeded, "Cannot allocate batch", e)
                .with_part(self.xml.part())
        })?;
        let mut bytes = batch.memory_bytes();
        self.set_aggregate_retained(bytes)?;
        while batch.rows.len() < capacity {
            let Some(row) = self.next_row()? else {
                break;
            };
            let cell_bytes = row.memory_bytes() - size_of::<Row>();
            if bytes.saturating_add(cell_bytes) > maximum.min(self.available_retained_bytes()?) {
                if batch.rows.is_empty() {
                    self.exhausted = true;
                    return Err(self.limit("A row exceeds the batch byte limit"));
                }
                self.pending = Some(row);
                break;
            }
            bytes += cell_bytes;
            self.set_aggregate_retained(bytes)?;
            batch.rows.push(row);
        }
        Ok((!batch.rows.is_empty()).then_some(batch))
    }
}
