# Alpha.5 exact style integer identities

`StyleInteger` preserves arbitrary decimal charset/theme/indexed metadata with
an allocation-free i64 fast path. Canonical normalization and ownership are
shared across fonts, rich runs, fill/gradient colors, borders, differential
styles and recent colors. Component and aggregate accounting includes rare
boxed integer payloads. See ADR 0054 and the reproducible public probe:

```sh
cargo build --release --example style_integer_domains
python benchmarks/probe_style_integer_domains.py --native target/release/examples/style_integer_domains --output benchmarks/results/alpha5-style-integer-interop.json
```

Native output is independently checked through openpyxl 3.1.5 for positive and
negative i64 overflow and a 41-digit identity. Public constructors retain these
values. Reference self-save instead writes scientific notation and fails its
own reload; the probe records this limitation without copying it into CrabXL.

One warmup and three rotating serial release samples compare exact earlier
core `ca40f674df0f97939821182c1e1f8c137b5ed3ae` against this checkpoint.
The intermediate formula/concurrency commits do not change style creation.
The existing style_registry_checkpoint harness validates every output cell,
style attributes and canonical component counts outside timing. Native samples
also re-register every style to verify deduplication; reference API call counts
differ. No build/test runs overlap timing.

| Styles | Current seconds / RSS KiB | Earlier seconds / RSS KiB | openpyxl seconds / RSS KiB |
| --- | --- | --- | --- |
| 1,000 | 0.037516 / 2,328 | 0.036740 / 2,188 | 0.188983 / 28,736 |
| 8,000 | 0.269143 / 3,684 | 0.274146 / 3,692 | 0.553531 / 39,740 |

Observed wall differences are +2.1% and -1.8%; no optimization claim is made.
Managed styles grow by 488 bytes on both sizes to 188,120 / 1,417,208 bytes.
The small domain model increases Color to 40 bytes and Font to 120 bytes on
this target; shared component counts limit the impact on this workload.
Rare large identities consume additional accounted wrapper/decimal bytes.
Required reference speed and desired lower reference RSS remain satisfied here.
Exact completed native worksheet spools remain 55,748 / 475,748 bytes. Sampled
25ms temporary peaks are lower bounds, exclude output ZIP and include cleanup
checks. The one-million-cell/concurrent SST evidence is recorded separately.

Raw CPU/wall/RSS, temporary and output sizes, binary identity and observations:
[regression samples](results/alpha5-style-integer-regression.json).
Reproduce with `style_registry_checkpoint.py --baseline <earlier executable>
--baseline-core ca40f674df0f97939821182c1e1f8c137b5ed3ae --runs 3 --output <json>`.
No calamine write overlap or rust_xlsxwriter direct comparison is claimed.
