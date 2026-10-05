# Declaration-only namespace stack measurements

Prior core `b6277d0` and the candidate use identical release workers, lockfile,
Rust 1.99 and dependency features. One warmup and three rotating serial samples
verify every numeric value; text/style probes verify their corresponding values
and metadata. Builds/tests/profiling do not overlap measurement. Binary/input
hashes, CPU, RSS and sampled working files are retained in the raw reports.

At two million numeric cells across two sheets:

| Read operation | Prior seconds / RSS KiB | Candidate seconds / RSS KiB | Calamine seconds / RSS KiB |
| --- | --- | --- | --- |
| Stream complete values | 1.1665 / 5,060 | 1.1541 / 5,008 | 0.4014 / 4,560 |
| Retain both models/ranges | 1.3266 / 68,932 | 1.2332 / 69,072 | 0.5112 / 106,080 |

Full-model median time falls about 7%; streaming changes about 1%. Numeric model
RSS remains lower than calamine, while both speed targets remain unmet.
The reference ranges are not editable and do not retain a source package.

At one million text/styled cells, stream median seconds (prior to candidate):
repeated RAM 0.7595 to 0.7511, repeated disk 0.7391 to 0.7122, unique RAM
1.4069 to 1.4182, unique disk 2.0371 to 1.9203 and mixed styles 0.7710 to
0.7467. Unique RAM gains are not established; do not generalize the modest
changes to a universal speed win. Peak RSS remains about 139 MiB for unique
RAM SST and roughly 2 MiB for the other probes. Sampled SST disk peaks remain
16,128/126,000,000 bytes and cleanup passes.

The optimized namespace stack retains scope/depth/prefix validation;
[ADR 0066](../docs/decisions/0066-declaration-only-namespace-stack.md) explains
the separate counters. Workspace tests, strict Clippy and Rust 1.88 streaming
tests pass. Raw reports:
[numeric](results/alpha7-declaration-numeric.json),
[text/styles](results/alpha7-declaration-streams.json).
