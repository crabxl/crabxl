# Literal calendar identity and source-serial rounding

## Decision

The canonical date value keeps a private source representation: imported serial, literal calendar datetime, clock time or elapsed duration. Each has one interpretation and no language-runtime object. The serial and workbook epoch remain available for XLSX output and raw retention. Literal calendar/time/duration constructors retain microseconds; original calendar days disambiguate equal early Windows serials when converted to another epoch.

`to_datetime`, `to_time` and `to_duration` convert imported serials using the pinned baseline's loaded millisecond behavior, and return literal values at their original precision. Language adapters call these core conversions rather than reinterpreting serials. A literal microsecond value saved as a numeric XLSX cell can legitimately load at millisecond precision; that is reference-compatible file behavior, not a claim of lossless serial encoding.

Literal elapsed constructors accept normalized day/second/microsecond components across the reference duration range without an intermediate floating-point Python reconstruction. Storage remains boxed within compact `CellValue`; heap accounting uses the actual enlarged core payload size. Per-row retention increases by the date payload size, not by file row count. Full loaded model memory still scales with materialized values.

## Boundaries

ISO date/time/duration codecs, date-only literal values, original-package date assignment and aggregate loaded catalog accounting remain staged. An imported fictitious Windows leap day still rejects epoch conversion. Gregorian literals avoid that ambiguity by converting the original calendar value directly. This does not implement formula calculation or timezone-aware Excel values.
