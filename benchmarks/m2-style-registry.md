# Canonical style registration checkpoint

The writer now shares component records through the canonical core registry. This checkpoint does not close loaded-bank editing, themes, differential styles or aggregate Auto budgets.

## Equivalent public output

One warmup and five rotating serial Linux samples compare sequential creation with openpyxl 3.1.5 and exact prior core revision `3dfda8a5b7d3fb376d781cc74cad84867a34e4a5`. Each row contains a floating value and a distinct font/fill/rotation combination. Both native versions additionally register all styles a second time to verify stable IDs; the reference shares component objects and interns formats during writing. API call counts differ. Every value and requested style is verified outside timing through public reference and native streaming readers. Builds and checks are excluded during sampling.

| Styles/cells | Current seconds / peak MiB | Prior seconds / peak MiB | openpyxl seconds / peak MiB |
| --- | --- | --- | --- |
| 1,000 | 0.035950 / 2.410 | 0.063595 / 2.766 | 0.243708 / 38.316 |
| 8,000 | 0.264976 / 3.754 | 1.267749 / 6.871 | 0.603174 / 69.445 |

These are median observations including process baseline, not universal guarantees. Required reference speed and desired reference RSS targets hold here. No calamine creation overlap or rust_xlsxwriter comparison is claimed.

At 8,000 formats, the current writer retains 17 fonts, 18 fills and one border instead of 8,005 fonts, 8,007 fills and 8,005 borders. Current managed style storage is 1,415,152 bytes, including capacities and conservative indices; it is not allocator RSS. Style XML decreases from 5,179,787 to 2,453,781 bytes; reference XML is 1,742,267 bytes. Current/prior cell-format counts include the five established native defaults. Output ZIP sizes are 112,893 / 217,926 bytes, versus approximately 109,578 reference bytes.

The completed native row spool is exactly 475,748 bytes at 8,000 rows (55,748 at 1,000), unchanged by registration. Reference completed worksheet XML is 524,005 / 62,005 bytes. Temporary peaks sampled every 25ms are lower bounds; output ZIPs are excluded and cleanup is checked. Raw samples, table sizes and semantics are in [results](results/m2-style-registry.json). Separate default and complete-style public readback results are recorded alongside them.

## Reproduction

Build release examples `style_registry_fixture`, `style_registry_read`, `style_fixture` and `write_demo`, then run `python benchmarks/style_registry_checkpoint.py --baseline /path/to/prior-style_registry_fixture`. The preserved prior executable SHA-256 is `89d539242e495c6886195ae329e8d883eed28cc22350a1d88b42ea31fe140837`; its engine is the exact revision above with the same additional benchmark example. Use `CARGO_TARGET_DIR` consistently. Run `benchmarks/verify_default_styles.py` and `benchmarks/verify_style_fixture.py` for separate interoperability checks.

Collision checks, signed zero, logical failure atomicity, capacity-ledger audits and tight-budget growth fallback have deterministic tests. Compatible serialization drops alignment zero/false overrides as the public reference does; raw owned registration values remain distinct before serialization. Explicit retention is a separate writer policy. Large escaped attributes remain subject to independently configured reader event limits.
