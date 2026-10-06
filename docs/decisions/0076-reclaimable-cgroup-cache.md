# ADR 0076: Conservative reclaimable cgroup file-cache availability

Status: implemented; A8 integration and release acceptance remain open.

Linux Auto availability previously used only `memory.max - memory.current` (or
legacy equivalents) at every visible hierarchy level. Compilation and large
file reads can fill the container's file cache while leaving reclaimable pages,
causing tiny workbook initialization to fail its configured working reserve.
The A7 release-wheel verification observed this under concurrent compilation;
the isolated rerun passed all 547 tests. Explicit budgets were unaffected.

Availability now subtracts only inactive file cache minus dirty and writeback
pages from current usage. It uses v2 `inactive_file`, `file_dirty`,
`file_writeback` and v1 hierarchical `total_*` counters. Active cache, shared
memory and anonymous pages do not count as reclaimable. Missing, malformed,
duplicate or incomplete fields fall back to zero reclaimable bytes. Saturating
arithmetic clamps the estimate to observed usage and the finite controller cap.
The most constrained visible parent, host MemAvailable, process limits, policy
headroom, fraction, concurrent-operation divisor and maximum remain applicable.

This remains a best-effort availability estimate across separately read kernel
snapshots, not a guaranteed allocation or hard RSS limit. Hidden ancestors,
active cache reclaimability and non-Linux host policies retain their documented
limitations. Caller availability and finite Budget policies bypass this probe
as before; no configured finite cap is increased or disabled.

Existing cgroup v1/v2 hierarchy tests cover clean versus dirty/writeback cache,
shared-memory exclusion, malformed/missing fields and snapshot saturation.
They remain seven resource test functions. Rust 1.88 and strict Clippy pass.
[Functional observations](../../benchmarks/alpha8-reclaimable-memory.md) compare
the published A7 dependency with this candidate and verify a fresh registry
workbook. No speed or physical-memory reduction is claimed.
