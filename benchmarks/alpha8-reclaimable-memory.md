# A8 functional cgroup cache availability observations

[ADR 0076](../docs/decisions/0076-reclaimable-cgroup-cache.md) corrects Auto's Linux
availability estimate without changing explicit budgets or retained data.
The probe links published crates.io `crabxl =0.1.0-alpha.7` and the local
candidate in one process. Three serial observations run after compilation/tests
finish; no profiling overlaps. This is functional evidence, not a timing/RSS
benchmark. Kernel snapshots can change between calls.

| Observation | A7 available bytes / budget bytes | Candidate available bytes / budget bytes |
| --- | --- | --- |
| 1 | 318,607,360 / 39,825,920 | 918,769,664 / 162,583,552 |
| 2 | 318,472,192 / 39,809,024 | 918,634,496 / 162,549,760 |
| 3 | 318,451,712 / 39,806,464 | 918,614,016 / 162,544,640 |

All three calls succeed at this observation point. Earlier A7 Python creation
failures occurred during compilation with substantially less raw headroom;
these observations do not reproduce or claim that exact earlier snapshot.
The difference credits clean inactive file cache, conservatively excluding all
dirty/writeback pages. Existing deterministic hierarchy tests cover pressured,
malformed and capped scenarios instead of modifying system cgroup files.

Each probe also creates a worksheet and verifies an integer through the newly
installed public A7 crate, with no local path substituted for the baseline.
Raw before/after cgroup usage/statistics, exact probe source/manifest, package
source, executable hash and results are in
[the report](results/alpha8-reclaimable-memory.json). The probe adds no workbook
copy, disk spool or memory allocation strategy; physical RSS and temporary-file
cost are not claimed to improve.
