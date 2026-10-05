# ADR 0053: Caller-controlled concurrent Auto allowances

`AutoMemory.concurrent_operations` defaults to one and must be positive. Auto
divides its availability/headroom/fraction allowance by the anticipated number
of simultaneous managed operations, then applies the per-operation maximum.
Each operation still reserves its own parser/row space and charges actual
catalog/cache/template/retained-data allocations. Explicit Budget policies are
unchanged. The setting creates no worker threads and is not a global reservation.

Independent readers can scan independent sheets concurrently; they own separate
ZIP positions and SST/catalog/cache state. This avoids shared mutable archive
state but duplicates resources. A caller must use compatible operation and SST
policies when supplying custom availability/concurrency; existing explicit
component controls remain independently honored and clamped to the joint pool.

Existing deterministic allowance tests now cover one/two/four operations and
zero-count rejection. Linux Rust 1.88 workspace tests and latest Clippy pass.
[Two-sheet evidence](../../benchmarks/alpha5-concurrent-auto.md) checks repeated
and high-cardinality strings at small/large caller budgets, complete values,
CPU/wall/RSS, actual storage decisions, logical temporary costs and cleanup.
This completes the concurrency-setting checkpoint, not every platform private
limit or broader M7 performance comparison. No general parallel speed guarantee
or whole-process RSS cap is claimed.
