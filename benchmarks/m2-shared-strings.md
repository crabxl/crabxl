# M2 plain shared-string checkpoint

Historical measurements from core `dd38e85ae81b5f56e620b9b04a63b0a631905490`. Subsequent typed rich-text support and separate measurements do not change these original samples.

Rust 1.88 release; openpyxl 3.1.5 read-only; calamine 0.36.1 public Range. Generated fixtures contain ten columns of 110-byte strings, either 128 repeated strings or one unique string per cell. Every engine verifies every complete value and its position, not only counts. One warmup and five rotating serial measurements include initialization and full SST parsing, worksheet decoding, owned value conversion and validation. Raw runs, input hashes, CPU, RSS and store diagnostics are in [results](results/m2-shared-strings.json).

| Rows / cardinality | Rust RAM s / MiB | Rust disk s / MiB | Rust Auto s / MiB | calamine s / MiB | openpyxl s / MiB |
|---|---:|---:|---:|---:|---:|
| 10,000 / repeated | 0.087 / 1.57 | 0.093 / 1.69 | 0.088 / 1.58 | 0.045 / 16.78 | 0.442 / 33.57 |
| 10,000 / unique | 0.154 / 15.31 | 0.202 / 2.23 | 0.158 / 15.33 | 0.079 / 31.27 | 0.964 / 56.70 |
| 100,000 / repeated | 0.873 / 1.58 | 0.886 / 1.71 | 0.873 / 1.59 | 0.445 / 154.12 | 3.079 / 41.32 |
| 100,000 / unique | 1.597 / 138.83 | 2.013 / 2.19 | 2.057 / 18.21 | 0.797 / 299.16 | 9.304 / 277.23 |

All measured Rust paths beat openpyxl on these fixtures. calamine is approximately twice as fast as the Rust RAM reader and remains an unmet desired speed target. Rust retains lower RSS in these cases. This is not a claim about other values, formats or universal workload performance. calamine materializes its public Range; Rust streams rows after preparing its SST, while openpyxl read-only eagerly prepares SST. Retention semantics differ and are reported explicitly.

The forced RAM run uses a 512 MiB component policy. Forced disk and Auto placement use a 16 MiB explicit policy with a 1 MiB decoded-cache ceiling. Thus Auto here tests actual budget spill, not unrestricted host availability. The caller-availability Auto policy has deterministic tests.

For one million unique strings, both disk-backed data and index total **126,000,000 logical bytes (120.16 MiB)**. The full index is on disk, not hidden in RAM. Disk retains approximately 548,864 managed cache bytes after reading, plus parser/row/dependency working space. With 128 repeated strings, disk performs 128 misses followed by cache hits and uses 16,128 temporary bytes. Temporary files are anonymous owned files; successful process exit, failed initialization and reconfiguration cleanup are checked. OS page cache, filesystem allocation granularity and tmpfs costs are additional and excluded from process RSS.

Auto grows an in-memory table until actual capacity and payload accounting exceed its allowance, then spills to disk. Its peak RSS can remain higher than forced disk because the process previously allocated table memory; dropping allocations does not force the allocator to return every page immediately.

Numeric-reader regression against the preserved 5c80ec release path is recorded in [regression runs](results/m2-shared-numeric-regression.json): 10k rows +5.4% wall time, 100k rows -0.8%. No consistent throughput improvement is claimed. Small-file startup/code-footprint overhead remains an optimization item.

Reproduce on Linux:

```sh
PATH=/home/agent/.cargo/bin:$PATH python benchmarks/shared_strings_checkpoint.py --rows 10000 100000 --runs 5 --output benchmarks/shared-strings.local.json
```

SST paths follow relationships; actual entries define IDs despite oversized advertised uniqueCount. XML/entity decoding uses the same bounded helper as scalar/inline values. Plain shared text removes the pinned public reference marker `x005F_`; arbitrary `_xHHHH_` sequences are not decoded. Inline literals keep their spelling. [Public observations](../docs/research/shared-string-public-probe.json) were obtained without implementation-source inspection.

Rich/phonetic entries retain IDs and reject selected access. They are not typed rich-text support. M2 remains in progress with rich text, styles/dates and non-normal formulas required.
