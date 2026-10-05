# Native four-engine numeric baseline before parser optimization

The user selected umya-spreadsheet as the primary remaining feature-port source
and requested immediate speed/RAM optimization. This comparison establishes
specific costs rather than treating feature breadth as performance evidence.
See [source assessment](../docs/research/umya-editable-core-assessment.md).

## Method and boundaries

CrabXL core `02f7c1c`, umya-spreadsheet 3.1.0, calamine 0.36.1 and
rust_xlsxwriter 0.99.1 run in one native release executable on Linux/Rust 1.99.
This deliberately unifies compatible ZIP/flate2 dependency features, including
the zlib-rs backend; it is not a comparison of four independent default builds.
Inspect the committed independent Cargo lockfile for transitive versions.
Neither Python conversion nor the user's NYC 1M-by-41 workbook is measured.

One warmup and three rotating serial samples per mode/scale. Kernel wait4 wall,
CPU and RSS include startup, operation and destruction. Builds, generation,
profiling and independent output verification do not overlap timing. Every
output numeric coordinate/value, count and checksum is verified through bounded
lxml iteration. Original unchanged numeric inputs have two sheets with ten
columns; creation writes one sheet. Input and binary hashes and every sample are
in [raw results](results/alpha7-four-engines.json).

Streams scan all values. CrabXL models retain both editable sheets; umya's normal
and lazy-model routes also retain both. Calamine Range loads and releases one
noneditable sheet at a time, so its RSS is not the cost of retaining both editable
sheets. Initialization-only lazy modes never scan values and are reported
separately in the raw data. Full-model edit routes load/sum both sheets, replace
the first A1 with 42 and save. No general feature/preservation equivalence is
claimed from this numeric overlap.

Working-file sampling observes actual process descriptors under an isolated
TMPDIR/output directory every 10ms, including anonymous/deleted spools and
adjacent/final output ZIPs. It is a lower bound, not an exact peak. Final output
bytes and successful cleanup are separate assertions. At one million written
cells, median sampled working bytes are approximately 35.8 MB for both sequential
spool writers and CrabXL owned export, versus 3.0 MB for the reference owned
writers. Sequential routes trade resident cell storage for temporary XML/I/O.

## Numeric results

Median wall seconds / kernel peak RSS KiB. Reads use two million numeric cells
(100,000 rows per sheet); creation uses one million (100,000 rows).

| Operation | CrabXL | Reference |
| --- | --- | --- |
| Read complete stream | 1.3768 / 4,932 | calamine cells 0.4296 / 4,612; umya callback 1.5078 / 70,548 |
| Read editable models | 1.7957 / 160,644 | umya normal 4.8268 / 597,340 |
| Read noneditable Range, one sheet at a time | Not the editable model workload | calamine 0.5100 / 74,716 |
| Sequential creation | 0.9007 / 5,200 | rust_xlsxwriter constant-memory 0.9826 / 5,740 |
| Owned creation | 1.1799 / 82,960 | rust_xlsxwriter normal 0.9325 / 110,924; umya 3.4404 / 344,660 |

The measured stream-read gap to calamine remains about 3.2 times. Owned creation
is roughly 27% slower than the matched rust_xlsxwriter numeric creation, while
using lower RSS here. Sequential creation is competitive in this unified-backend
workload. CrabXL's editable numeric model is faster and uses lower RSS than umya
on these fixtures, despite its much narrower current feature coverage. These
results do not establish general superiority or disprove other reported gaps.
The smaller scales and full-model edit/lazy modes remain in the raw report.

## Profile and next work

Separate gperftools CPU sampling, excluded from timings, puts the XML event
route in about 77% of stream samples and compression in about 86% of sequential
write samples. Address repeated namespace/attribute processing, numeric decoding,
model allocation and binding conversion; measure compression backend/level
tradeoffs and output sizes before changing defaults. Recheck numeric, text,
sparse, styled/formula and multisheet routes as each implementation changes.

The umya-created new numeric workbook exposes a separate interoperability gap:
its base-style table is absent while cell XFs reference zero, which current
CrabXL rejects. Independent numeric XML verification does not resolve or hide
that style-policy incompatibility.

## Reproduction

```sh
cargo build --release --locked --manifest-path benchmarks/umya/Cargo.toml
cargo build --release --locked -p crabxl --example workbook_demo
cc -O2 -Wall -Wextra -Werror benchmarks/measure.c -o benchmarks/measure
python benchmarks/editable_engines.py --rows 1000 10000 100000 --runs 3 \
  --output benchmarks/results/four-engines.local.json
```

The Python harness needs lxml. Run on an otherwise idle Linux host; rebuilding
after code changes measures the new revision rather than reproducing this binary.
