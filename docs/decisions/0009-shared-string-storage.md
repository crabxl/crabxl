# ADR 0009: Adaptive shared-string storage

Status: accepted for the initial plain shared-text checkpoint; [ADR 0010](0010-rich-text-projections.md) extends storage to typed rich/phonetic values.

Keep shared strings in the XLSX codec, not the runtime-independent core or a Python engine. Resolve the SST through workbook relationships. Prepare once at first row access by incremental XML parsing through EOF/CRC. Do not allocate from uniqueCount, load a whole worksheet/XML buffer, or treat the whole SST as small metadata.

`SharedStringOptions` selects Memory, Disk or Auto placement and a shared MemoryPolicy. The policy reserves parser working space before calculating retained table/cache allowance; separate metadata, caller rows, allocator/dependency costs and OS page cache remain additional. Component budgets are not a hard global RSS limit. Auto spends RAM on a complete table when actual capacities and payloads fit; otherwise it moves existing entries to disk and continues incrementally. Forced Memory returns a typed budget error rather than silently changing mode.

The disk store owns separate payload and fixed-width 16-byte index files. Every ID has an index record, including a sentinel for unsupported entries. Neither the full index nor the payloads are retained in RAM. Buffered writes are flushed before installation. Lookup uses checked actual IDs, independent seeks and a decoded direct-slot cache with capacity/payload byte accounting. Collision replacement and byte-limit eviction are bounded; this is not an unbounded LRU. Owned returned text remains independent of cache lifetime.

A parse/write/CRC/budget error drops the unpublished table and temporary files. Row parsing may be retried against the original archive. Workbook Drop and option reconfiguration release the store; reconfiguration causes the next row access to rebuild. Table preparation currently scans the entire SST even when the requested projection has no shared strings. Deferred selected-ID preparation and cache tuning are future optimizations, not completed acceptance claims.

Literal semantics are format-specific. The pinned public reference leaves inline escape-looking literals unchanged but removes `x005F_` from shared strings. Preserve these distinct observable behaviors instead of importing a general calamine escape decoder. XML text/entity accumulation is shared across codecs. ADR 0010 extends the entry representation and storage encoding without introducing another workbook model.

Tests and measurements are linked in milestones.md and benchmarks/m2-shared-strings.md. All M2 acceptance remains required; this storage checkpoint does not complete M2.
