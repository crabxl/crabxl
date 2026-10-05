# Packed canonical models and all-sheet-retained calamine comparison

The canonical sparse model now stores cells in bounded contiguous blocks instead
of one ordered-map entry per cell. No second model, worksheet clone or unsafe
code is introduced; see [ADR 0064](../docs/decisions/0064-packed-sparse-cell-storage.md).

## Method

Prior core `4279dcb` and the packed candidate use identical native worker source,
release Rust 1.99, lockfile and dependency features. Numeric and text/style runs
record their separate binary hashes because the worker gained text/style probes
between measurement batches. Both variants in each batch use the same probe.
The shared executable unifies ZIP/flate2 features, including zlib-rs; independent
default-package builds are not claimed.

One warmup and three rotating serial samples per size/mode. Kernel wait4 wall,
CPU and RSS include operation/destruction; builds, generation, tests, profiling
and independent numeric output verification do not overlap timings. Numeric
edit/create outputs are checked at every coordinate outside timing. Mixed/text
workers verify every value and applicable style/coordinate inside timing. These
are different workloads and their absolute timers should not be combined.
Actual temporary/output descriptors are sampled every 10ms; this is a lower
bound. Cleanup, output sizes and input/binary hashes are separate evidence.

The added `calamine-model` explicitly retains all returned ranges until complete
traversal finishes. Both engines now retain both numeric sheets. Its returned
Range remains noneditable; CrabXL additionally owns its source package and
editable models. The earlier one-sheet-at-a-time Range measurement remains a
distinct mode, not an estimate of all-sheet retention.

## Numeric models and export

At 100,000 rows per sheet/ten columns, model operations read two million cells;
owned creation writes one million. Median wall seconds / peak RSS KiB:

| Operation | Prior CrabXL | Packed CrabXL | Calamine retaining both ranges |
| --- | --- | --- | --- |
| Retain both full models | 1.6967 / 160,684 | 1.3180 / 69,120 | 0.5230 / 106,128 |
| Load, edit A1, save | 4.5305 / 161,576 | 4.2254 / 69,616 | Not editable |
| Owned numeric creation | 1.2154 / 82,936 | 1.0009 / 37,092 | No writer |

Packed full models reduce measured RSS by about 57% and wall time by 22% against
the previous model. RSS is about 35% below the retained calamine workload here.
**Load speed is still about 2.5 times slower than calamine and remains unmet.**
Reference sample variation is visible: the identical reference in the prior
executable measured 0.5655 seconds / 106,100 KiB. The optimization does not change
reference code or claim reference speed improvement.

Owned creation reduces wall time by about 18% and RSS by 55%; edit/save wall time
drops about 7%. Numeric edit output remains 5,887,428 bytes and owned creation
2,942,935 bytes in both variants. Sampled edit working bytes vary around the
output size; owned export still uses approximately 35.8 MB of XML/ZIP working
files. RAM reduction does not remove this disk tradeoff.

All sizes/samples: [numeric report](results/alpha7-packed-models.json).

## Text, SST placement and mixed styles

One million cells (100,000 rows/ten columns), median seconds / RSS KiB:

| Workload | Prior CrabXL | Packed CrabXL | Calamine retained text Range |
| --- | --- | --- | --- |
| Repeated 110-byte text, RAM SST | 1.0977 / 239,016 | 0.9359 / 193,800 | 0.4141 / 160,464 |
| Unique 110-byte text, RAM SST | 1.9015 / 379,560 | 1.6266 / 334,408 | 0.7422 / 309,044 |
| Repeated text, disk SST | 1.1998 / 239,168 | 0.9003 / 194,184 | No matched disk policy |
| Unique text, disk SST | 2.5223 / 239,656 | 2.2208 / 194,504 | No matched disk policy |
| Mixed styles/dates/durations/booleans/errors/inline text/formula caches | 1.0003 / 114,088 | 0.8102 / 68,488 | Different typed/date contract; not compared |

SST working-file peaks remain 16,128 bytes for repeated disk strings and
126,000,000 bytes for unique disk strings; all are removed after each operation.
Both core model candidates use a 1 GiB joint/model allowance, 256 MiB SST allowance
and 1 MiB disk cache; calamine uses its defaults. These limits are explicit and
are not measured RSS caps. The conservative core 256-byte per-cell ledger remains
unchanged even though physical storage is more compact.

Packing improves these workloads, but **RAM-SST text models still use more RSS
than calamine and are slower**. Remaining duplicated decoded SST/text ownership
needs separate optimization; do not extend the numeric RAM win to all inputs.
The fixtures use ASCII strings, not the unavailable NYC file or a complete rich
text/Unicode corpus. Raw data: [text/style report](results/alpha7-packed-text-styles.json).

## Correctness and reproduction

Workspace tests and strict workspace/benchmark Clippy passed. The existing sparse
workflow covers multiple blocks, reversed interior insertion, key removals,
row traversal and complete removal/reuse against an independent ordered map.
Its five worksheet integration tests also pass on Rust 1.88. Public Cell/style/
formula semantics and failed-mutation budgets retain their existing checks.

```sh
python benchmarks/editable_engines.py --baseline-binary /path/to/prior/worker \
  --rows 1000 10000 100000 --runs 3 \
  --modes crabxl-model calamine-model crabxl-edit write-crabxl-model \
  --output benchmarks/results/packed-models.local.json
python benchmarks/packed_model_workloads.py --before /path/to/prior/worker \
  --rows 10000 100000 --runs 3 \
  --output benchmarks/results/packed-text-styles.local.json
```

Build prior/current workers with identical source/features in separate targets.
Generate the text/style fixtures with xml_scope_checkpoint before these runs.
Python conversion, streaming speed and all M4/M6 capabilities remain separate
acceptance work. This checkpoint is not a new Alpha release.
