# Input limits

This note describes how CrabXL protects itself from hostile files, and why.

## Goals

1. Open every file Excel can write, with no setting changed.
2. Stop files built to exhaust memory or CPU: ZIP bombs, deep XML nesting and
   huge single XML tokens.
3. Write whatever the program gives, within the rules of the XLSX format.
   Data the program writes itself is not an attack.
4. When a limit stops a file, the error names the limit, the value found and
   how to raise it.

## What other libraries do

| Library | Guard | Default |
|---|---|---|
| Apache POI | ratio of compressed to uncompressed entry size | 1%, so 100 times expansion |
| Apache POI | uncompressed size of one entry | 4 GB before 5.1, removed since |
| Apache POI | extracted text | about 10 million characters |
| Apache POI | DOCTYPE in XML | rejected since 5.1 |
| excelize | total uncompressed size | 16 GB |
| excelize | one XML part in memory | 16 MB, then a temporary file |
| libxml2 | one text node | 10,000,000 bytes |
| libxml2 | nesting depth | 256 |
| Open XML SDK | characters in one part | no limit unless set |
| calamine | cells | 100 million |
| openpyxl | none beyond optional defusedxml | |

The common pattern: guard the shapes that real files never have, set the
thresholds far above real files, and spill to disk instead of failing where
data is merely large.

## Design

Two separate settings:

- **`ResourceLimits`** guards input files. It applies when reading, loading
  and editing files, never to data the program writes.
- **`MemoryPolicy`** decides how much memory CrabXL uses. It is `Auto` by
  default. Running out of it names the memory setting, not a limit.

`ResourceLimits` has six fields:

| Field | Default | `strict()` | Stops |
|---|---|---|---|
| `max_expansion_ratio` | 100 | 100 | ZIP bombs: an entry larger than 1 MiB that expands more than this many times |
| `max_total_uncompressed_bytes` | 16 GiB | 1 GiB | archives whose entries add up to more than this |
| `max_part_bytes` | 4 GiB | 256 MiB | one uncompressed entry larger than this |
| `max_archive_entries` | 1,000,000 | 10,000 | archives with absurd entry counts |
| `max_xml_depth` | 256 | 256 | deeply nested XML |
| `max_xml_event_bytes` | 10 MiB | 1 MiB | one XML text, tag or attribute larger than this |

XML document types are always rejected, so entity expansion attacks cannot
happen. The six fields are the documented surface. The older fields for cell
text, styles, shared formulas, sheets, rows and metadata stay as hidden fields
with no limit by default; the format bounds them already, or the guards above
do, or memory does. `strict()` sets them to the old conservative values.

Three presets:

- `ResourceLimits::default()` accepts every file Excel writes.
- `ResourceLimits::strict()` is for services that read uploads from strangers.
- `ResourceLimits::unlimited()` turns every guard off, for files you trust.

Errors have kind `LimitExceeded`. The message names the part, the field, the
value found and the maximum, and suggests `unlimited()` for trusted files.

Python takes `limits="strict"`, `limits="unlimited"` or a
`ResourceLimits(...)` in `load_workbook`, and the `CRABXL_LIMITS`
environment variable sets the default for code that cannot pass arguments,
such as pandas.

## Implementation

`ResourceLimits` keeps its hidden fields, so existing code and tests that set
a tiny limit keep working. `unlimited()` sets every guard to the type's
maximum; `default()` starts from it and sets the six guards.

The expansion ratio and the part size are checked once, when the archive
opens, from the declared sizes of each entry. Every XML reader already stops
at the declared size of its entry, so a lying header cannot get past it.

The memory working reserve caps each buffer term at a typical size, since an
unlimited guard would otherwise reserve the whole budget.
