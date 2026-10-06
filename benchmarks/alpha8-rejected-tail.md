# A8 rejected append-tail prototype

A bounded 128-cell append tail attempted to avoid one BTreeMap last-entry lookup
per monotonically loaded cell. Reads chained the tail with committed blocks;
random mutation flushed it into the existing sparse tree. The prototype passed
existing sparse and loaded workflows, but was removed after paired measurements.
The exact rejected diff and binary hashes remain in
[the raw report](results/alpha8-tail-models.json), not in production storage.

Both release workers share Rust 1.99, source, dependency features and reference
versions. The baseline is A7 core `ea04f692fffd8a52f838f44e9f7c36f48af55255`.
One warmup precedes three rotating serial samples, with complete counts/checksums,
all output coordinates, RSS, sampled temporary files and cleanup checked.
Generation, builds, tests and profiling do not overlap timed runs. Calamine
retains both returned ranges; its results do not measure editable workbooks.

| Source cells | Operation | A7 median seconds / RSS KiB | Rejected median seconds / RSS KiB |
| --- | --- | --- | --- |
| 200,000 | Retain both models | 0.108766 / 11,716 | 0.123361 / 11,532 |
| 2,000,000 | Retain both models | 1.072553 / 69,188 | 1.114261 / 69,132 |
| 2,000,000 | Load, edit, save | 3.456013 / 69,996 | 3.510818 / 69,884 |
| 1,000,000 written | Owned creation | 0.938390 / 37,348 | 0.973537 / 37,296 |

The small 100,000-cell creation sample improved, while larger read/edit/write
samples regressed or varied. RSS changes are negligible. There is no accepted
optimization or new release from this experiment. The measured complete-model
read gap versus calamine remains roughly twofold, with lower numeric RSS.
The existing sparse reference-map workflow now includes an incomplete final
block and verifies its row traversal without adding a new test function.

Reproduce the candidate by applying `candidate_patch` from the raw JSON to the
recorded baseline, building both workers, then running:

```sh
python benchmarks/editable_engines.py --baseline-binary /path/to/a7-worker \
  --rows 10000 100000 --runs 3 \
  --modes crabxl-model calamine-model crabxl-edit write-crabxl-model \
  --output /tmp/tail-results.json
```
