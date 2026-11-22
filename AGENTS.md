# AGENTS.md

Notes for anyone — human or agent — working on this repository. The
`README.adoc` describes what rsjudge is and how to install it; this file records
the things that only become obvious after the build has failed a few times.

## Getting to a first successful build

### Build dependencies must be installed before `cargo build`

The build fails late and confusingly when a system dependency is missing, so
install them first.

| Dependency            | Why                                                                | Note                                                                                                                                    |
| --------------------- | ------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| `libseccomp` >= 2.5.0 | `rsjudge-runner` links it for the seccomp filter                   | Package name is `libseccomp-dev` on Debian/Ubuntu, `libseccomp-devel` on Fedora/RHEL, and comes with the headers on Arch (`libseccomp`) |
| `clang`               | `bindgen` in the `libseccomp` build script                         |                                                                                                                                         |
| `buf`                 | `rsjudge-grpc`'s build script shells out to it to list proto files | Required by the default `grpc` feature                                                                                                  |
| `protoc` >= 3.15      | `tonic-prost-build` compiles the proto files                       | Package is `protobuf-compiler` on Debian/Ubuntu                                                                                         |

Install the **latest** version of each. `libseccomp` in particular must be at
least 2.5.0, because the profile relies on syscall tables that cover `openat2`
and `faccessat2`; an older one does not fail at build time, it fails at run
time.

A missing `buf` surfaces as a build-script error inside `rsjudge-grpc` rather
than as a missing-tool message, so if the gRPC crate fails to build for no
obvious reason, check `which buf` first.

### Submodules

`schemas/test-cases-schema` is a submodule and some tests read from it:

```bash
git submodule update --init --recursive
```

`crates/rsjudge-test-cases-schema/tests/fixtures.rs` parses the reference
examples in `schemas/test-cases-schema/out/`. Without the submodule those tests
panic with a message telling you to initialise it; they do not skip silently, on
purpose.

### Build output lives in `build/`, not `target/`

`.cargo/config.toml` sets `build-dir = "build"`. Looking in `target/` for the
binary will find nothing. Both directories are gitignored.

## Running the tests

### Use `cargo nextest`

CI runs `cargo hack nextest --each-feature --workspace`. Locally:

```bash
cargo nextest run --workspace --all-features
```

### `nextest` does not run doctests, so run those separately

`rsjudge-utils` carries doctests (including deliberate `compile_fail` ones) that
`nextest` skips entirely. Run them explicitly:

```bash
cargo test --doc --workspace
```

A change that only runs `nextest` can pass while the doctests are broken.

### Cover the feature matrix

