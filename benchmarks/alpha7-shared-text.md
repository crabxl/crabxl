# Mode-aware retained shared text

Retained canonical models now share plain SST payloads, while ordinary row
streams retain owned storage. See [ADR 0065](../docs/decisions/0065-mode-aware-shared-text.md).
This is a RAM improvement with measured tradeoffs, not calamine speed acceptance.

## Method

One warmup and three rotating serial release samples per size/mode. No builds,
tests or profiling overlap timing. Both native model binaries use the same
worker source, lockfile, Rust 1.99 and unified dependency features. Prior model
core is `f5e03ca`; row-stream baseline is the namespace-optimized reader whose
text codec was unchanged by subsequent cell packing. Binary/input hashes,
wall/CPU, kernel peak RSS and sampled temporary bytes are in the raw reports.
All workers verify every value and relevant coordinates/styles. Calamine retains
all returned ranges; CrabXL retains editable models and source catalogs.

## One million retained cells

Median wall seconds / peak RSS KiB:

| Workload | Prior packed model | Shared model | Calamine retained range |
| --- | --- | --- | --- |
| Repeated 110-byte text, RAM SST | 0.8920 / 193,976 | 0.8380 / 68,756 | 0.4222 / 160,676 |
| Unique 110-byte text, RAM SST | 1.5019 / 334,392 | 1.4787 / 224,916 | 0.7171 / 309,116 |
| Repeated text, disk SST | 0.9079 / 194,232 | 0.8263 / 68,756 | No matched disk policy |
| Unique text, disk SST | 2.0602 / 194,488 | 2.1377 / 209,300 | No matched disk policy |
| Mixed styles/scalars/inline text/formula caches | 0.8175 / 68,408 | 0.8417 / 68,372 | Different typed/date contract |

RAM-SST repeated models use about 65% less RSS than the prior model and 57%
less than calamine; unique models use about 33% less than prior and 27% less
than calamine. Both are still approximately twice as slow as calamine.
Unique disk models allocate reference-count headers for decoded retained
payloads, increasing RSS by about 8% and wall time by about 4%. This is an open
tradeoff, not a universal optimization. Mixed-style time rises about 3% in this
sample set; there is no claimed style speed gain.

Disk SST working-file peaks remain 16,128 bytes for repeated strings and
126,000,000 bytes for unique strings. Cleanup is verified. Core uses 1 GiB
joint/model allowances, 256 MiB SST allowance and 1 MiB disk cache; these
managed allowances are not an RSS cap. Conservative alias charges remain.

## Ordinary row streams

At one million cells, owned RAM-SST unique text measures 1.3688 to 1.3747
seconds and 142,684 to 142,624 KiB: the blanket-sharing RAM increase is avoided.
Repeated RAM text measures 0.7540 to 0.7040 seconds at roughly 2 MiB RSS.
Unique disk text measures 1.8575 to 1.9558 seconds at roughly 2.5 MiB RSS;
mixed styles measure 0.7263 to 0.7435 seconds. These samples do not establish
universal streaming speed improvement. Input/probe differences prevent treating
these timers as the retained calamine operation.

Raw evidence includes both 100,000 and one million cells:
[retained models](results/alpha7-shared-text-models.json) and
[row streams](results/alpha7-shared-text-streams.json).

The calamine load-speed gap remains an explicit optimization target. Python
conversion, bounded dataframe delivery and the remaining M4/M6 feature gates
require separate implementation and measurements.
