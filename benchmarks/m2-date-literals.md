# M2 literal datetime, clock and duration precision

Core literal values preserve microseconds and original Gregorian identity while imported numeric serials retain pinned baseline millisecond conversion. This fixes the model distinction needed for language bindings without reimplementing date semantics in an adapter. Boxed payload accounting includes the enlarged representation; it remains one transient row in streaming mode.

`cargo run --release -p crabxl --example date_fixture -- <win-path>` and the same command with `<mac-path> mac` generate four original interoperability cells. `python benchmarks/verify_date_fixture.py <win-path> <mac-path>` verifies all resulting values/types through public openpyxl 3.1.5. The literal calendar/clock/duration inputs retain microseconds in core, while numeric XLSX readback rounds to milliseconds, matching the pinned reference. The ambiguous 1899-12-31 calendar value becomes a clock under Windows numeric readback and preserves its calendar day under Mac readback. Results are in [m2-date-literals-interop.json](results/m2-date-literals-interop.json).

The standard mixed numeric/date/cache benchmark was rerun after the representation change: `python benchmarks/styled_checkpoint.py --output benchmarks/results/m2-date-literals-read.json`. One warmup/five rotating serial runs verify all values, independently computed calendar components and shared catalog counts. Linux wall/CPU/RSS include runtime baseline, exclude builds/generation, and have no temporary storage for this workload. The host remains limited to two CPU quota units and 8 GiB. Calamine materializes overlapping values, while native/openpyxl stream; different retention modes are explicit.

| Cells | Engine | Median seconds | CPU seconds | RSS MiB |
| ---: | --- | ---: | ---: | ---: |
| 100,000 | calamine | 0.0382 | 0.0380 | 8.68 |
| 100,000 | crabxl | 0.0903 | 0.0902 | 1.88 |
| 100,000 | openpyxl | 0.5521 | 0.7069 | 33.48 |
| 1,000,000 | calamine | 0.4136 | 0.4134 | 73.18 |
| 1,000,000 | crabxl | 0.8861 | 0.8851 | 1.93 |
| 1,000,000 | openpyxl | 4.1261 | 4.3034 | 41.23 |

Required speed against openpyxl remains met; desired calamine speed remains unmet. Native RSS remains lower than both. Retained style capacity remains 2,829 bytes, separate from transient date payloads and other working allocations. Full samples and generated input hashes are in [m2-date-literals-read.json](results/m2-date-literals-read.json). Historic style-checkpoint measurements remain separate; no precision/date-only/ISO completion is inferred from this workload.

Deterministic tests retain microsecond literals, imported rounding, early calendar epoch identity, negative one-microsecond durations, maximum baseline duration and malformed/overflow boundaries. ISO/date-only codecs, safe original-package date assignment and aggregate loaded accounting remain open before complete M2/M4 acceptance.
