# Instructions for AI agents

These rules apply to AI agents that work on this repository. Human contributors
use [CONTRIBUTING.md](CONTRIBUTING.md). The rules in CONTRIBUTING.md also apply
to agents.

Read [internal/STATUS.md](internal/STATUS.md) before you start. It tells you
the current state and the next tasks.

## Repository map

| Path | Use |
|---|---|
| `crates/crabxl` | Public facade crate. |
| `crates/crabxl-core` | Format-independent models. |
| `crates/crabxl-xlsx` | XLSX reader, writer, editor and loaded workbook. |
| `crates/crabxl-bench` | Benchmark drivers and fixture generators. Not published. |
| `docs/` | Public user documentation. The documentation site publishes this folder. |
| `internal/` | Records for maintainers and agents. Not published. |
| `benchmarks/` | Benchmark scripts. Reports are in `reports/`, raw data is in `results/`. |
| `third_party/` | Provenance and licenses of ported code. |

The Python binding is in [crabxl-python](https://github.com/crabxl/crabxl-python).
The documentation site is in [crabxl-docs](https://github.com/crabxl/crabxl-docs).

## Language and identity

- Write all repository content in English.
- Use this Git identity for author and committer, unless the user gives another
  identity: `cmostw <300344883+cmostw@users.noreply.github.com>`. Set it in the
  repository configuration. Do not use the global identity of the environment.

## Writing rules

These rules apply to all text: documentation, internal records, comments, error
messages and commit messages. They follow the principles of ASD-STE100
(Simplified Technical English). The STE dictionary is not used. Use the project
glossary in crabxl-docs instead.

- Write short sentences. Use a maximum of 20 words in an instruction and 25
  words in a description.
- Give one instruction in each sentence.
- Use the active voice and the imperative for instructions.
- Use one word for one meaning. Use the glossary term. Do not use synonyms.
- Do not put more than three nouns in a sequence.
- Write for a reader who does not know the history of the project. Do not use
  milestone or release codes (for example "M4" or "A11") without a definition.
- Do not record conversations. Do not write "the user approved" or "the user
  asked". Record the decision and its reason.

## Where to write records

- `docs/` is public. Put only user documentation there. Follow the content
  specification in crabxl-docs.
- `internal/STATUS.md` is the single current status. Replace its content when
  the status changes. Do not append a history log.
- `internal/roadmap.md` is the long-term plan. Edit it in place. Remove items
  that are done or no longer planned.
- `internal/decisions/` contains Architecture Decision Records (ADRs). Write an
  ADR only for a decision that changes a public contract, a resource strategy
  or a file-format policy. Do not write an ADR for each feature.
- `internal/validation/` contains release acceptance evidence.
- `internal/archive/` contains plans that are complete or replaced. Do not edit
  archived files.
- `benchmarks/reports/` contains benchmark reports. `benchmarks/results/`
  contains raw data.

## Product rules

- Target the public capabilities and observable behavior of openpyxl 3.1.5.
  Rust names and syntax can be different. Do not remove a capability or change
  value, formula, date, editing or preservation semantics.
- Track read, create, edit and preserve support separately. Preservation alone
  is not read or edit support.
- Keep all baseline features in the roadmap. Do not reduce the scope to the
  capabilities of upstream Rust libraries.
- The Rust core is the canonical implementation. Keep it independent of
  Python and other runtimes.
- Follow the [binding contract](internal/binding-contract.md) for binding
  priorities and API principles.
- Read [internal/architecture.md](internal/architecture.md) before you change an
  affected area. Update it when the architecture changes.
- Use openpyxl documentation, public behavior and fixtures as references. Do
  not read the openpyxl implementation broadly. Reading the diff of a pending
  openpyxl merge request is permitted to review a known defect.

## Memory and performance

- CrabXL must be faster than openpyxl. Being faster than calamine and
  rust_xlsxwriter, and using less peak memory than all three, is a goal.
- Correctness, memory and performance are joint requirements. Do not claim an
  improvement without a measurement.
- A streaming reader must not load a full worksheet, decoded range or worksheet
  XML. Full loading is a separate, explicit mode.
- Limit buffers and batches by bytes and by row or cell count.
- Do not add unbounded caches, full-file copies or repeated per-cell metadata.
- Moving data from memory to disk is a tradeoff. Measure temporary disk use, I/O
  and cleanup.
- For parser, writer, allocation and streaming changes, run release benchmarks.
  Record time, peak memory, temporary storage and checksums.
- Keep timing measurements out of ordinary tests.

## Porting code

- Pin the upstream commit before you port code. Record the repository, commit,
  source files, destination, license and behavior changes in
  `third_party/ports.json`.
- Port selected modules into the existing architecture. Do not wrap or
  re-export calamine or rust_xlsxwriter as the core.
- Keep upstream types out of the public API.
- Keep the required copyright notices.

## Code rules

- `crabxl-core` owns the shared models. `crabxl-xlsx` owns format codecs and
  I/O.
- Use zero-based coordinates internally. Convert A1 references explicitly.
- Return typed errors with part, cell or source context. Do not panic on user
  input.
- Do not use `unsafe`. Do not add a C ABI for a possible future binding.
- Give each fixture a source or a generation instruction.
- Deliver changes that compile and can be reviewed. Do not leave imported code
  without integration.

## Commits

- Read and follow [Coherent Commits](.agents/skills/commit/SKILL.md) before you
  make a commit.
- Use Conventional Commits: `<type>(<scope>): <description>`. Describe the
  result, not the work process.
- Put one coherent intent in each commit.
- Before you commit, review the staged changes, check the identity and run the
  applicable checks.
- Push each commit immediately after you make it. Make sure that the remote
  has the commit before you make the next commit.
- Do not force-push unless the user authorizes it.
- If a push fails, keep the local commit, report the problem and try again.
