# ADR 0022: Composable managed read policies

## Decision

WorkbookReader::read_with_policy_options accepts the same ReadOptions as direct streams. Sampling, successful materialization and reopened fallback preserve projection, rich text, cache-only, date, formula and cell-metadata choices. The existing read_with_policy remains a default-options convenience method.

Compatible projection retains shared masters before and within the requested row interval, including masters outside selected columns needed by selected followers. It does not retain unrelated masters after the final requested row. ValidateGroups explicitly retains and checks the full source groups. Both native modes still consume the complete XML and CRC; omitted cells are not materialized. Excluded-cell errors retain their actual part/address context.

## Verification

Tests cover cache/formula projections through scan and repeated-access strategies, excluded masters, rich-string upgrades, raw serial date projection and compatible versus strict tail retention. Workspace tests, formatting and Clippy pass. See benchmarks/m2-policy-options.md for measured output validation and the reference tail-validation difference.

This composes existing policies; it does not complete owned-bank editing or M2/M4/M7 acceptance.
