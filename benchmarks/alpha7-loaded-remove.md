# Source-backed worksheet removal

The release workflow opens two generated numeric sheets, validates owners,
removes and drops the first canonical model, saves, then completely reads the
remaining sheet. The returned detached model is discarded by this workload;
caller-retained detached objects have separate costs. Catalog/body count,
active selection, cell count/checksum and cleanup are verified.

Rust 1.99.0, two-CPU quota, 8 GiB ceiling, explicit 1 GiB model allowance and
768 MiB per-sheet allowance. One warmup and three serial runs exclude builds and
fixture generation. See [raw measurements](results/alpha7-loaded-remove.json)
for input/binary hashes and environment. There is no before/after speedup claim
or equivalent third-party comparison.

| Original cells | Remaining cells | Complete wall seconds | Peak RSS KiB | Retained managed bytes |
|---|---:|---:|---:|---:|
| 200,000 | 100,000 | 0.298209 | 5,952 | 16,648 |
| 2,000,000 | 1,000,000 | 3.005568 | 34,580 | 16,648 |

Wall time includes owner/source validation, materialization required for the
owned removal result, its disposal, save and every output row decoded to EOF.
The unrelated retained worksheet stays lazy. Managed bytes are the remaining
bank/source admission ledger, not transient peak RSS. Adjacent ZIP working files
measured at completion were 305,470 and 2,945,735 bytes. Periodic working-file
samples are lower bounds; SST temporary bytes are zero and cleanup is verified.

```sh
python3 benchmarks/alpha6_loaded_bank.py --remove-only \
  --output benchmarks/results/alpha7-loaded-remove.json
```
