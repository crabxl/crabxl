# ISO calendar values with explicit baseline semantics

## Decision

Core now distinguishes literal date-only values from calendar datetimes, clocks and durations. `from_ymd` preserves Gregorian identity; date-only raw serial construction requires integral days. `parse_iso8601` and `to_iso8601` provide runtime-independent parsing/formatting, shared by XLSX codecs and future adapters. No regex cache, runtime engine or duplicate binding serializer is needed.

The pinned public reference accepts recognized date/clock prefixes, ignores trailing text (including zone suffixes), truncates fractional seconds to three digits and supports positive elapsed hour/minute/second notation. Public probes record these behaviors rather than inferring strict ISO compliance. The compatibility parser deliberately matches them, including rejection or prefix fallback for unsupported long duration fractions. It does not introduce timezone-aware spreadsheet values. Empty ISO input is absent; decoded nonempty input is not whitespace-trimmed.

`WriteOptions.iso_dates` defaults to false. When enabled, date/calendar/clock values are encoded with cell type `d` and baseline millisecond strings, independent of workbook serial epoch. Elapsed durations remain numeric under an elapsed format. Default writer formats now include a fifth date-only record; registered IDs remain workbook-local implementation identities. ISO payload limits are checked before spooling, and malformed/over-budget rows permit valid retry.

## Evidence and remaining work

All 42 recorded public utility observations match native parsing. Generated tests inspect typed streaming/materialized values, caches, original early dates, both epochs, malformed contextual errors, byte budgets and temporary cleanup. Sequential creation/read benchmarks verify every date/time/duration kind and value, process time/RSS, output sizes and exact native logical temporary bytes through completed worksheet XML.

Themes, full imported catalog editing/normalization, shared/array/data-table formula metadata, aggregate loaded accounting and safe original-package date assignment remain required. This completes the scoped ISO/date-only codec checkpoint, not the whole M2 value/style/formula milestone. Public reference prefix permissiveness is documented compatibility behavior, not a promise of a strict ISO validator.
