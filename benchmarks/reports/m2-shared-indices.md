# Literal shared formula identities

Fifteen public openpyxl 3.1.5 cases cover absent/empty/canonical/leading-zero/plus/negative/overflow/opaque/whitespace/escaped identifiers, including mismatched spelling groups. Every native expression and public cache matches. Deterministic tests additionally cover source identity allocation/sharing, stream/projection/materialization, strict validation and atomic writer XML rejection/retry. Workspace tests, formatting and Clippy pass.

Raw evidence: results/m2-shared-index-interop.json, results/m2-shared-index-regression.json and results/m2-shared-index-literal.json. Both readers stream and check every expanded expression. Native checks cached integers in the same pass; public caches use a separate untimed data_only pass. One warmup plus five rotating serial cold-process wall/CPU/RSS samples include runtime/import baseline. No builds/tests overlap timing. Temporary samples are zero and cleanup asserted; these reads allocate no worksheet spools.

The immediate prior core is 1b1a8726f266917ec589ae4eefc43d0e61c9b732; preserved binary SHA256 e85fb038feaaa4f4feb1cd035f7bf0d780cbbf03903c54eab8ba4f27d770eb18. The numeric group remains allocation-free, but the larger key raises its conservative template estimate from 355 to 419 bytes. No speed/memory optimization is claimed.

| Numeric cells | Current seconds / peak RSS KiB | Prior seconds / peak RSS KiB | openpyxl seconds / peak RSS KiB |
| --- | --- | --- | --- |
| 20,000 | 0.032876 / 2,076 | 0.031420 / 1,928 | 0.357810 / 39,736 |
| 200,000 | 0.310403 / 1,992 | 0.310106 / 1,936 | 2.070034 / 47,804 |

The larger current result is approximately 0.1% slower and 56 KiB higher RSS; the smaller result is 4.6% slower with 148 KiB higher RSS. Required openpyxl speed and desired RSS hold for this workload. Equivalent calamine streaming formula/cache comparison remains open.

The literal feature workload uses group & value, including entity decoding and transient literal ownership. Retained metadata interns against one shared key allocation. Its template estimate is 448 bytes: numeric fixed key storage plus 13 string bytes and a 16-byte shared ownership header. No equivalent old native API accepts this whole identifier domain.

| Literal cells | crabxl seconds / peak RSS KiB | openpyxl seconds / peak RSS KiB |
| --- | --- | --- |
| 20,000 | 0.033623 / 1,944 | 0.373089 / 39,736 |
| 200,000 | 0.325787 / 2,048 | 2.100532 / 47,412 |

These results are native readers, not Python binding calls. Complete formula and milestone acceptance remain open.
