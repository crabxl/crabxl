# ADR 0080: Default model cardinality follows coordinates and byte budgets

Status: implemented and integrated into Python; A8 release acceptance remains open.

The archive byte defaults were removed in A6, but ordinary editable models still
defaulted to ten million physical cells and 1,024 worksheets. A one-million-row
by 41-column sheet exceeds the cell count independently of available memory.
These defaults impose arbitrary model cardinality restrictions.

EditLimits now defaults to the worksheet's legal coordinate space, saturating
for narrower usize platforms. WorkbookLimits defaults aggregate physical cells
and sheet slots to usize::MAX. Address/name/identity validation, retained and
structural-work byte budgets, fallible slot reservations and explicitly supplied
smaller cell/sheet counts remain enforced. This changes defaults rather than
disabling caller-configured resource controls.

The existing sheet-identity workflow now creates 1,025 additional worksheets and
verifies stable active selection. Existing finite count/byte/work failure tests
retain their assertions. Rust 1.88 core and loaded-workbook tests, Clippy and
documentation checks pass without new test functions or huge generated fixtures.

The actual NYC million-row file is unavailable here. Removing its fixed count
obstacle does not verify the file's elapsed time, RSS or successful complete
materialization under a particular memory policy. Packed model charging remains
conservative at 256 bytes per physical cell plus owned payload; its capacity-aware
accounting is still tracked separately.

Python commit `a4657794982eec1f81554b92ddf3ef903a8c7814` pins this exact core
revision. Its freshly built CPython 3.12 wheel passes all 547 compatibility cases,
Ruff formatting/checks and strict Clippy. Core GitHub Rust run `37405637664`
passes the configured platform/MSRV/quality matrix.
