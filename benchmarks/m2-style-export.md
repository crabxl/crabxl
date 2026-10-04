# Source style export checkpoint

Both engines load the ten stored typed/cache values and declared style catalogs, change A1 number format, and save a new package. Every saved value and listed public font/fill/alignment/protection/number-format property agrees with openpyxl 3.1.5. Fixtures contain no advanced/opaque graphs; complete original-package preservation is not claimed.

One warmup and five rotating serial cold-process samples; no concurrent builds/checks. Public readback is outside timing.

| Input declared formats | Native seconds / RSS KiB / output bytes | openpyxl seconds / RSS KiB / output bytes |
| --- | --- | --- |
| 1,001 | 0.007319 / 3,076 / 8,863 | 0.166525 / 35,592 / 4,701 |
| 50,001 | 0.284127 / 9,480 / 259,396 | 0.442750 / 90,488 / 4,701 |

Native retains all unused source declarations plus automatic presets: 1,004/50,004 number formats and seven cell formats. Public save prunes unused formats: one number format and three cell formats in both cases. Native styles XML is 59,671/3,080,172 bytes versus public 2,536 bytes. Thus requested public outputs agree, but serialization work/file sizes differ substantially. Required reference speed and desired reference RSS hold despite this extra native work; no prior equivalent import/export API existed to benchmark. No calamine/rust_xlsxwriter claim is made.

Native temporary worksheet XML is exactly 540 bytes in both cases. Directory sampling reports 0/540 bytes and can miss short-lived files; public sampled temp peaks are zero. Deterministic writer cleanup/failure tests remain separate. Raw results/properties: results/m2-style-export.json; reproduction: style_export_checkpoint.py and style_catalog_export.

Default shared style-combination creation regression against 7b3ee6120d1beeec4eda1a7562bc2b9035c72a54:

| Styles | Native seconds / RSS KiB | Previous seconds / RSS KiB | openpyxl seconds / RSS KiB |
| --- | --- | --- | --- |
| 1,000 | 0.035247 / 2,496 | 0.034540 / 2,624 | 0.233259 / 39,120 |
| 8,000 | 0.262748 / 3,912 | 0.261779 / 3,920 | 0.593333 / 71,124 |

Large native wall is approximately 0.4% slower; no ordinary creation speed improvement is claimed. Exact native spool bytes remain 55,748/475,748. Native duplicate-registration passes, reference call-count differences and transient temp sampling limitations remain as documented in m2-style-registry.md. Raw results: results/m2-style-export-writer-regression.json. Preserved prior fixture SHA-256: 4b09109c685ca7b952c1cae93484c02ef6297a7b2788397f4e81e0223acf52b2.

No hard process RSS bound, loaded structural edit or milestone completion is claimed.
