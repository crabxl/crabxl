# ADR 0082: Charge retained model blocks and reserve growth work

Status: implemented and integrated into Python; native and Python A8 published
with platform and fresh public-package acceptance.

Canonical cells have used bounded contiguous blocks since ADR 0064, while the
managed ledger still charged 256 bytes for every cell. A dense numeric sheet
therefore exhausted its allowance far above its retained representation. Default
cardinality changes in ADR 0080 do not solve this independent byte estimate.

Track aggregate Vec capacity and charge it at the public Cell size, plus a
conservative 512-byte allowance for each indexed block. Charge names, metadata
and value heap payloads as before; shared text aliases remain conservatively
charged individually. This is a managed reservation, not a process RSS ceiling
or a promise about private standard-library node layouts or allocator overhead.

Control vector growth explicitly with bounded reserve_exact capacities. Cache
tail length/capacity for append planning, shrink materially underused vectors
only when old/new buffer overlap fits existing operation headroom, and release
capacity when its block is removed. Removal remains infallible when shrinking
must be skipped. Interior splitting reserves the final right
capacity directly instead of growing a temporary half block.

Set and row append validate both final retention and old/new buffer overlap
before changing cells. Structural operations keep separate conservative 256-byte
per-cell work reservations with a 1 KiB nonempty root allowance; this checkpoint
does not claim amortized optimal structural staging. No shadow cell/value model,
unsafe code or ownership-count heuristic is
introduced. Existing infallible Vec/tree allocations retain their earlier
process-allocation behavior; byte/count validation failures remain typed and
atomic. Caller-retained values and allocation overhead remain outside this ledger.

Extend existing sparse/resource workflows with a compact dense model bound,
buffer-growth failure checks for set and append, retained capacity release and
reuse. Replace obsolete fixed per-cell arithmetic in workbook tests with public
resource invariants and equivalent finite limits. The existing 4 MiB two-sheet
SST fixture can now load both sheets in RAM; count/aggregate rejection and disk
ownership/cleanup coverage remain in the same loaded-workbook workflow.

Acceptance requires full/MSRV/strict lint and both backend validation, plus paired
numeric, text/style, reverse population and Python evidence. Reject or revise the
implementation if accounting overhead materially degrades complete processing.

Local workspace tests, Rust 1.88 core/resource tests, strict Clippy, documentation
and native-zlib loaded/writer workflows pass. Existing function counts remain
unchanged; the comparison measurements run separately from these checks.

[The resource/performance report](../../benchmarks/alpha8-capacity-accounting.md)
verifies one million cells under 64 MiB versus the previous 262,143-cell cutoff,
and row insertion under 384 MiB. Large reading remains comparable after checked
helper inlining; per-cell creation has about 4-5% overhead. This is accepted as a
resource usability improvement, not a universal speed or physical RSS gain.
