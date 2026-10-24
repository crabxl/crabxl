# Owned bank theme checkpoint

Snapshot workload: clone 64 immutable holders, drop the original and verify every byte in all holders. One warmup and five rotating serial cold-process samples; no builds during timing and no temporary storage. This is a native ownership extension, without cross-package speed or Python binding claims.

| Theme payload | Current seconds / RSS KiB | Previous seconds / RSS KiB |
| --- | --- | --- |
| 64 KiB | 0.003455 / 1,088 | 0.003538 / 5,052 |
| 1 MiB | 0.040800 / 2,968 | 0.042874 / 66,700 |

The large case shares one payload rather than retaining 64 copies. Each managed holder conservatively charges the full payload; actual RSS and managed accounting are distinct. Source conversion transients and caller-held snapshots are additional to library bank retention. Raw samples: results/m4-theme-snapshots.json, reproduction: theme_snapshot_checkpoint.py and theme_snapshots example.

Representative style-writer regression, same shared-combination fixture and all output properties checked:

| Styles | Current seconds / RSS KiB | Previous seconds / RSS KiB | openpyxl seconds / RSS KiB |
| --- | --- | --- | --- |
| 1,000 | 0.034683 / 2,496 | 0.035094 / 2,624 | 0.239605 / 39,120 |
| 8,000 | 0.264460 / 3,948 | 0.259776 / 3,920 | 0.617802 / 71,120 |

Large native wall is approximately 1.8% slower; no writer optimization is claimed. Required public-reference speed and desired public-reference RSS hold here. Exact native temporary XML is 55,748/475,748 bytes; sampled reference temp peaks are 8,206/521,575 bytes and can miss short peaks. Duplicate-registration native passes and reference call-count differences remain as documented in m2-style-registry.md. Raw results: results/m4-bank-theme-writer-regression.json.

Prior core: 9a13237f06b349eef12ac2f358b4eee4c66f1d19. Preserved snapshot binary SHA-256: 38e31271713a480e4c918bfb1b21c0cd4100a66f736775be2e67638d5651a94b; style fixture binary SHA-256: 60f94d7e126249499dc6360d9240348df42263debf20b7e100d809a73a0b825b. No calamine/rust_xlsxwriter overlap or milestone completion is claimed.
