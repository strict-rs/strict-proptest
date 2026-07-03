# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

Scope: `proptest/src/test_runner/failure_persistence/` — pluggable storage for minimized failing seeds so a failure replays deterministically before any novel cases on the next run. The backend is selected through `Config::failure_persistence` and driven by the runner; for the rest of the driver see the parent `test_runner/AGENTS.md`, and for shared conventions the workspace-root `AGENTS.md`.

## The `FailurePersistence` trait (`mod.rs`)

`FailurePersistence: Send + Sync + fmt::Debug` is the backend abstraction. The live API is the `*2`-suffixed pair:

- `load_persisted_failures2(source_file: Option<&'static str>) -> Vec<PersistedSeed>` — the seeds to replay for a given source file.
- `save_persisted_failure2(&mut self, source_file, seed: PersistedSeed, shrunken_value: &dyn fmt::Debug)` — record one new failing seed; `shrunken_value` is the minimized value, used only for the human-readable comment.

The un-suffixed `load_persisted_failures` / `save_persisted_failure` are `#[deprecated]`, `panic!` by default, and only speak legacy 16-byte XorShift seeds (`[u8; 16]`) — they predate the multi-algorithm `Seed`. The default `*2` impls bridge to them (load wraps each `[u8; 16]` as `Seed::XorShift`; save delegates only when the seed is XorShift), so a pre-existing backend that overrode only the deprecated methods keeps working. New backends override the `*2` methods and leave the deprecated ones alone.

Trait-object plumbing — these three have no default and every impl must provide them:

- `box_clone(&self) -> Box<dyn FailurePersistence>` backs the hand-written `impl Clone for Box<dyn FailurePersistence>`.
- `eq(&self, other: &dyn FailurePersistence) -> bool` backs `impl PartialEq for dyn FailurePersistence`. Every impl does `other.as_any().downcast_ref::<Self>()` then compares, so two different concrete backends are never equal.
- `as_any(&self) -> &dyn Any` exists solely to enable that downcast.

`Config` (parent module) stores the choice as `Option<Box<dyn FailurePersistence>>`; the runner calls `load_persisted_failures2` before generating novel cases and `save_persisted_failure2` when a case fails. `None` means "persist nothing" and is the practical off switch.

## `PersistedSeed` and the on-disk wire format

`PersistedSeed(pub(crate) Seed)` is the opaque public seed; it wraps the crate-internal `Seed` enum defined in `../rng.rs`. Its `Display` / `FromStr` *are* the wire format and delegate straight to `Seed::to_persistence()` / `Seed::from_persistence()` (`FromStr::Err = ()`, so an unparsable string is just `Err(())`).

A line encodes one seed as a leading algorithm key (`RngAlgorithm::persistence_key`) followed by whitespace-separated data:

- `xs` (XorShift) — four decimal `u32` dwords (the 16-byte seed read as little-endian `u32`s), e.g. `xs 1820944860 846518628 1859875405 4214131038`. This is the only human-decimal form and is the legacy encoding (XorShift was the default through Proptest 0.9.0).
- `cc` (ChaCha) — one lowercase base16 string of 32 bytes (64 hex chars). ChaCha is the current default algorithm, so this is what new fixtures use.
- `rc` (Recorder) — same shape as `cc`: base16 of 32 bytes.
- `pt` (PassThrough) — base16 of a variable-length byte buffer; may be empty (key only).

Parsing is `trim` + whitespace-split with a strict part-count per algorithm; an unknown key or a wrong count yields `None`.

On-disk (`file.rs` only), each seed line is `<seed> # shrinks to <Debug>` — the persisted seed, then a `#` comment carrying the minimized value's `Debug`, with any `\n` / `\r` in that debug rewritten to spaces so the record stays a single line. On read, `parse_seed_line` splits each line at the first `#` (`split_once`), skips blank lines, and emits a warning diagnostic for (then ignores) any non-blank line that fails to parse — which is also what makes a torn line from a concurrent writer harmless.

## Backends

`FileFailurePersistence` (`file.rs`, `std`-only) — the default backend. A `#[non_exhaustive]`, `Copy` enum of path strategies (the on-disk path is derived from the source file under test):

- `SourceParallel(&str)` — the `Default` (`SourceParallel("proptest-regressions")`). Walk up from the source file's directory until one containing `lib.rs` or `main.rs` is found, then mirror the source's relative path into a sibling directory tree of that name, with the extension changed to `.txt` (e.g. `…/project/src/foo/bar.rs` → `…/project/proptest-regressions/foo/bar.txt`). Falls back to `WithSource` if no crate root is found, and to `Off` if no source file is known.
- `WithSource(&str)` — the source path with its extension replaced by the given string (`bar.rs` → `bar.regressions`).
- `Direct(&str)` — the literal path, source file ignored.
- `Off` — no file and no I/O (semantically `Direct("/dev/null")` / `Direct("NUL")`).

`resolve(source: Option<&Path>) -> Option<PathBuf>` (`pub(super)`) computes that path. It first runs `absolutize_source_file`, which makes `file!()` absolute: a no-op on Unix (where `file!()` is already absolute), but on Windows it pops the cwd upward until `cwd.join(source)` names an existing file. Saving `create_dir_all`s any missing parent directories, claims the multi-line `#`-comment header block via `OpenOptions::create_new` — an OS-atomic claim, so exactly one writer (in this process *or any other*) writes the header — then appends each seed line as a single whole-record `write_all` on an append-mode handle, and emits a "Saving this and future failures … you may wish to add the following line" hint (including the seed, noting when the file was just created) through the runner diagnostics seam. There is no lock: the atomic header claim plus the read side's torn-line tolerance replace the old process-global `RwLock`, and concurrent processes appending to the same file interleave whole records. There is no in-file dedup: saves append unconditionally.

`MapFailurePersistence` (`map.rs`, no_std / `alloc`) — `pub map: BTreeMap<&'static str, BTreeSet<PersistedSeed>>`, keyed by source file. In-memory only; `save` silently drops a `None` source, and the `BTreeSet` dedups identical seeds. Intended for accumulating failures across several `TestRunner` instances for external or batched reporting.

`NoopFailurePersistence` (`noop.rs`) — load returns empty, save does nothing. Note it is a *private*, `#[allow(dead_code)]` struct that `mod.rs` does not re-export (only `file` and `map` are `pub use`d), so it is not reachable from outside the crate; to actually disable persistence set `Config.failure_persistence = None` (or use `FileFailurePersistence::Off`).

## Feature gating & no_std

- `mod file;` and its glob re-export are `#[cfg(feature = "std")]` (and carry a `doc(cfg)` attribute for docsrs), so `FileFailurePersistence` only exists with `std`. `map` and `noop` always compile.
- `mod.rs`, `map.rs`, and `noop.rs` import `Box` / `Vec` / `fmt` / `BTreeMap` / `BTreeSet` from `crate::std_facade`, never `std`/`alloc`, to stay no_std-clean. `file.rs` is the one file here that names `std::` directly — that is fine precisely because the whole module is `std`-gated.

## Gotchas

- The `proptest-regressions/` directories that `FileFailurePersistence` writes are intentional, checked-in fixtures — don't delete them. Each file's header comment tells users to commit it.
- Keep `Seed`'s encoding in `../rng.rs` and `PersistedSeed`'s `Display` / `FromStr` here in lock-step: changing one without the other silently breaks replay of every already-persisted seed.
