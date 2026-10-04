# Canonical number-format classification checkpoint

Both native engines prepare complete style catalogs and stream all 100,000 cache-only styled values. Public openpyxl 3.1.5 read-only verifies the same floats, dates, clocks, elapsed durations, booleans, text, errors, formula caches and integers. Additional declared formats are intentionally unused by cells, exposing catalog preparation cost. One warmup and five rotating serial cold-process samples; no concurrent builds/checks or temporary storage.

| Declared formats | Current seconds / RSS KiB | Previous seconds / RSS KiB | openpyxl seconds / RSS KiB |
| --- | --- | --- | --- |
| 1,001 | 0.093432 / 2,052 | 0.090831 / 2,120 | 0.577677 / 34,800 |
| 50,001 | 0.125791 / 4,716 | 0.121895 / 5,064 | 0.830106 / 89,128 |

The large case removes the former 50,001-entry classification scratch vector and lowers median process RSS by 348 KiB. Cached classification fits into existing record padding on this target; record layout is not a public ABI promise. Native wall is approximately 3.2% slower in this sample; no speed improvement against the prior engine is claimed. Required public-reference speed and desired public-reference RSS hold for these workloads. No calamine comparison, full style API parity or milestone completion is claimed.

Previous core: 541d8dfe83704ded24ceb3a81aa982ea254fb6c7. Preserved styled_read binary SHA-256: 84eb509ef4deaf318abdafca516229482fcb21b274b8ac526d8699cd4bf79dea. Reproduction: style_classification_checkpoint.py; raw results: results/m2-style-classification.json. All returned values are checked, not just cell counts. Dependency/allocator costs remain additional to the managed budget.
