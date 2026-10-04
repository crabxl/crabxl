# Opaque theme checkpoint

Public APIs verify exact default/custom/opaque theme bytes, empty fallback and explicit omission. [Interop results](results/m2-theme-interop.json) include payload sizes, managed storage and SHA-256. The default theme is generated through openpyxl 3.1.5 Workbook.save, held in static Rust storage with ASCII Unicode escapes and emitted exactly. Default emission adds 10,140 uncompressed bytes (see generation manifest for the exact payload); it does not allocate an owned default theme per writer.

## Creation regression

The same style-registry workload compares this checkpoint against exact prior core `ad068e47ee6eea32c8437fc55e1590b2c57626d7`: one warmup, five rotating serial samples, every public/native value and requested style checked outside timing, no builds/checks during sampling. Both native versions additionally duplicate-register all styles; reference API call counts differ. RSS includes process baseline.

| Styles/cells | Current seconds / RSS KiB | Previous seconds / RSS KiB | openpyxl seconds / RSS KiB |
| --- | --- | --- | --- |
| 1,000 | 0.034559 / 2,572 | 0.034812 / 2,444 | 0.243034 / 39,128 |
| 8,000 | 0.258806 / 3,980 | 0.259462 / 3,808 | 0.576852 / 71,128 |

Wall differences are -0.7% / -0.3%, observations rather than an optimization claim. Required reference speed and desired reference RSS remain satisfied here. RSS increases by 128 / 172 KiB; default-theme storage is static, so this is not an owned payload allocation measurement. Managed style counts/bytes and row spools are unchanged. Exact native completed spools remain 55,748 / 475,748 bytes; 25ms temporary sampling is a lower bound, excludes ZIP output and checks cleanup. Current output ZIPs add approximately 1,688 bytes. Raw [samples](results/m2-theme-creation-regression.json) retain sizes and all runs. No overlap comparison with calamine creation or rust_xlsxwriter is claimed.

Reproduce using release `theme_fixture` with `verify_themes.py --native /path/to/theme_fixture`, and `style_registry_checkpoint.py --baseline /path/to/previous-example --baseline-core ad068e47ee6eea32c8437fc55e1590b2c57626d7 --output results.json`. Preserved previous registry executable SHA-256: `5c42b64496b11928260fb7fed4feeafaee89965b17e9864305c2c5cf18926d65`.

Theme access is lazy and opaque, matching the public baseline; strict validation is separate. This is not parsed palette/font support or loaded-bank editing. Shared metadata input allowance covers prepared theme/style parts, while aggregate Auto work/retained allocation remains staged.
