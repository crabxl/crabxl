/// Guards against hostile input files, applied when reading and editing.
///
/// The defaults open every file Excel writes and stop ZIP bombs, deeply nested
/// XML and huge XML tokens. [`ResourceLimits::strict`] suits services that
/// read uploads from strangers; [`ResourceLimits::unlimited`] turns every
/// guard off for files you trust. Memory use is governed separately by
/// `MemoryPolicy`. XML document types are always rejected.
///
/// The hidden fields are internal tuning and test hooks.
#[derive(Clone, Copy, Debug)]
pub struct ResourceLimits {
    /// Largest ratio of uncompressed to compressed size for an archive entry
    /// that expands beyond 1 MiB. Defaults to 100, as in Apache POI.
    pub max_expansion_ratio: u64,
    /// Largest sum of the declared uncompressed entry sizes. Defaults to 16 GiB.
    pub max_total_uncompressed_bytes: u64,
    /// Largest uncompressed archive entry. Defaults to 4 GiB.
    pub max_part_bytes: u64,
    /// Largest number of archive entries. Defaults to 1,000,000.
    pub max_archive_entries: usize,
    /// Deepest XML nesting. Defaults to 256, as in libxml2.
    pub max_xml_depth: usize,
    /// Largest single XML text, tag or attribute run. Defaults to 10 MiB, as
    /// in libxml2.
    pub max_xml_event_bytes: usize,
    #[doc(hidden)]
    pub input_buffer_bytes: usize,
    #[doc(hidden)]
    pub max_materialized_bytes: usize,
    #[doc(hidden)]
    pub max_archive_bytes: u64,
    #[doc(hidden)]
    pub max_metadata_bytes: u64,
    #[doc(hidden)]
    pub max_cell_bytes: usize,
    #[doc(hidden)]
    pub max_theme_bytes: usize,
    #[doc(hidden)]
    pub max_style_bytes: usize,
    #[doc(hidden)]
    pub max_style_records: usize,
    #[doc(hidden)]
    pub max_formula_table_bytes: usize,
    #[doc(hidden)]
    pub max_shared_formulas: usize,
    #[doc(hidden)]
    pub max_sheets: usize,
    /// Excel's column count; a row cannot hold more cells.
    #[doc(hidden)]
    pub max_row_cells: usize,
    #[doc(hidden)]
    pub max_row_bytes: usize,
    /// Rows per owned batch; a tuning value, not a guard.
    #[doc(hidden)]
    pub max_batch_rows: usize,
    /// Estimated bytes per owned batch; a tuning value, not a guard.
    #[doc(hidden)]
    pub max_batch_bytes: usize,
}

const MIB: usize = 1024 * 1024;
const GIB: u64 = 1024 * 1024 * 1024;

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_expansion_ratio: 100,
            max_total_uncompressed_bytes: 16 * GIB,
            max_part_bytes: 4 * GIB,
            max_archive_entries: 1_000_000,
            max_xml_depth: 256,
            max_xml_event_bytes: 10 * MIB,
            ..Self::unlimited()
        }
    }
}

impl ResourceLimits {
    /// Tight guards and memory caps for untrusted uploads.
    pub fn strict() -> Self {
        Self {
            max_expansion_ratio: 100,
            max_total_uncompressed_bytes: GIB,
            max_part_bytes: 256 * MIB as u64,
            max_archive_entries: 10_000,
            max_xml_depth: 256,
            max_xml_event_bytes: MIB,
            input_buffer_bytes: 32 * 1024,
            max_materialized_bytes: 256 * MIB,
            max_archive_bytes: u64::MAX,
            max_metadata_bytes: 16 * MIB as u64,
            max_cell_bytes: 128 * 1024,
            max_theme_bytes: 16 * MIB,
            max_style_bytes: 16 * MIB,
            max_style_records: 100_000,
            max_formula_table_bytes: 16 * MIB,
            max_shared_formulas: 100_000,
            max_sheets: 1024,
            max_row_cells: 16_384,
            max_row_bytes: 4 * MIB,
            max_batch_rows: 1024,
            max_batch_bytes: 8 * MIB,
        }
    }

    /// No guards, for files you trust.
    pub fn unlimited() -> Self {
        Self {
            max_expansion_ratio: u64::MAX,
            max_total_uncompressed_bytes: u64::MAX,
            max_part_bytes: u64::MAX,
            max_archive_entries: usize::MAX,
            max_xml_depth: usize::MAX,
            max_xml_event_bytes: usize::MAX,
            input_buffer_bytes: 32 * 1024,
            max_materialized_bytes: usize::MAX,
            max_archive_bytes: u64::MAX,
            max_metadata_bytes: u64::MAX,
            max_cell_bytes: usize::MAX,
            max_theme_bytes: usize::MAX,
            max_style_bytes: usize::MAX,
            max_style_records: usize::MAX,
            max_formula_table_bytes: usize::MAX,
            max_shared_formulas: usize::MAX,
            max_sheets: usize::MAX,
            max_row_cells: 16_384,
            max_row_bytes: usize::MAX,
            max_batch_rows: 1024,
            max_batch_bytes: 8 * MIB,
        }
    }
}
