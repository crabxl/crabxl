# M2 typed rich-text checkpoint

Rust 1.88 release and public openpyxl 3.1.5 read-only rich_text=True. Ten columns contain two formatted runs per shared value: a 14-byte position-dependent prefix and 96-byte suffix. Every displayed value, run boundary and specified font/color override is verified. One warmup and five rotating serial measurements include startup, SST preparation and complete reading; generation/builds are excluded. [Raw evidence](results/m2-rich-text.json) records wall/CPU/RSS, exact input hashes and shared-string diagnostics. These are native comparisons, not Python adapter timings. These SST measurements precede the inline-only protection normalization correction found by subsequent creation interoperability; the SST lookup algorithm remains the same. Final creation/projection correctness is checked separately.

| One million cells | Native RAM seconds / MiB | Native Disk seconds / MiB | Native Auto seconds / MiB | openpyxl seconds / MiB |
|---|---:|---:|---:|---:|
| 128 repeated rich values | 0.944 / 1.76 | 0.961 / 1.90 | 0.919 / 1.76 | 4.135 / 41.60 |
| One million distinct rich values | 4.993 / 512.88 | 18.799 / 3.01 | 18.558 / 18.94 | 87.499 / 1488.06 |

The RAM policy supplies a 512 MiB component budget. Disk and Auto use 16 MiB explicit budgets, with a 1 MiB decoded-cache ceiling. Auto keeps the repeated table in RAM and spills the high-cardinality table. This measures budget-based adaptation; unrestricted host-availability adaptation has deterministic policy tests. Returned rows, dependencies and allocator costs are additional, so budget bytes are not a process RSS cap.

One million distinct disk-backed rich entries use **399,000,000 logical temporary bytes (380.52 MiB)** including the complete fixed-width index. Retained decoded cache is 1,048,424 managed bytes. The 128 repeated entries use 51,072 temporary bytes and 128 disk misses followed by 999,872 cache hits. Cache bypass for oversized individual values, forced-memory failure, temporary quota failure and resource/result ownership have dedicated tests. Filesystem allocation/page cache/tmpfs costs are additional; low process RSS does not eliminate disk or host-memory costs. Auto may retain allocator pages from its earlier RAM phase.

All measured rich cases are faster and have lower process RSS than the public reference. RAM spends substantially more memory to avoid disk encoding/decoding. These are fixture-specific results, not a universal performance claim or full M2 acceptance.

## Plain projection overlap

The same files are measured separately with formatting projected away, using native/default openpyxl and calamine 0.36.1 public Range. Calamine does not participate in the typed rich comparison. Native streams returned rows, openpyxl eagerly retains its SST, and calamine materializes its range; retention differences are explicit.

At one million distinct values, native RAM takes **4.004 seconds / 139.12 MiB**, calamine **1.712 seconds / 299.14 MiB**, and openpyxl **50.856 seconds / 277.48 MiB**. Desired calamine speed superiority remains unmet. Native disk takes 4.543 seconds / 2.39 MiB plus 126,000,000 temporary bytes for flattened text/index. Historical plain-input measurements remain in m2-shared-strings.md; richer input requires substantially more XML scanning and is not a plain-input regression comparison.

## Reproduction and boundaries

```sh
python benchmarks/rich_text_checkpoint.py --rows 10000 100000 --runs 5 --output benchmarks/rich-text.local.json
cargo run --release -p crabxl --example rich_fixture -- /tmp/rich-native.xlsx
python benchmarks/verify_rich_fixture.py /tmp/rich-native.xlsx
python benchmarks/probe_rich_text.py
```

Public creation readback verifies whitespace/CRLF/literal markers, empty styled runs, explicit true/false font fields and ARGB/tint. Separate OOXML assertions verify pronunciation metadata that the reference projects away. [Public marker observations](../docs/research/rich-text-public-probe.json) record differences across run boundaries; implementation source was not inspected.

Imported font/theme catalog resolution and phonetic-font assignment in existing packages remain staged, as do Python typed rich classes. See ADR 0010. This checkpoint does not complete M2, M4, or the full Rust baseline.

## Plain-input regression against the previous checkpoint

[Identical plain-SST input runs](results/m2-rich-plain-regression.json) compare a preserved dd38e85 native release binary against this checkpoint's final native binary. Both perform complete text verification in 512 MiB Memory mode; one warmup and five alternating repetitions include startup.

| Rows / cardinality | Previous seconds | Current seconds | Wall change |
|---|---:|---:|---:|
| 10000 / repeated | 0.0914 | 0.0892 | -2.4% |
| 10000 / unique | 0.1580 | 0.1622 | +2.7% |
| 100000 / repeated | 0.9192 | 0.8610 | -6.3% |
| 100000 / unique | 1.5316 | 1.6411 | +7.2% |

The large distinct plain-input path regresses by about 7.2%; it remains an explicit profiling/optimization task. No across-the-board speed improvement is claimed. The codec now also rejects forbidden XML characters and retains new typed modes; exact causation has not been isolated by profiling.
