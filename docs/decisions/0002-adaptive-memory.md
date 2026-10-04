# ADR 0002: Adaptive allocation must buy useful throughput

Status: required design; automatic allocation is not implemented yet.

The library must offer intelligent Auto, a caller-specified managed-memory budget, and advanced buffer/batch/cache/concurrency settings. Optimize useful throughput within that budget, rather than minimizing RSS or filling all available RAM. A 32 GiB host should be able to trade more memory for measured faster strategies. A large allocation with no measurable benefit is not a speed strategy.

Auto must determine effective availability from host available memory, container/cgroup and applicable process limits, keep explicit headroom, and respect a caller budget first. Availability is different from installed RAM and can change. Provide a conservative documented fallback on unsupported platforms; expose the chosen budget, mode, parameters, and decision reasons so callers can diagnose or override decisions. No platform probing belongs in the runtime-independent core models.

Mode selection needs an access-pattern hint: sequential scan, repeated lookup, or editable workbook. A caller explicitly selecting streaming or materialization must retain that semantic choice. Auto must not silently materialize an arbitrary huge sheet just because dimensions claim it is small; use bounded sampling/validated input estimates. Current `read_sheet` is a numeric snapshot and does not complete the future editable model.

Measure separate allocation strategies: useful input buffering, bounded string/style caches, compact row/batch storage, reusable indexes, and parallel work. Independent sheets are an initial parallelism candidate. Parallel parsing within a compressed worksheet requires a validated decompression/chunk boundary design; it cannot assume arbitrary ZIP seeks yield independently decodable XML. If indexing/spooling uses disk, measure temporary space, I/O, cleanup, and total latency.

The current numeric buffer experiment found approximately seven seconds with 32 KiB through 8 MiB input buffers, with CPU time close to wall time. Larger buffering alone gives little evidence of a meaningful speedup on this host. This does not prove that all memory-rich strategies lack benefit: profile decompression, namespace/XML handling, coordinate/attribute parsing, allocations, and value conversion before selecting the next optimization. Preserve validation and all required feature semantics in faster paths.

Budget accounting must distinguish managed allocations, working memory, runtime/dependency overhead, and caller-retained data. The existing ZIP catalog allocates before entry-count validation, so this checkpoint must not claim a hard global memory limit. Define/enforce accounting for new caches and materialization, reserve working headroom, shrink/evict safely when possible, and expose budget failures as typed errors. Never rely on process out-of-memory termination as the budget mechanism.

Acceptance: repeated measured runs across scan/repeated-access and numeric/text/styled/multi-sheet workloads, small and large budgets, host/container constraints, and explicit overrides. Report wall time, CPU time, peak RSS, managed allocations, temporary storage, and correctness. Auto should choose measured useful strategies, remain predictable under constraints, and avoid materialization when it provides no workload benefit.
