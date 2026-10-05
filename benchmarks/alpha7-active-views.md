# Deferred active-view checkpoint

Same generated two-sheet numeric fixtures, Linux native `wait4`, one warmup and
three rotating serial release runs as the visibility checkpoint. Build and
generation are excluded. [Raw samples](alpha7-active-views.json) include exact
binary hash, quotas, wall/CPU/RSS and complete integer count/checksum checks.

| Rows per sheet | Request | Wall seconds | Peak RSS KiB | Managed bytes | Completed ZIP bytes |
| --- | --- | ---: | ---: | ---: | ---: |
| 10,000 | Visible stable ID | 0.13364 | 2,880 | 13,851 | 606,812 |
| 10,000 | Deferred relative view `-1` | 0.14003 | 2,880 | 13,851 | 606,813 |
| 100,000 | Visible stable ID | 1.38065 | 2,944 | 13,851 | 5,882,396 |
| 100,000 | Deferred relative view `-1` | 1.42875 | 2,880 | 13,851 | 5,882,397 |

Both requests save and stream every output cell back, retain zero materialized
cells and use no SST temporary files. The signed spelling adds one output byte;
managed costs are identical and constant across these sizes. Timings include
full output verification, not only the small view edit. This is correctness and
resource evidence, not an optimization or Python performance claim.

Full-model controls remain in the report. At 100,000 rows per sheet, the bank
median is 1.89176 seconds versus standalone 1.79046 seconds. Together with earlier
samples, this coordination cost and variability remain an Alpha 8 profiling
task, not a claimed improvement.

```sh
python benchmarks/alpha6_loaded_bank.py --active --deferred \
  --checkpoint 'Alpha 7 deferred view checkpoint; M4 stage remains open' \
  --output benchmarks/alpha7-active-views.json
```
