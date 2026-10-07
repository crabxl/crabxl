# Component style derivation

Status: native and Python candidate implemented; A10 publication pending acceptance.

`StyleComponent` replaces one font, fill, border, alignment or protection component.
Workbook banks and sequential writers derive workbook-local format IDs under
existing aggregate allowances. The registry checks the changed component and
uses collision-checked shared tables and borrowed format keys. Unrelated font
names, gradient vectors and border payloads remain shared. Repeated assignments
reuse the same formats and do not grow the registry's retained-byte ledger.
Appearance assignment preserves temporal encoding preferences; explicit full-style
or number-format assignment retains A9 serial semantics. Application flags are
set only for the changed component; number-format IDs,
base links and unrelated overrides retain their identity.

Loaded workbooks use the same derivation and guarded canonical save path as A9
number formats. Signed packages, unknown style sections, data-only loading,
missing source stylesheets and affected unmodeled source graphs still reject.
Valid interned records remain reusable if a later cell update fails. This is
component support, not named-style/theme or advanced source-graph closure.

Python public component values and immutable cell proxies are thin adapter
objects. Reusable native component snapshots avoid repeatedly decoding Python
attribute dictionaries. Cache ownership is per caller-owned value, with one
current snapshot; mutations and nested color/side/gradient-stop changes invalidate
it. There is no global unbounded appearance cache or shadow workbook engine.
Write-only directives are decoded before row commit and participate in the
existing row allowance; the canonical writer owns format IDs and deduplication.
Read-only getters borrow the reader catalog and return small appearance values.

Existing tests cover repeated loaded saves, all component kinds, registry ledger
accounting, duplicate reuse, invalid references/components, tight allowances,
shared large unrelated payloads, immutable proxies and nested cache updates.
Full compatibility cases remain 547. The complete public benchmark adds all
five components to the existing owned/load-edit-save/write-only workload and
checks every saved value and appearance after measurement. Initial dictionary
conversion measurements failed the openpyxl speed gate. Reusable native snapshots
and removing repeated per-assignment Python imports/type-table construction
passed the complete three-mode candidate gate; the final pinned release evidence
is recorded in the A10 acceptance document before publication.

The existing typed component layouts retain their recorded umya provenance.
This registry integration and Python conversion are original project code;
working shared model and XLSX codecs are reused rather than importing a second
spreadsheet engine.
