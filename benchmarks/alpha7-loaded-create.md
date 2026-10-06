# Source-backed worksheet creation

The current release workflow opens two generated numeric sheets, selects the
second, creates and populates a third worksheet, saves, then reads all output
cells to EOF. It verifies the new relationship/catalog/body and checks the full
cell count and checksum. Original sheets remain unmaterialized; only one new
cell resides in the bank. This is a functional/resource checkpoint, not a
before/after optimization or a calamine comparison.

Rust 1.99.0, two-CPU quota, 8 GiB memory ceiling; one warmup followed by three
serial runs. Fixture generation and builds are excluded. Raw measurements,
binary/input hashes and environment are in
[alpha7-loaded-create.json](results/alpha7-loaded-create.json).

| Original cells | Complete wall seconds | Peak RSS KiB | Managed retained bytes |
|---|---:|---:|---:|
| 200,000 | 0.334804 | 3,540 | 17,767 |
| 2,000,000 | 3.354631 | 3,456 | 17,767 |

Wall time includes complete output decoding, not just opening or lazy iteration
construction. The adjacent ZIP working file measured at completion was 607,622
and 5,888,152 bytes respectively. Periodic working-file samples are lower bounds;
SST temporary bytes are zero and output cleanup is verified. No worksheet body
is buffered or cloned. Copy/removal and the A7 acceptance gate remain open.

Reproduce after building both examples, with no competing build/test workloads:

```sh
python3 benchmarks/alpha6_loaded_bank.py --create-only \
  --output benchmarks/results/alpha7-loaded-create.json
```
