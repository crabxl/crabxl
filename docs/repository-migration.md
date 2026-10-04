# Repository split

CrabXL core lives at https://github.com/crabxl/crabxl. The openpyxl-compatible Python adapter lives at https://github.com/crabxl/crabxl-python and imports as crabxl.

The core history preserves the original monorepo through 5c80ecaa080e3605cff98eb8c80721d85aa4af00. The adapter history is extracted from bindings/python with git-filter-repo; its original assertions and author/committer identities are retained. Historical source paths and names describe their recorded versions.

Rust crates are crabxl, crabxl-core and crabxl-xlsx. Python is not a member or excluded child of the Rust workspace. Core remains the only canonical implementation; adapters select compatibility calls and own runtime resources. The Python repository pins the core Git revision and verifies selected original reference tests independently.

Pending inline-literal work was preserved separately before splitting and is not included in this migration. M4-M7 remain incomplete; published checkpoint evidence is retained.
