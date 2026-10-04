# M2 structured formula checkpoint

Native and openpyxl 3.1.5 readers stream the same generated worksheet: two columns per row, one shared group with ID 4294967295 and one normal formula per row. Every expanded expression is checked. Native also checks every cached integer in the same timed pass; public reference cache verification is a separate untimed pass. One warmup precedes five rotating serial samples. Linux process wall/CPU and peak RSS include interpreter/runtime baseline. Builds and fixture generation are excluded; no heavy checks overlap sampling.

| Cells | Native seconds | Reference seconds | Native peak MiB | Reference peak MiB |
|---|---:|---:|---:|---:|
| 20,000 | 0.029741 | 0.358341 | 1.934 | 35.750 |
| 200,000 | 0.277448 | 2.063277 | 1.945 | 43.375 |

Shared template managed-storage estimate remains 355 bytes for both workloads, with one group and no unresolved followers. Owned temporary storage is zero and per-process TMPDIR remains empty. The estimate is not exact RSS; allocator overhead and process baseline are separate. Required reference speed and desired lower reference RSS hold for this workload. calamine formula/cache streaming overlap has not yet been validated, so no comparative win is claimed. rust_xlsxwriter has no read overlap; structured creation timings remain separate future evidence.

Fourteen generated public read cases pass, including actual anchors, sparse IDs, missing masters, reused definitions, source ranges, array/table object properties and cache-only behavior. The two nonfinite cases are explicitly deferred rather than counted as passing. Native creation is independently reopened through public ArrayFormula/DataTableFormula APIs, preserving optional flags, source range spelling, zero cache values and verbatim/empty normal expression bodies.

Reproduce with `python benchmarks/verify_formula_probe.py <release-formula-probe> --output benchmarks/results/m2-formula-public-interop.json` and `python benchmarks/shared_formula_checkpoint.py`. Build/run structured_formula_fixture and verify_structured_formula_fixture.py for creation evidence. Shared target paths honor CARGO_TARGET_DIR. Raw samples and case outputs are under benchmarks/results/. M2 remains in progress.