The crates have mutually independent features (`grpc`, `rest`, `amqp`,
`mimalloc`, plus each crate's own `serde`), and a change can compile under
`--all-features` while breaking a smaller set. CI checks each one:

```bash
cargo hack check --each-feature --workspace
cargo hack clippy --all-targets --each-feature --workspace
```

## Formatting and linting

### Rust formatting requires nightly

`rustfmt.toml` sets `unstable_features = true` together with
`group_imports`, `imports_granularity` and `wrap_comments`. Stable `cargo fmt`
does not error on these: it prints `Warning: can't set ... unstable features are
only available in nightly channel` and silently leaves the file formatted the
stable way, which then fails CI.

```bash
cargo +nightly fmt --all
```

### `dprint` covers the non-Rust files

TOML, JSON, Markdown, YAML and the Dockerfile are formatted by `dprint`, and its
config runs `rustfmt +nightly` as an exec plugin for `.rs` files too.

```bash
dprint check
```

Note the version pinning: `dprint` reads `dprint.json` and downloads the plugin
versions it names. A mismatch with the locally installed `dprint` shows up as
files that pass locally and fail in CI.

### Do not format the submodule

`dprint.json` excludes `/schemas/test-cases-schema`: that repository has its own
`dprint.json` and its own plugin versions. Running the root formatters across
it produces diffs that belong to a different repository and cannot be committed
from here.

## Dependency policy

### Keep `cargo deny` green

```bash
cargo deny --all-features --workspace check
```

Three things catch people out:

- **Licenses** are allow-listed in `deny.toml`. A new dependency with a license
  outside the list fails the check; add it deliberately, not as a reflex.
- **Unmaintained advisories** are checked workspace-wide
  (`unmaintained = "workspace"`), so a crate that is fine transitively but not
  acceptable for us still fails.
- **Duplicate crate versions** are reported as warnings, deliberately. They are
  not a build or security problem, and failing on them would block Dependabot
  from landing routine bumps. Do not turn `multiple-versions` into `"deny"`; if
  a duplicate is worth collapsing, upgrade the crate that pins the older one.

### Update dependencies in their own commit

Use `cargo upgrade` for the manifests and `cargo update` for the lockfile, and
keep the result in a commit of its own. A `Cargo.lock` that moves as a side
effect of an unrelated change is noise in review and is easy to miss when
something breaks later.

Dependabot opens daily bumps against `Cargo.toml`, `Cargo.lock`, the
devcontainer and GitHub Actions; let it do its job rather than hand-pinning.

## MSRV

The policy is N - 2, where N is the current stable Rust, and the number lives in
`rust-version` in `Cargo.toml`.

**Do not verify the MSRV locally.** Doing so means installing and maintaining an
older toolchain to re-run checks that CI already runs for you: the `msrv` job
pins the exact version and builds the workspace with it. Develop against stable,
and when the MSRV moves, update both `Cargo.toml` and the `channel` in the
`msrv` job of `.github/workflows/building.yml` in the same commit — the README
explicitly calls this out to maintainers, and the two drifting apart produces a
CI failure that looks like a compiler bug.

## Working on the code

### Do not commit to `main`

Work on a branch, open a pull request, and let CI run. The checks that must be
green are `check-code-style`, `lint-debug`, `lint-release`, `test`, `msrv`,
`pkg-deb`, `pkg-rpm` and CodeQL.

### Rewrite history instead of stacking fixes

A pull request is reviewed as a whole. If review or your own testing finds a
mistake in an earlier commit of the same branch, **amend or `git rebase -i` the
commit that introduced it** and force-push with `--force-with-lease`. Do not
follow up with a "fix the previous commit" commit: the PR should read as though
it was written correctly the first time, with no back-and-forth visible in its
history.

The same applies to reverting your own change: rewrite the commit that added it
rather than adding a revert commit on top.

### Commit messages

Conventional Commits, scoped by crate or area — `feat(judger):`,
`fix(runner):`, `build(deps):`, `ci:`, `docs:`, `chore:`. The repository's
history is the reference.

Reference issues in the body:

- `Fixes #N` — closes the issue when the PR merges.
- `Refs #N` / `Part of #N` — links without closing.
- Cross-repository references must be written out in full:
  `NJUPT-SAST/rsjudge-test-cases-schema#N`.

### Language

Code, comments, documentation and commit messages are in English.
`README.adoc` and `README.zh-CN.adoc` are translations of each other, so a change
to one normally needs the matching change in the other.

## The test-cases schema repository

`schemas/test-cases-schema` is a separate repository that holds the problem
package format: the TypeScript definitions (`rsjudge-schema.d.ts`), the
generator (`index.ts`) and the reference examples (`out/`). The Rust serde
implementation lives here, in `crates/rsjudge-test-cases-schema`.

### The reference examples are a contract, not fixtures to edit by hand

`out/*.{json,toml}` is generated. Change `index.ts` and regenerate:

```bash
cd schemas/test-cases-schema
bun install
bun run index.ts
```

Regeneration is deterministic; running it twice must produce byte-identical
files, and re-running it on a clean tree must be a no-op. Every generated file
ends with a newline and carries no license header of its own, so `cat` and
`diff` behave normally and regeneration cannot silently drop a header.

The Rust crate's tests parse these files, which is what keeps the two
implementations from drifting apart. If a fixture test fails, the fix is
normally in the schema repository, not in the test.

### Keep the gitlink on a commit that is on `main`

A submodule pointer must name a commit reachable from the schema repository's
`main`. That repository merges with **rebase**, so a branch commit's SHA changes
when the pull request lands: a gitlink pinned to the branch commit points at a
commit that is not an ancestor of `main` and disappears from a fresh clone.

The order that works is: merge the schema pull request first, then bump the
gitlink here to the resulting `main` commit. Cross-repository changes therefore
land as two pull requests, with this one second.

## Environment notes

- The kernel must be >= 4.5 for seccomp BPF support.
- Some examples under `crates/rsjudge-runner/examples/` exercise capabilities and
  the sandbox directly and expect privileges; they are diagnostics, not part of
  the test suite, and are not run by CI.
