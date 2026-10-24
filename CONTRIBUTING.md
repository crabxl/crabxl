# Contributing to CrabXL

Thank you for your help. This guide tells you how the repository is organized,
how to build and test it, and what a pull request must include.

CrabXL is alpha software. A bug report with a small file that shows the problem
is as valuable as code.

## Ways to contribute

- **Report a bug.** Use the [issue forms](https://github.com/crabxl/crabxl/issues/new/choose).
  Give the CrabXL version and the smallest `.xlsx` file or code that shows the
  problem. Remove confidential data first.
- **Report a difference from openpyxl.** CrabXL targets the public capabilities
  of openpyxl 3.1.5. Tell us the openpyxl call or behavior and how CrabXL is
  different. Report Python API problems in
  [crabxl-python](https://github.com/crabxl/crabxl-python/issues).
- **Improve the documentation.** User documentation is in [docs](docs). The
  documentation site is built from it by
  [crabxl-docs](https://github.com/crabxl/crabxl-docs).
- **Contribute code.** For a change that is larger than a small fix, open an
  issue first. Agree on the design before you start the work.

Do not report security problems in public issues. Read [SECURITY.md](SECURITY.md).

## Repository layout

| Path | Contents |
|---|---|
| `crates/crabxl` | Public facade crate. Users depend only on this crate. |
| `crates/crabxl-core` | Format-independent models: values, addresses, styles, formulas, worksheets and memory accounting. |
| `crates/crabxl-xlsx` | XLSX package I/O: streaming reader, sequential writer, editor and loaded workbook. |
| `crates/crabxl-bench` | Benchmark drivers and fixture generators. Not published. |
| `crates/crabxl/examples` | Usage examples. Start with [`quickstart.rs`](crates/crabxl/examples/quickstart.rs). |
| `docs/` | Public user documentation. |
| `internal/` | Maintainer records: [status](internal/STATUS.md), roadmap, architecture, design decisions and release evidence. |
| `benchmarks/` | Benchmark scripts. Reports are in `reports/`. Raw data is in `results/`. |
| `third_party/` | Provenance and licenses of code ported from other projects. |
| `tools/` | Release and inventory scripts. |

The Python package is in a separate repository,
[crabxl-python](https://github.com/crabxl/crabxl-python). It uses a pinned
revision of this repository.

## Development setup

Install [rustup](https://rustup.rs/). The `rust-toolchain.toml` file selects
the development toolchain (Rust 1.99) with rustfmt and Clippy.

```sh
git clone https://github.com/crabxl/crabxl.git
cd crabxl
cargo build --workspace
cargo run -p crabxl --example quickstart
```

The library supports an older Rust version (the MSRV), which is **1.88**. The
`rust-version` field in `Cargo.toml` declares it. Do not use language or
standard library features that are newer than the MSRV.

## Checks

CI runs these checks. Run them before you open a pull request:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --no-deps --locked
```

CI also runs the tests with the MSRV on Linux, Windows and macOS, and with the
native zlib backend. To check the MSRV on your computer:

```sh
rustup toolchain install 1.88.0
cargo +1.88.0 test --workspace --locked
cargo +1.88.0 test -p crabxl -p crabxl-xlsx --no-default-features --features deflate-zlib --locked
```

## Code rules

- **Do not panic on user input.** Return the typed `Error` with context (part,
  sheet or cell). Clippy warns on `unwrap` and `expect`, and CI rejects
  warnings.
- **Do not use `unsafe`.** The workspace forbids it.
- **Document public items.** CI rejects missing rustdoc comments.
- **Keep the layers separate.** `crabxl-core` does not know about XLSX. Types
  from other libraries do not appear in the public API.
- **Keep streaming bounded.** A streaming reader must not load a full
  worksheet. Limit buffers by bytes and by row or cell count.
- **Use zero-based coordinates.** Convert A1 references explicitly.
- **Fail explicitly.** If a feature is not supported, return an error. Do not
  drop data on save.
- **Write in English** in code, comments, documentation and commit messages.

### Tests and fixtures

- Put unit tests next to the code. Put integration tests in `crates/*/tests`.
- Use small, generated fixtures. Give each committed fixture a source or a
  generation instruction.
- Do not put timing limits in tests. Measure performance in `benchmarks/`.

### Performance changes

If you claim a speed or memory improvement, include the measurements. Give the
time, peak memory and temporary disk use of a release build, and the commands
that you used. [benchmarks/README.md](benchmarks/README.md) describes the
method.

### Code from other projects

Code that you adapt from another project must have a license that is
compatible with MIT. Record it in [third_party/ports.json](third_party/ports.json):
the repository, the pinned commit, the source files and the license. Keep the
original copyright notices.

### Design decisions

Record a significant design decision as an Architecture Decision Record (ADR)
in [internal/decisions](internal/decisions). Write an ADR when you change a
public contract, a memory strategy or a file-format policy. When a change adds
or completes a capability, update
[internal/features.json](internal/features.json).

## Documentation style

User documentation follows the principles of ASD-STE100 (Simplified Technical
English):

- Write short sentences: 20 words or fewer in an instruction, 25 or fewer in a
  description.
- Give one instruction in each sentence.
- Use the active voice.
- Use one word for one meaning. Use the terms in the project glossary in
  [crabxl-docs](https://github.com/crabxl/crabxl-docs).

The page format is defined in crabxl-docs. CI checks the style with Vale.

## Commits and pull requests

- Use [Conventional Commits](https://www.conventionalcommits.org/):
  `<type>(<scope>): <description>`. For example:
  `fix(reader): reject truncated shared-string records`. Common types are
  `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci` and `chore`.
- Put one coherent change in each commit. Related code, tests and documentation
  can be in the same commit. Put unrelated changes in separate commits.
- Describe the result, not the work process. Do not write "fix stuff" or
  "address review".
- Complete the pull request template. Link the related issue. Make sure that CI
  passes.
- If a change affects the Python package, tell how you checked it with
  [crabxl-python](https://github.com/crabxl/crabxl-python).

## Releases

Maintainers make releases with the **Release Rust alpha** workflow. You do not
need to change version numbers. [internal/releases.md](internal/releases.md)
describes the process.

## License

You agree that your contributions are licensed under the [MIT License](LICENSE).
