# Shared temporal number-format assignment

Twenty public openpyxl 3.1.5 cases combine date/datetime/clock/duration values with General, numeric, date, clock and elapsed formats. Native and public saved readback match value/type, final code, explicit font/name/size/bold/color, fill/color, border/color and alignment. Existing date/duration codes remain unchanged even when they cause a different loaded temporal interpretation. Non-date codes derive the public default without copying shared components. No reference implementation is inspected. Workspace tests, rustfmt, Clippy and release interoperability pass.

Raw evidence: results/m2-temporal-styles-interop.json, results/m2-temporal-styles.json and results/m2-temporal-style-regression.json. The reusable borrowed registry key avoids per-cell alignment allocation after the first variant. Writer source rows remain immutable; owned-bank pre-save style mutation/view compatibility remains separate acceptance work.

Both feature writers stream five formats across four temporal kinds. Native registers the source styles once; public WriteOnlyCell sets the source format before assigning its temporal value. Every output feature is verified through public readback outside timing. One warmup plus five rotating serial cold-process wall/CPU/RSS samples include process/import baseline. Builds/tests do not overlap timing. No prior native API supports the whole explicit style assignment workload, so no equivalent old feature baseline is fabricated.

| Cells | crabxl seconds / peak RSS KiB | openpyxl seconds / peak RSS KiB |
| --- | --- | --- |
| 5,000 | 0.006901 / 2,344 | 0.321007 / 34,760 |
| 50,000 | 0.048894 / 2,368 | 1.411716 / 34,828 |

Required speed/desired RSS targets hold for this overlap. Native interned style storage is 6,323 managed bytes, independent of row count. Exact native worksheet spools are 241,275/2,471,031 bytes; sampled medians are zero/2,223,251 bytes. Public sampled medians are 243,196/2,693,800. Sampling every 25 ms misses short peaks, so native zero at the smaller size is not no temporary I/O. Cleanup is asserted and final ZIP output is outside monitored temporary space. No calamine creation or equivalent rust_xlsxwriter call API is claimed.

Normal combination creation is separately compared to an immediately preserved a090dc45d90e1b506eaf50df8d2482bd9c935bae binary (SHA256 c5a6504e003bd369407ed42efa7eb14d899d3ac029b5dd544f41f270d49d4c8d). Both native versions additionally re-register every style; public interning happens during row writing. Every value/style is verified after timing. Native default clock now uses built-in 21, reducing custom-format declarations from five to four for this fixture; output semantics for selected cells stay equal.

| Styles/cells | Current seconds / peak RSS KiB | Prior core seconds / peak RSS KiB | openpyxl seconds / peak RSS KiB |
| --- | --- | --- | --- |
| 1,000 | 0.034157 / 2,520 | 0.033285 / 2,596 | 0.234343 / 39,116 |
| 8,000 | 0.260292 / 3,940 | 0.257184 / 4,012 | 0.564514 / 71,112 |

The larger current result is 1.2% slower and 72 KiB lower RSS; this is a compatibility checkpoint, not a speed optimization claim. Managed registry bytes are 187,576/1,416,664, 120 bytes below the prior model. Exact native spools are unchanged at 55,748/475,748 bytes; public sampled medians are 32,864/488,723. All remaining milestone acceptance stays open.
