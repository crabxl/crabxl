# Alpha.5 concurrency-aware Auto evidence

Linux x86_64, Rust 1.99 release. Two worksheets share one SST; each scan uses an
independent reader. One warmup and three rotating serial samples include every
value/order verification and all ZIP/XML traversal. Generation/builds/tests are
excluded. The host CPU quota is two CPUs and the memory ceiling is 8 GiB.

Controlled caller availability is supplied to both the operation and SST Auto
policies. Serial executes two scans with concurrent_operations=1; parallel uses
two threads with concurrent_operations=2. Derived per-operation budgets are
16/8 MiB at 64 MiB availability and 256/128 MiB at 1,024 MiB. Explicit budgets
and default concurrency=1 are unchanged. The count does not create workers or
reserve global memory.

| Rows/sheet | Strings | Availability MiB | Mode | Seconds | Peak RSS KiB | Logical temp upper bound bytes |
| ---: | --- | ---: | --- | ---: | ---: | ---: |
| 10,000 | repeated | 64 | serial | 0.1657 | 2,096 | 0 |
| 10,000 | repeated | 64 | parallel | 0.0889 | 2,228 | 0 |
| 10,000 | repeated | 1,024 | serial | 0.1715 | 2,008 | 0 |
| 10,000 | repeated | 1,024 | parallel | 0.0865 | 2,220 | 0 |
| 10,000 | unique | 64 | serial | 0.3187 | 16,084 | 0 |
| 10,000 | unique | 64 | parallel | 0.2348 | 17,228 | 25,200,000 |
| 10,000 | unique | 1,024 | serial | 0.3079 | 16,088 | 0 |
| 10,000 | unique | 1,024 | parallel | 0.1591 | 30,312 | 0 |
| 100,000 | repeated | 64 | serial | 1.5944 | 2,096 | 0 |
| 100,000 | repeated | 64 | parallel | 0.8471 | 2,120 | 0 |
| 100,000 | repeated | 1,024 | serial | 1.5812 | 2,036 | 0 |
| 100,000 | repeated | 1,024 | parallel | 0.8429 | 2,220 | 0 |
| 100,000 | unique | 64 | serial | 4.2866 | 19,036 | 126,000,000 |
| 100,000 | unique | 64 | parallel | 2.1667 | 17,292 | 252,000,000 |
| 100,000 | unique | 1,024 | serial | 3.2066 | 143,320 | 0 |
| 100,000 | unique | 1,024 | parallel | 1.6398 | 283,368 | 0 |

Parallelism improves wall time on these two-CPU workloads, but duplicates package
catalogs and string stores and can increase RAM/disk cost. More caller availability
can keep high-cardinality SSTs in RAM and avoid temporary files; this is measured
workload evidence, not a guarantee that larger budgets or more workers are faster.
Keep concurrency caller-controlled; do not silently thread every scan.

Temporary bytes are completed logical data/index sizes: the maximum single-reader
store for serial execution, and the sum as an upper bound for overlapping parallel
stores. They exclude filesystem block allocation, OS cache and total filesystem
sampling. Auto can transiently retain growing allocations before spill, so RSS is
not the final managed ledger and can stay higher than forced disk. Successful
processes leave no owned temporary files; raw samples include storage/cache
diagnostics for both readers. No hard process RSS limit is claimed.

[Raw measurements](results/alpha5-concurrent-auto.json) include source/executable
hashes, CPU/wall/RSS, budgets and cleanup checks. Historical single-sheet/SST
comparisons remain separate; no new cross-library speed claim is made.

```sh
cargo build --release -p crabxl --example concurrent_shared_text --locked
python benchmarks/alpha5_concurrent_auto.py \
  --binary target/release/examples/concurrent_shared_text \
  --measure /path/to/measure --output /tmp/concurrent-auto.json
```
