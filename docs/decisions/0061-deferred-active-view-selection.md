# ADR 0061: Deferred signed active-view selection

Keep strict visible stable-ID selection and deferred display-view selection
distinct. Compatibility callers may request a negative index, a hidden target or
an out-of-range view. The canonical core resolves relative indexes and computes
the serialized active-tab attribute; a binding must not rebuild this policy.

`ActiveViewSelection` carries the optional serialized index and the requested
index after normalization. A visible target retains its signed index. Otherwise
use the reference's slice of the visible-sheet list; when no candidate remains,
omit `activeTab` while retaining the hidden/unselected requested view. This can
differ from the default view chosen when the emitted file is subsequently read.
The policy accepts signed 64-bit indexes and does not claim arbitrary-size
Python integer compatibility.

The reader resolves original negative indexes and retains their signed value.
The loaded bank now represents originally unselected views instead of silently
selecting its first placeholder. The original-package editor and writer expose
deferred view APIs. Loaded visibility and view controls can commit together after
one metadata/policy validation and prospective aggregate reservation. No cell
models are required; failed metadata edits leave both controls unchanged.

Existing loaded and owned-export workflows batch the public two/three-sheet
matrix for `-3`, `-1`, `1` and `10`, including a hidden middle sheet, repeated saves,
original view loading, original part preservation and failed metadata guards.
The expected matrix was obtained by public openpyxl 3.1.5 calls and ZIP metadata
inspection, not implementation reads. Existing strict selection and visibility
tests remain intact. No new small test functions are introduced.

All-hidden compatible output reports a typed `NoVisibleSheet` error; a single
hidden worksheet reports invalid data, matching the reference's distinct public
error categories. Default strict core writer behavior stays separate. Python
exposure is the next checkpoint, and M4 stage acceptance remains open.

Native release time/RSS/output evidence is in
[the deferred-view report](../../benchmarks/alpha7-active-views.md). These runs
verify complete saved values while retaining zero materialized cells.
