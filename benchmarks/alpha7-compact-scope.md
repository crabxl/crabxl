# Compact checked namespace classification

Base core `31cfbee25dc5715088468451115ab888e28ac57a` versus the compact-enum
candidate, identical worker sources/lockfile/Rust 1.99/unified dependency
features. Fresh prior and candidate binaries/examples are preserved and hashed.
Linux x86-64, two-CPU quota; one warmup and three rotating serial release runs
at each scale. Generation, compilation, tests and layout probing never overlap
timings. Every value/count/coordinate assertion and temporary cleanup is checked.

At two million numeric cells, median stream time is 0.9978 to 0.9680 seconds
(about 3% lower), retained model time 1.1449 to 1.1492 (essentially unchanged).
Candidate model peak RSS is 69,132 KiB versus calamine's 106,196. Calamine stream
and model times are 0.4361 and 0.5275 seconds: both speed targets remain unmet.
Unchanged calamine code varies 2–4% between binaries/runs, so these small effects
must not be generalized into a strong throughput claim.

At one million cells, stream seconds before/after: repeated RAM 0.6236/0.6411,
repeated disk 0.6219/0.6291, unique RAM 1.2226/1.1633, unique disk 1.7054/1.6920,
styled 0.6821/0.6641. Retained-model seconds: repeated RAM 0.7283/0.7411,
repeated disk 0.7285/0.7187, unique RAM 1.3541/1.3528, unique disk 1.9888/1.9151,
styled 0.7214/0.7297. Repeated text and styled models do not improve.

Retained repeated/unique RAM models remain about 67/220 MiB versus calamine's
157/302 MiB on these inputs. Candidate temporary disk peaks remain 16,128 bytes
for repeated text and 126,000,000 for unique text, with cleanup verified.
Moving storage to disk is still a tradeoff; parser/frame compactness does not
close speed gaps or eliminate temporary I/O. Managed allowances and unequal
retained editing capabilities stay visible in raw reports.

The representation removes invalid scope/URI combinations and reduces private
snapshot/Result layout, as documented in ADR 0071. Full workspace tests, strict
Clippy and all 91 Rust 1.88 streaming tests pass. No new implementation-mirroring
tests or timing thresholds are added.

All sizes and samples:
[numeric](results/alpha7-compact-scope-numeric.json),
[streams](results/alpha7-compact-scope-streams.json),
[models](results/alpha7-compact-scope-models.json).
