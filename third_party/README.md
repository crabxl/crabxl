# Third-party code

CrabXL adapts code from these MIT-licensed projects. None of them is a runtime
dependency. Adapted source files keep the original copyright notice. The
licenses are in [licenses/](licenses/).

| Project | Used for | License file |
|---|---|---|
| [calamine](https://github.com/tafia/calamine) | Package discovery, cell and value parsing, date and number-format detection. | `licenses/calamine-MIT.txt` |
| [rust_xlsxwriter](https://github.com/jmcnamara/rust_xlsxwriter) | XML layouts for cells, formulas, styles, views and printing; date conversion; sequential spooling and packaging. | `licenses/rust_xlsxwriter-MIT.txt` |
| [quick-xml](https://github.com/tafia/quick-xml) | Namespace event ownership in the XML reader. | `licenses/quick-xml-MIT.txt` |
| [umya-spreadsheet](https://github.com/MathNya/umya-spreadsheet) | Rich text runs, fonts and colors. | `licenses/umya-spreadsheet-MIT.txt` |

[ports.json](ports.json) lists each adapted symbol: its source file and pinned
commit, its destination and the changes. Pinned revisions are also in
[internal/upstream-sources.md](../internal/upstream-sources.md).

openpyxl 3.1.5 is the compatibility reference. Its generated default theme data
is used in `crabxl-xlsx/src/default_theme.rs` (`licenses/openpyxl-MIT.txt`).
Code and tests copied from openpyxl are in the [Python package](https://github.com/crabxl/crabxl-python/tree/main/third_party).
