# Cache-only formula projection checkpoint

All readers stream the complete worksheet and verify every integer cache. Each normal formula contains 64 repeated cell references; expressions are deliberately discarded by data_only. One warmup and five rotating serial cold-process samples include runtime baseline; no builds/checks run during timing. Full formula evaluation is outside the baseline.

| Rows | Current seconds / RSS KiB | Previous seconds / RSS KiB | openpyxl seconds / RSS KiB |
| --- | --- | --- | --- |
| 10,000 | 0.017480 / 1,992 | 0.018730 / 1,992 | 0.265715 / 36,208 |
| 100,000 | 0.165880 / 1,928 | 0.187289 / 1,996 | 1.136082 / 44,508 |

The large workload improves wall time by approximately 11.4% against the previous engine. Required reference speed and desired reference RSS goals hold here. No temporary storage, worksheet materialization, calamine comparison or writer comparison is claimed. Malformed-header projection and bounded XML rejection are separate deterministic tests.

Previous core: 5e643fe70d4facde50155097c3234a0259f033ee; preserved common-driver binary SHA-256 cbf7c63aa87e41ae4e9e9c5198b808d6d2108e0f753ec3ea41dd80bcedd9e678. Reproduce with formula_cache_checkpoint.py and the formula_cache_read example; raw samples: results/m2-formula-cache.json. Managed limits are distinct from peak process RSS. No milestone completion is claimed.
