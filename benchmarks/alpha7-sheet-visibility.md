# Lazy sheet visibility checkpoint

Generated two-sheet numeric workbooks, ten columns per sheet. One warmup and
three rotating serial release runs per mode; native Linux `wait4` captures wall,
CPU and peak RSS. Generation and build are excluded. Raw samples, binary SHA,
platform, quota and checksums are in [the report](alpha7-sheet-visibility.json).

Both metadata modes save to adjacent temporary files and stream every output
cell back to verify the complete count/checksum and active selection. Visibility
also verifies the saved hidden state. Neither materializes worksheet cells.

| Rows per sheet | Operation | Wall seconds | Peak RSS KiB | Managed bytes | Completed ZIP bytes |
| --- | --- | ---: | ---: | ---: | ---: |
| 10,000 | Active selection | 0.14359 | 2,876 | 13,851 | 606,812 |
| 10,000 | Hide first sheet and normalize selection | 0.13709 | 2,940 | 14,107 | 606,822 |
| 100,000 | Active selection | 1.31603 | 2,876 | 13,851 | 5,882,396 |
| 100,000 | Hide first sheet and normalize selection | 1.36133 | 2,960 | 14,107 | 5,882,406 |

Visibility uses one additional fixed 256-byte overlay charge and ten additional
XML bytes in this fixture. No SST temporary files are needed; completed output
uses disk and is removed after verification. Peak RSS includes dependency and
process costs, not just the ledger. Timings include the full output scan and are
not a metadata-only parser measurement or an optimization claim.

The full model controls remain in the raw report. At 100,000 rows per sheet,
bank loading takes 1.81057 seconds versus standalone 1.73490 seconds with matching
158,008 KiB RSS. This measured ownership/coordination cost remains in the Alpha 8
performance backlog; it is not hidden by comparing metadata-only operations to
full materialization. Python is not measured here.

Reproduce after building `loaded_rows`, `workbook_demo` and `benchmarks/measure`:

```sh
python benchmarks/alpha6_loaded_bank.py --active --visibility \
  --checkpoint 'Alpha 7 core visibility checkpoint; M4 stage remains open' \
  --output benchmarks/alpha7-sheet-visibility.json
```
