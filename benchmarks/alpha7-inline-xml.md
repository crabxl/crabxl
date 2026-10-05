# Inline shared XML events in codec loops

Independent CPU samples still identify shared XML events and cell decoding as
the main numeric-read costs. The candidate forces the existing shared event
method to inline into its codec callers; parsing, namespace, byte/depth/root
and error semantics are unchanged. No alternate parser or unchecked mode is
introduced.

Prior core `9b5311b` and the candidate use the same release worker source,
lockfile, Rust 1.99 and unified dependency features. One warmup and three
rotating serial samples at each size verify all values/coordinates. Builds,
tests and independent CPU sampling do not overlap timings. Hashes, wall/CPU,
RSS and sampled temporary bytes are retained in the raw reports.

At two million numeric cells across two sheets:

| Operation | Prior seconds / RSS KiB | Inlined seconds / RSS KiB | Calamine seconds / RSS KiB |
| --- | --- | --- | --- |
| Complete row stream | 1.1602 / 4,788 | 0.9955 / 5,140 | 0.4225 / 4,372 |
| Retain both models/ranges | 1.3693 / 68,916 | 1.1805 / 68,884 | 0.5042 / 106,020 |

Both CrabXL numeric modes improve about 14%. Retained-model RSS remains about
35% below calamine, while **both speed targets remain unmet**. Calamine returns
noneditable ranges and does not retain an editable source package.

At one million text/styled cells, prior/candidate stream seconds: repeated RAM
0.7584/0.7105, repeated disk 0.7154/0.6895, unique RAM 1.3707/1.3103, unique
disk 1.8252/1.9086 and mixed styles 0.7908/0.7232. Unique disk regresses about
5%; the other probes improve modestly. Streaming baseline RSS increases by
hundreds of KiB in some cases; unique RAM SST remains roughly 139 MiB.
Disk peaks stay 16,128/126,000,000 bytes and cleanup passes.

Inlining increases release code size: the combined comparison executable grows
from 10,879,968 to 11,179,312 bytes; the shared-text example grows from 1,548,464
to 1,791,328 bytes. Larger binaries and compiler work are explicit tradeoffs;
do not generalize this hint to unrelated code without measurement.

Workspace correctness and strict Clippy pass. Existing resource/namespace/error
tests retain their assertions. Raw evidence includes both scales:
[numeric](results/alpha7-inline-xml-numeric.json),
[text/styles](results/alpha7-inline-xml-streams.json).
Python wheels require a separately verified published core pin; these native
results are not Python package or dataframe speed claims.
