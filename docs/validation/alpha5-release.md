# Alpha.5 verified publication

A5 is complete. Assigned M2 core acceptance is closed with the
[behavioral audit](alpha5-m2-acceptance.md); full M4–M7 and Python API coverage
remain separately staged. Published tags/artifacts are immutable.

| Repository | Published version | Release commit | Successful workflow |
| --- | --- | --- | --- |
| crabxl/crabxl | 0.1.0-alpha.5, all three crates | 7efa37b10c6b19757ff58dbf930f9e233b3af7d4 | [Release Rust alpha, 37294417305](https://github.com/crabxl/crabxl/actions/runs/37294417305) |
| crabxl/crabxl-python | 0.1.0a5, 25 wheels + sdist | 5523e6c734286a5658007cc47bc3ddf973cf26d8 | [Release Python alpha, 37294935553](https://github.com/crabxl/crabxl-python/actions/runs/37294935553) |

Core [platform CI](https://github.com/crabxl/crabxl/actions/runs/37294417349)
passes Linux/Windows/macOS Rust 1.88 default/native-zlib tests and latest-stable
quality checks. Python CPython 3.11–3.15 wheels pass 544 tests each across Linux
x86_64/ARM64, Windows x86_64 and macOS Intel/Apple Silicon. The five platform
jobs share Rust compilation across each platform's five ABIs; source distribution
validation and PyPI OIDC upload also succeed.

Public verification uses an independent registry-only Rust 1.88 Cargo project,
not a workspace/path/Git dependency. It runs exact large style identities, typed
theme access, compression, value readback and concurrency-aware Auto. All three
downloaded crate archive hashes match crates.io metadata, and their embedded
VCS identities match the release tag. See [registry evidence](alpha5-release.json).

A fresh CPython 3.12 environment installs only `crabxl==0.1.0a5` from PyPI.
Ordinary/write-only creation, forced RAM/Auto/disk SST reads, bounded batches,
Auto model allowance, loaded patch failure/retry, source protection, stored
rewritten worksheet and complete readback pass without openpyxl installed.
The installed native module/configuration match the downloaded public wheel;
wheel/sdist hashes and canonical source pin are verified. Python artifact hashes
and installation evidence are recorded in that repository's
`docs/validation/alpha5-release.json`.

The model allowance excludes canonical parser working reserve; maximum_bytes
is the operation cap, not a promise that the entire cap is available for retained
models. Loaded components and caller/runtime overhead remain separately managed;
these controls are not hard process RSS limits.
