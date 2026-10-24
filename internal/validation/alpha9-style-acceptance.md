# Alpha 9 style-assignment acceptance

Status: published and verified from the public Rust and Python registries.

## Scope

A9 adds canonical in-place style assignment, explicit temporal format export,
source number-format derivation, relationship-resolved stylesheet rewriting,
and compatible Python `Cell.number_format`, style diagnostics and write-only
format assignment. Retained detached aliases keep their format/value snapshot.
Selected native formula token tools and compact range foundations implemented
after A8 are included; their complete M5 families remain staged.

Source transitions retain unsupported graph/signature/data-only/unknown-style
checks. Sources without a stylesheet remain unsupported for format registration.
A first explicit temporal style assignment can change numeric encoding even if
the format ID is unchanged. Untouched source ISO dates retain their fallback.
Repeated supported format edits reuse the verified model instead of rescanning
all cells. Cell values are not cloned during style assignment.

## Evidence

- ADR 0083 and ADR 0084; existing core/loaded/writer tests and strict Clippy.
- Native Rust 1.88 correctness checks and cross-platform Rust CI run 37558974933
  passed at core `84b7aa06e587ce1c4f7f5779d954bdef4a7788a4`.
- Python integration commit `82f3df9` pins that core, passes all 547 compatibility
  cases, Ruff and strict Clippy; corresponding platform package gates remain
  release-workflow requirements.
- [Complete public operation evidence](https://github.com/crabxl/crabxl-python/blob/82f3df9/benchmarks/results/alpha9-style-assignment-python.json)
  contains one warm-up and three alternating samples per engine/mode, 100,000
  cells, full value/format assertions, CPU time, output sizes, Linux VmHWM and
  sampled temporary bytes/zero remaining spools. Benchmarks ran without local
  builds/tests. The validation import follows timing/RSS collection.

| Mode | CrabXL seconds | openpyxl seconds | CrabXL RSS KiB | openpyxl RSS KiB |
| --- | ---: | ---: | ---: | ---: |
| New editable | 0.3296 | 0.7467 | 24,084 | 80,132 |
| Load/edit/save | 0.4570 | 1.0357 | 24,760 | 84,388 |
| Write-only | 0.3398 | 0.8227 | 20,448 | 32,736 |

The interrupted initial loaded run recorded one 17.206-second sample before
repeated model/source validation was removed; it is not a before/after median.
The applicable openpyxl speed goal passes on these workloads. Native competitor
style-edit comparisons and full-feature profiling remain open; these results do
not establish universal superiority or complete M5 acceptance. Temp sampling is
an observation at 5 ms intervals, not an exact peak-disk guarantee. Repeatable
Python new-book export currently clones bounded style metadata into the writer,
which remains an optimization opportunity.

## Public release verification

Rust release workflow 37559496335 succeeded at
`511d91e60df0b2cc492658f0d867362646f1da20`; remote tag
`0.1.0-alpha.9` points to that commit. All three exact-version crates are on
crates.io. A fresh registry-only Rust 1.88 consumer verified explicit General
dates and two loaded number-format save/reopen cycles.

Python release workflow 37559875709 succeeded at
`8c0ba707fc0186654c586305be0e88c124589c79`, including all five platforms and
CPython 3.11–3.15, followed by OIDC publication. PyPI exposes 25 wheels and one
source archive for `0.1.0a9`. A clean CPython 3.12 environment installed the
public manylinux 2.28 x86_64 wheel and ran the release commit's unchanged
547 compatibility cases: all passed (4.60 seconds). The freshly published
version was not yet visible through the configured/simple index cache, so the
installation used the wheel URL returned by the public PyPI version JSON.
See [release receipt](alpha9-release.json) for artifact identities.

## Closure

A9 completes its usable style-assignment scope. M4 graph dependencies, complete
M5/A10–A19, M6 and M7 remain open. The Rust MSRV/package and Python five-platform publication gates passed,
and fresh public installations were verified.
