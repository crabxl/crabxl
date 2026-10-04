# Literal indexed palette interoperability

Five mixed-case and six/eight-digit palette entries agree with public openpyxl 3.1.5 RgbColor and ColorList properties after canonical source-catalog transfer and native save. The scalar worksheet is also reopened and checked. Run benchmarks/probe_palette_literals.py after building palette_literal_export in release mode. Rust source adoption retains vector ownership; malformed inputs and budget accounting have deterministic tests. Workspace tests, formatting and Clippy pass.

Raw regression evidence is results/m2-palette-literals-regression.json. The immediate baseline is core 26f4f5d7d18aaf36a2669174d925e71041f51159, preserved binary SHA256 265e16498dff4c89d73025d5189b837577f4116673d9afe1765694a73800f23d. One warmup and five rotating serial cold-process samples run with no simultaneous builds/tests. Each reader validates all 100,000 values across numeric/date/clock/duration/boolean/text/error/cache/integer types. Native prepares all source formats and streams cache-only values; public openpyxl uses read-only projection. Additional format declarations are unused, exposing preparation costs.

| Declared formats | Current seconds / peak RSS KiB | Prior seconds / peak RSS KiB | openpyxl seconds / peak RSS KiB |
| --- | --- | --- | --- |
| 1,001 | 0.091809 / 2,184 | 0.091896 / 2,056 | 0.544729 / 34,808 |
| 50,001 | 0.122274 / 4,704 | 0.121727 / 4,744 | 0.766346 / 89,136 |

The larger case is 0.4% slower with 40 KiB lower median RSS; the smaller case is approximately unchanged with 128 KiB higher RSS. RSS noise does not negate the definite managed allocation increase: palette slots are now eight bytes rather than four, adding 256 bytes for the 64-entry source palette. No speed or memory optimization is claimed. Required openpyxl speed and desired RSS hold for this workload; calamine and rust_xlsxwriter overlap comparisons remain open. All readers report zero sampled temporary space and assert cleanup; sampling is not a universal peak guarantee.
