# Alpha.5 formula annotation evidence

Linux x86_64 Rust 1.99 release builds. One warmup and three rotating serial
samples include archive opening and complete XML/CRC traversal. Generation,
compilation and tests are excluded. Every reader verifies formula expression,
range and cached integer values; retention additionally checks every cm value.
No worksheet is materialized and no temporary store is created.

| Rows | Mode | Median seconds | Peak RSS KiB |
| ---: | --- | ---: | ---: |
| 10,000 | Prior ordinary projection | 0.026270 | 2,024 |
| 10,000 | Current ordinary projection | 0.026792 | 2,056 |
| 10,000 | Current explicit retention | 0.027581 | 2,004 |
| 100,000 | Prior ordinary projection | 0.243590 | 2,008 |
| 100,000 | Current ordinary projection | 0.238316 | 2,056 |
| 100,000 | Current explicit retention | 0.247331 | 1,956 |

Prior core is ca40f674df0f97939821182c1e1f8c137b5ed3ae. Ordinary readers perform
the same two-cell visible projection per row. Their observed time differences
are +2.0% and -2.2%; these are sample variation/regression evidence, not a claimed
optimization. Explicit retention selects column A only, owning its cm reference;
it does different work and is not an equivalent-mode speed comparison. Fixed
streaming settings retain one row and avoid size-dependent metadata maps.

FormulaMetadata adds eight fixed bytes on 64-bit hosts. Explicit annotations add
an owned wrapper plus bounded reference payload; scalar cells do not grow.
Peak RSS includes runtime/dependency/allocator effects, not only that ledger.
Source and executable hashes, CPU/wall/RSS samples are in
[raw evidence](results/alpha5-formula-annotations.json).

```sh
python benchmarks/alpha5_formula_annotations.py --prior /path/to/prior-reader \
  --current target/release/examples/dynamic_formula_read \
  --retained target/release/examples/annotated_formula_read \
  --measure /path/to/measure --output /tmp/annotations.json
```
