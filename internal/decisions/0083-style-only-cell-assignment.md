# Native style-only assignment checkpoint

Status: implemented in the native model; unreleased. Work paused at the user's request after this checkpoint. M5 and Alpha 9 remain incomplete.

## Behavior

`Worksheet::set_style` updates an existing cell in place without cloning its value or changing its storage charge. Reapplying the same style leaves a clean worksheet clean. A missing coordinate creates a checked empty cell. Workbook editors validate style IDs against the canonical catalog before mutation; standalone worksheets retain the caller-managed style-ID contract.

Canonical workbook exports preserve the final explicit style, including General assigned to a date or duration. Direct streaming writers retain automatic temporal-format inference. Integral date serials use integer XML spelling, so a date explicitly assigned General reloads as an integer rather than a float.

## Verification

Existing tests cover a 2 MiB text value retaining its allocation, unchanged storage charges, clean/dirty behavior, invalid-ID rejection before mutation, missing-cell assignment, repeated borrowed exports and consuming exports. No new test functions were added.

Rust 1.88 core tests passed. All 44 writer tests passed with both the default compression backend and `deflate-zlib`. Strict workspace Clippy, formatting and whitespace checks passed.

The release-mode probe in [the raw measurement record](../../benchmarks/results/alpha9-style-assignment-checkpoint.json) validates every coordinate and value after one million numeric style assignments. Median assignment time was 0.0600 seconds for direct mutation and 0.2229 seconds for cloning and replacing each cell. Peak process RSS was approximately 33 MiB for both. This compares two implementations on the same working tree, not published releases, and does not establish a general read/write performance improvement.

A separate 100,000-cell creation, style-assignment and finalized-export probe took a median 0.0978 seconds, with 8,444 KiB peak process RSS including untimed read-back validation. The record contains the probe source, manifest, binary and runtime-diff hashes, sample results and timing boundaries. Temporary-disk usage was not measured.

## Remaining scope

Python exposure, loaded-model style integration, complete style coverage and the remaining M5 features still require implementation and acceptance. This checkpoint does not publish an alpha or close a milestone.
