/// Intended access pattern, used to choose a useful allocation strategy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AccessPattern {
    /// One sequential pass; retaining every row provides no expected benefit.
    #[default]
    Scan,
    /// Repeated access; retain data when a bounded estimate fits the budget.
    RepeatedAccess,
}

/// Automatic budget controls. These tune an operation, not total process RSS.
#[derive(Clone, Copy, Debug)]
pub struct AutoMemory {
    /// Fraction of available memory after headroom, from 1 through 1000.
    pub fraction_per_mille: u16,
    /// Preferred headroom; never subtract more than half observed availability.
    pub headroom_bytes: u64,
    /// Optional upper bound on the automatically derived operation budget.
    pub maximum_bytes: Option<usize>,
    /// Effective availability supplied by the caller instead of platform probing.
    /// Include container/process constraints when providing this override.
    pub available_bytes: Option<u64>,
}
impl Default for AutoMemory {
    fn default() -> Self {
        Self {
            fraction_per_mille: 250,
            headroom_bytes: 256 * 1024 * 1024,
            maximum_bytes: None,
            available_bytes: None,
        }
    }
}

/// Shared managed-memory policy for reading and editable operations.
#[derive(Clone, Copy, Debug)]
pub enum MemoryPolicy {
    /// Derive an operation budget from effective availability and headroom.
    Auto(AutoMemory),
    /// Explicit operation budget, including a conservative parser working reserve.
    /// Retained data must fit the remainder; this is not a hard RSS limit.
    Budget(usize),
}
impl Default for MemoryPolicy {
    fn default() -> Self {
        Self::Auto(AutoMemory::default())
    }
}

/// A selected numeric read strategy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadMode {
    /// Rows are decoded on demand without retaining a whole sheet.
    Streaming,
    /// All sparse rows are owned by the returned sheet snapshot.
    Materialized,
}

/// Where effective available-memory information came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemorySource {
    /// The caller specified a fixed operation budget.
    ExplicitBudget,
    /// The caller supplied effective availability.
    CallerAvailability,
    /// Linux host, mounted cgroup v1/v2 visible hierarchies, and process limits were examined.
    Linux,
    /// Native Windows/macOS host availability; private process/job constraints
    /// are not discovered. Supply effective availability for constrained hosts.
    NativeHost,
    /// Constraint discovery was unavailable; a conservative fallback was used.
    ConservativeFallback,
}

/// The reason for the final automatic strategy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionReason {
    /// Sequential access benefits from streaming rather than retained data.
    SequentialAccess,
    /// A bounded sample predicts retained data will fit for repeated access.
    SampleFits,
    /// The estimated data size is larger than the retained-data allowance.
    EstimateExceedsBudget,
    /// Later rows exceeded the allowance; partial materialization was discarded.
    ActualDataExceedsBudget,
}

/// Observable diagnostics for an automatic read operation.
#[derive(Clone, Debug)]
pub struct ReadDecision {
    /// Final selected mode, including any materialization fallback.
    pub mode: ReadMode,
    /// Effective policy budget before subtracting the working reserve.
    pub budget_bytes: usize,
    /// Conservative component reserve for parser buffers and one current row.
    pub working_reserve_bytes: usize,
    /// Non-evictable managed package/style/theme/shared-string catalog storage.
    pub catalog_bytes: usize,
    /// Current optional decoded shared-string cache, sharing the row/template allowance.
    pub cache_bytes: usize,
    /// Initial row/vector allowance after prepared catalogs; later templates/cache growth also consume it.
    pub retained_data_bytes: usize,
    /// Sample-based estimate, absent for sequential scans.
    pub estimated_data_bytes: Option<usize>,
    /// Observed or caller-provided effective availability, when applicable.
    pub available_bytes: Option<u64>,
    /// Source of memory information.
    pub memory_source: MemorySource,
    /// Strategy selection/fallback reason.
    pub reason: DecisionReason,
}

/// Runtime-independent diagnostics for a managed operation allowance.
/// Existing catalogs, dependency allocations and caller data are additional;
/// this is a snapshot of availability, not a reservation or hard RSS ceiling.
#[derive(Clone, Debug)]
pub struct MemoryAllowance {
    /// Policy budget before working space is reserved.
    pub budget_bytes: usize,
    /// Conservative parser/row working reserve.
    pub working_reserve_bytes: usize,
    /// Bytes available for managed retained data after the reserve.
    pub retained_data_bytes: usize,
    /// Effective observed or caller-supplied availability.
    pub available_bytes: Option<u64>,
    /// Source of the availability/budget information.
    pub memory_source: MemorySource,
}
