# Public style-domain compatibility checkpoint

The expanded font/color records retain fractional family values, signed charsets/theme/indexed IDs and mixed-case hexadecimal colors. Public observations and output assertions are recorded in `m2-style-domains-interop.json` and `docs/research/style-domains-public.json`.

One warmup and five rotating serial creation samples repeat the canonical registry workload against exact previous core `0d15c98c1a802bde4352b3827b49bfd9d0e9637c`. Native/reference values and requested styles are checked outside timing. Native workflows include a duplicate-registration pass; reference API call counts differ. No build/check activity runs during timing.

| Styles | Current seconds / RSS KiB | Previous seconds / RSS KiB | openpyxl 3.1.5 seconds / RSS KiB |
| --- | --- | --- | --- |
| 1,000 | 0.033727 / 2,468 | 0.033490 / 2,388 | 0.247168 / 39,112 |
| 8,000 | 0.260891 / 3,828 | 0.260864 / 3,804 | 0.584290 / 71,116 |

Observed wall change is +0.7% at 1,000 and approximately +0.01% at 8,000; no performance improvement is claimed. Required reference speed and desired lower reference RSS remain satisfied on this workload. Managed style accounting increases by 1,568 bytes to 187,632 / 1,416,720 bytes. Output/spool sizes and component counts are unchanged: completed native spools are 55,748 / 475,748 bytes. Sampled temporary peaks are 25ms lower bounds, exclude output ZIP, and cleanup is checked. See [raw samples](results/m2-style-domains-regression.json).

Reproduce with `style_registry_checkpoint.py --baseline /path/to/previous-example --baseline-core 0d15c98c1a802bde4352b3827b49bfd9d0e9637c --output results.json`, after release example builds. Previous executable SHA-256: `ed52437f28a0d53828cbdee80355e7abd645d7a7256d9c3f5e1087905c5e1ad2`. For public readback build `style_domains`, generate an XLSX and run `probe_style_domains.py --native path.xlsx`.

The Rust signed integer domain is i64, not arbitrary-precision reference integers. Theme/palette resolution and existing-file style edits remain separate work. This checkpoint does not complete M2 or M4.
