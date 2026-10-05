# Alpha.4 ZIP compression evidence

Linux x86_64, Rust 1.99 release builds; zip 8.6, flate2 1.1.10 and zlib-rs 0.6.8. Native zlib is selected exclusively with no default features. Each workload uses 100,000 rows and ten columns. One warmup and three rotating serial samples include complete creation or original-source A1 replacement/save; builds, tests and CRC/hash readback are excluded from timing.

| Workload | Backend | Level | Median seconds | ZIP bytes | Peak RSS KiB |
| --- | --- | ---: | ---: | ---: | ---: |
| numeric | zlib-rs | 0 | 0.1646 | 32,881,746 | 1,712 |
| numeric | zlib-rs | 1 | 0.1787 | 5,513,484 | 2,032 |
| numeric | zlib-rs | 3 | 0.2487 | 3,299,121 | 2,040 |
| numeric | zlib-rs | 6 | 0.9200 | 2,942,935 | 2,000 |
| numeric | zlib-rs | 9 | 1.3189 | 3,031,461 | 2,012 |
| numeric | zlib | 0 | 0.1501 | 32,881,746 | 1,608 |
| numeric | zlib | 1 | 0.2146 | 3,290,847 | 1,864 |
| numeric | zlib | 3 | 0.3291 | 3,179,811 | 1,856 |
| numeric | zlib | 6 | 1.0815 | 3,031,631 | 1,864 |
| numeric | zlib | 9 | 4.9449 | 3,030,205 | 1,788 |
| mixed | zlib-rs | 0 | 0.1957 | 48,770,636 | 1,728 |
| mixed | zlib-rs | 1 | 0.2269 | 6,673,671 | 2,036 |
| mixed | zlib-rs | 3 | 0.3093 | 4,022,235 | 2,072 |
| mixed | zlib-rs | 6 | 0.6281 | 3,632,101 | 2,080 |
| mixed | zlib-rs | 9 | 1.1126 | 3,678,082 | 1,988 |
| mixed | zlib | 0 | 0.1952 | 48,770,636 | 1,604 |
| mixed | zlib | 1 | 0.2802 | 4,304,732 | 1,828 |
| mixed | zlib | 3 | 0.3165 | 4,216,886 | 1,832 |
| mixed | zlib | 6 | 0.6264 | 3,826,822 | 1,864 |
| mixed | zlib | 9 | 2.9764 | 3,678,078 | 1,860 |
| edit | zlib-rs | 0 | 0.7004 | 38,872,160 | 2,096 |
| edit | zlib-rs | 1 | 0.7155 | 5,410,004 | 2,364 |
| edit | zlib-rs | 3 | 0.7529 | 3,155,745 | 2,368 |
| edit | zlib-rs | 6 | 1.3052 | 3,156,360 | 2,372 |
| edit | zlib-rs | 9 | 1.7273 | 3,144,077 | 2,392 |
| edit | zlib | 0 | 0.7124 | 38,872,160 | 1,900 |
| edit | zlib | 1 | 0.7767 | 3,243,544 | 2,176 |
| edit | zlib | 3 | 0.9057 | 3,243,378 | 2,164 |
| edit | zlib | 6 | 1.4204 | 3,141,091 | 2,248 |
| edit | zlib | 9 | 4.7759 | 3,142,778 | 2,304 |

The default remains pure-Rust zlib-rs at level 6, matching the former default algorithm and avoiding a native C build requirement. Level 3 is a useful explicit balance in these workloads: numeric creation is 3.70x faster with about 12.1% larger ZIP output, while edit/save is 1.73x faster with nearly unchanged output size. Native zlib is not universally faster or smaller. Level 9 can be slower and even produce larger output; no monotonic time/size guarantee is claimed.

All uncompressed package-part hashes agree across both backends and all levels within each workload; reading parts to EOF validates ZIP CRCs. Level 0 uses ZIP Stored. Unchanged editor entries preserve their original compressed bytes regardless of the selected level. These measurements do not establish universal performance or complete feature support.

Creation retains bounded worksheet XML spools (32,866,902 numeric bytes; 48,755,792 mixed bytes). Editing adds no worksheet spool. Atomic path saves still stage a complete output ZIP, whose sizes are reported above, alongside any old target. These are logical storage costs, not sampled total filesystem peaks. Peak RSS excludes OS cache and is not a hard memory cap.

Default and native-zlib Rust 1.88 tests pass. Focused tests cover values/CRC, raw passthrough, invalid levels, output protection and retry. Raw samples, source and executable hashes are in [results](results/alpha4-compression-levels.json).

```sh
python benchmarks/compression_levels.py --source /path/to/numeric.xlsx \
  --measure /path/to/measure --binary-prefix /path/to/crabxl-alpha4 \
  --output /tmp/compression.json
```

Build/copy edit_demo and write_demo under the rs prefix with default features, then build/copy under the zlib prefix using --no-default-features --features deflate-zlib. Example binaries accept an optional final compression-level argument; write_demo uses `-` for the default temporary directory before that argument.
