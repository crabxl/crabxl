# Source-backed worksheet copy

This release workflow opens two generated numeric sheets, copies the first,
changes copied A1 to 42, saves all three sheets, and fully reads the output.
Source and copy cells reside in one canonical bank; the unrelated second source
sheet remains lazy. Counts/checksums, new catalog/body, changed copied value and
cleanup are verified. These are functional/resource measurements, without a
before/after or equivalent third-party comparison.

Rust 1.99.0, two-CPU quota, 8 GiB ceiling, explicit 1 GiB model allowance and
768 MiB per-sheet allowance. Default conservative aggregate accounting rejected
the first attempt; these explicit limits are part of the measured configuration.
One warmup and three serial runs exclude fixture generation and builds.
See [raw measurements](results/alpha7-loaded-copy.json) for hashes/environment.

| Original cells | Copied cells | Complete wall seconds | Peak RSS KiB | Managed bytes |
|---|---:|---:|---:|---:|
| 200,000 | 100,000 | 0.633661 | 10,076 | 51,217,512 |
| 2,000,000 | 1,000,000 | 6.389507 | 67,096 | 512,017,512 |

Time includes source materialization, requested copy/edit, save and complete
readback of every output cell. Managed bytes are a conservative admission ledger,
not actual RSS; their large difference remains a resource-accounting backlog.
Adjacent ZIP files measured at completion were 908,913 and 8,828,386 bytes.
Periodic working-file samples are lower bounds; SST temporary bytes are zero
and output cleanup is verified. No source XML or unrelated model is cloned.

```sh
python3 benchmarks/alpha6_loaded_bank.py --copy-only \
  --output benchmarks/results/alpha7-loaded-copy.json
```
