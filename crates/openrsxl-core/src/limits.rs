/// Limits for archive metadata, XML events, rows, and owned batches.
///
/// ZIP central-directory allocation happens inside the ZIP dependency before
/// entry-count checks; the compressed archive-size limit is checked first.
#[derive(Clone, Copy, Debug)]
pub struct ResourceLimits {
    /// Worksheet decompression/XML input buffer size; independent of batch size.
    /// Larger buffers do not guarantee faster CPU-bound parsing.
    pub input_buffer_bytes: usize,
    /// Maximum estimated retained allocation for explicit sheet materialization.
    /// This is a data budget, not a hard process RSS cap.
    pub max_materialized_bytes: usize,
    /// Maximum compressed archive size, checked before ZIP parsing.
    pub max_archive_bytes: u64,
    /// Maximum number of archive entries.
    pub max_archive_entries: usize,
    /// Maximum sum of declared uncompressed archive entry sizes.
    pub max_total_uncompressed_bytes: u64,
    /// Maximum uncompressed worksheet size.
    pub max_part_bytes: u64,
    /// Combined uncompressed metadata budget.
    pub max_metadata_bytes: u64,
    /// Maximum bytes consumed while parsing a single XML event.
    pub max_xml_event_bytes: usize,
    /// Maximum bytes in one decoded cell value.
    pub max_cell_bytes: usize,
    /// Maximum XML nesting depth.
    pub max_xml_depth: usize,
    /// Maximum worksheet catalog entries.
    pub max_sheets: usize,
    /// Maximum cells in a returned sparse row.
    pub max_row_cells: usize,
    /// Maximum estimated allocation for a returned row.
    pub max_row_bytes: usize,
    /// Maximum rows in an owned batch.
    pub max_batch_rows: usize,
    /// Maximum estimated allocation for an owned batch.
    pub max_batch_bytes: usize,
}
impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            input_buffer_bytes: 32 * 1024,
            max_materialized_bytes: 256 * 1024 * 1024,
            max_archive_bytes: 512 * 1024 * 1024,
            max_archive_entries: 16_384,
            max_total_uncompressed_bytes: 4 * 1024 * 1024 * 1024,
            max_part_bytes: 2 * 1024 * 1024 * 1024,
            max_metadata_bytes: 16 * 1024 * 1024,
            max_xml_event_bytes: 64 * 1024,
            max_cell_bytes: 64 * 1024,
            max_xml_depth: 64,
            max_sheets: 1024,
            max_row_cells: 16_384,
            max_row_bytes: 1024 * 1024,
            max_batch_rows: 1024,
            max_batch_bytes: 8 * 1024 * 1024,
        }
    }
}
