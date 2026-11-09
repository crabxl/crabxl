# Roadmap

A12 completed openpyxl 3.1.5 capability. One stage remains before the beta.

## A13: performance and memory

Goal: meet the hard requirements in [AGENTS.md](../AGENTS.md) on every
benchmark scenario.

| Scenario | Faster than | Lower peak memory than |
|---|---|---|
| Read | openpyxl, calamine | openpyxl, calamine |
| Write | openpyxl, rust_xlsxwriter | openpyxl, rust_xlsxwriter |
| Edit (load, change, save) | openpyxl, umya-spreadsheet | openpyxl, umya-spreadsheet |

Work:

1. Run the benchmark suite (`benchmarks/README.md`) and record the baseline.
2. Profile the slowest and largest scenarios. Fix the hot paths in the core.
3. Run the Python suite in crabxl-python against openpyxl, python-calamine and
   XlsxWriter. Fix binding conversion costs.
4. Repeat until every scenario meets its target, or record why a target is not
   reachable.
5. Validate produced files in Excel desktop on Windows and macOS.

Rules:

- Do not change behavior. The Rust tests, the Python tests and the openpyxl
  suite check must pass after every change.
- Commit the final benchmark results only, not one report per change.

## Beta

Release `0.1.0-beta.1` of the core and the Python package after A13.
