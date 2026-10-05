# Cached default namespace scope checkpoint

The shared XML reader now avoids repeated namespace attribute decoding and
default-namespace classification on ordinary cell/value elements. Prefixes and
namespace declarations still use quick-xml's authoritative resolver. See
[ADR 0062](../docs/decisions/0062-cached-default-namespace-scope.md).

## Numeric operations

Preserved prior binary: core `02f7c1c`. Candidate: the same four-engine executable
with the private XML-route change, built with the same lockfile, Rust 1.99 and
release settings. One warmup and three rotating serial samples per variant/size.
The harness records binary/input hashes and independently verifies every saved
numeric coordinate/value outside timing. No builds/tests/profiles overlap timing.
Raw data: [numeric report](results/alpha7-namespace-numeric.json).

At 100,000 rows per sheet and ten columns, reads retain/scan two million cells.
Edit/save materializes both sheets, replaces the first A1 with 42 and saves.

| Operation | Before seconds | After seconds | Wall reduction | Before/after peak RSS KiB |
| --- | ---: | ---: | ---: | ---: |
| Complete numeric stream | 1.4101 | 1.1860 | 15.9% | 5,044 / 5,052 |
| Both editable models | 1.8752 | 1.7043 | 9.1% | 160,692 / 160,764 |
| Load, edit and save | 4.8440 | 4.3718 | 9.7% | 161,472 / 161,660 |

Edit output is 5,887,428 bytes in both variants; median sampled temporary/output
working bytes are 5,849,781 in both. Streams/models create no working files.
Smaller fixtures are in the raw report: at 1,000 rows model time is slightly
higher, so this is not a universal speedup for every operation/size. The calamine
read gap remains unresolved; the earlier reference result is contextual rather
than a newly paired reference measurement here.

## Text and styles

The companion harness reuses existing probes that verify every shared-string
value/order and every mixed numeric/date/duration/boolean/inline-text/error/
formula-cache value. It compares identical before/current probe source and
default-feature release builds separately from the unified four-engine binary.
RAM and disk SST modes distinguish memory from I/O costs. Descriptor sampling is
a 10ms lower bound and includes deleted disk-backed SST files; cleanup is checked.
Fixtures are generated outside timing. These ASCII text fixtures do not measure
all Unicode/rich-text workloads; namespace and richer format correctness remain
covered by workspace tests.

At 100,000 rows (one million cells), median wall seconds are:

| Workload | Before | After | Before/after RSS KiB | Sampled working bytes |
| --- | ---: | ---: | ---: | ---: |
| Repeated text, RAM SST | 0.8677 | 0.7410 | 1,944 / 2,016 | 0 |
| Repeated text, disk SST | 0.8219 | 0.7303 | 2,200 / 2,076 | 16,128 |
| Unique text, RAM SST | 1.5614 | 1.4021 | 142,616 / 142,684 | 0 |
| Unique text, disk SST | 2.2532 | 2.0462 | 2,664 / 2,588 | 126,000,000 |
| Mixed styled/date/formula cache | 0.8850 | 0.7666 | 1,984 / 1,984 | 0 |

Working bytes are unchanged between variants. RAM/RSS variations here are small;
the optimization does not claim a memory reduction. The unique disk SST's
126 MB remains an explicit disk/RAM tradeoff. All probes verify every value and
all temporary directories are empty after each operation. The 10,000-row results
and individual samples are in the [text/style report](results/alpha7-namespace-text-styles.json).

Full workspace tests, strict workspace/all-target Clippy and 91 streaming
integration tests under Rust 1.88 passed. Existing namespace checks include
Unicode/rich text and unrelated package codecs; those are correctness evidence,
not additional timed Unicode performance claims.

## Reproduction

```sh
python benchmarks/editable_engines.py --rows 1000 10000 100000 --runs 3 \
  --baseline-binary /path/to/preserved/editable-core-comparison \
  --modes crabxl-stream crabxl-model crabxl-edit \
  --output benchmarks/results/namespace-numeric.local.json
python benchmarks/xml_scope_checkpoint.py --before /path/to/prior/examples \
  --after /path/to/current/examples --rows 10000 100000 --runs 3 \
  --output benchmarks/results/namespace-text-styles.local.json
```

Build `shared_text` and `styled_read` with the same release toolchain/features in
separate targets, or clean the workspace packages between worktree builds before
preserving binaries. No performance thresholds are added to ordinary tests.
