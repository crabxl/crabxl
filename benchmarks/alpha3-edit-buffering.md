# Alpha.3 buffered edit/save evidence

Linux x86_64, Rust 1.99 release builds. The alpha.2 executable is built from
published core commit 6d617a48; the candidate adds bounded editor output buffering.
The input contains 100,000 rows and ten numeric columns. Both versions replace
A1 with -1 and save, retaining global formula-cache invalidation. One warmup and
three rotating serial cold-process samples include source opening and output
save. Build, tests and public readback do not overlap the final measurements.

| One million cells | Alpha.2 | Buffered candidate |
| --- | ---: | ---: |
| Median elapsed seconds | 7.0485 | 1.3450 |
| Median peak RSS KiB | 2,540 | 2,684 |
| Output ZIP bytes | 3,868,674 | 3,156,360 |

The measured edit/save is 5.24 times faster. The fixed 64 KiB buffer batches small
XML fragments before compression and also changes compressor chunking; ZIP output
is about 18.4% smaller. Every uncompressed package part is byte-identical between
versions. Public openpyxl readback verifies all one million values and the edited
checksum. This numeric workload does not establish universal gains or complete
editing support.

No worksheet spool is introduced. Atomic path saving still temporarily retains
the complete new ZIP alongside the old target; final ZIP sizes above describe that
logical output staging cost, not a sampled total filesystem high-water mark.
Peak RSS is not a hard process memory ceiling.

Existing Rust tests cover original-part preservation, cache invalidation, XML
byte limits, output failures/retry and temporary cleanup. Latest stable and Rust
1.88 workspace tests pass; Rustfmt and strict Clippy pass. Shared-string, printing
and view edits use the same explicitly flushed rewrite path.

Raw samples and source/binary hashes: [results](results/alpha3-edit-buffering.json).

```sh
python benchmarks/edit_buffering.py --before /path/to/alpha2-edit_demo \
  --after target/release/examples/edit_demo --source /path/to/numeric.xlsx \
  --measure /path/to/measure --output /tmp/edit-buffering.json
```
