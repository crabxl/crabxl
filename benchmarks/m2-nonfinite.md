# M2 nonfinite compatibility measurements

The sequential creation workload has five columns: finite floating point, positive infinity, negative infinity, NaN and a normal formula with a blank serialized cache. Native receives an infinity cache; public openpyxl has no cache-assignment API, so it receives a normal formula with absent cache. Both output blank numeric values and reopen as None in data_only mode. This is equivalent public output, not equivalent cache-input APIs. Every cell is reopened and checked in both formula and cache-only modes outside timing.

One warmup precedes five rotating serial Linux process samples. Generation/builds and other heavy verification do not overlap samples. RSS includes process/runtime baseline. Separate owned TMPDIR sampling every 25ms is a lower bound; exact native logical spool accounting includes the closed worksheet footer. The final ZIP is excluded from temporary storage. No residual owned files remain.

| Cells | Native seconds | Reference seconds | Native peak MiB | Reference peak MiB | Native spool MiB | Reference XML MiB |
|---|---:|---:|---:|---:|---:|---:|
| 50,000 | 0.025546 | 0.382073 | 2.141 | 33.938 | 1.527 | 1.756 |
| 500,000 | 0.288958 | 2.284634 | 2.109 | 33.934 | 16.033 | 18.321 |

Required reference speed and desired reference RSS targets hold for this workload. Native temporary use grows with serialized cells; it is not a RAM-only result. calamine has no creation overlap, and rust_xlsxwriter nonfinite policy/API overlap has not yet been validated; no comparative win is claimed.

The finite shared-formula regression rotates the preserved exact a710b6d binary, the current native binary and the pinned public reader. At 20,000 cells, median native wall changes 0.029601 to 0.028844 seconds (-2.6%); at 200,000, 0.281879 to 0.279317 (-0.9%). These small differences are observations rather than a promised optimization. Output counts and every expression/cache remain checked, one bounded template remains 355 accounted bytes, and no temporary read storage is used. Raw before/after samples and baseline binary SHA-256 are recorded with ADR 0015.

Reproduce using benchmarks/probe_nonfinite.py, verify_formula_probe.py, nonfinite_checkpoint.py and shared_formula_checkpoint.py --baseline <preserved-binary> --output <regression-json>. All 16 generated public formula cases pass; former overflow deferrals have been removed. Strict rejection and malformed tokens have separate deterministic tests. M2 remains in progress.
