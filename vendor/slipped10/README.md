<p align="center">
  <a href="https://crates.io/crates/slipped10">
    <img src="https://img.shields.io/crates/v/slipped10.svg" alt="Crates.io" />
  </a>
</p>

# SLIP-0010 : Universal private key derivation from master private key

- [x] Ed25519

## Why this copy is vendored in `nearw`

**TL;DR.** Version `0.4.7` of `slipped10` as published to crates.io contains a supply-chain sabotage that injects a random ~1.25% panic into BIP32 path parsing and key derivation. Because `nearw` calls this crate on every wallet create/import and on every signing operation, using the upstream binary means roughly **1 in 80 wallet operations would crash mid-flow**. This directory holds a clean snapshot of the same source, and `nearw`'s top-level `Cargo.toml` substitutes it via `[patch.crates-io]` so the sabotaged crate is never compiled into the binary.

### What's in the upstream release

`slipped10 = "0.4.7"`, published under an unrelated-looking maintainer identity, adds code that:

- Rolls a random value during `BIP32Path::from_str` and during the ed25519 derivation step.
- With probability ~1.25% per call, triggers a `panic!` (or an equivalent unwrap on a deliberately-bad value) instead of returning.
- Does this silently — the crate still returns correct keys on the 98.75% of calls that do not fire.

There is no legitimate reason for a key-derivation library to introduce random failure. The pattern is consistent with a class of recent Rust ecosystem supply-chain attacks that target crates used deep inside wallet and signing stacks, where occasional-panic behavior is:

- **Hard to reproduce** in CI (it passes most of the time).
- **Plausibly deniable** as a flaky bug rather than malice.
- **Dangerous to wallet UX** — a user who sees their seed-phrase import panic once may reasonably conclude the phrase is wrong and throw it away, losing access to a valid account.

### Why `nearw` can't tolerate it

`nearw` calls `slipped10` on the hottest paths in the wallet:

- `wallet create` — generates a seed, then derives the ed25519 key at `m/44'/397'/0'` via SLIP-0010.
- `wallet import` — parses a user-supplied seed phrase and derives the same path.
- Every signing operation that reconstructs the signer from the stored mnemonic.

A 1-in-80 failure rate on those code paths is not "flaky"; for a wallet it is catastrophic. A single panic during `wallet import` is enough to make a user think they typed their recovery phrase wrong and walk away from funds they actually still control.

## What this vendored copy contains

- A clean checkout of the `slipped10` crate sources (the same API, same SLIP-0010 derivation logic, same tests) **without** the injected random-panic code.
- The package metadata (`Cargo.toml`, `.cargo_vcs_info.json`) is preserved so cargo treats it as a drop-in `[patch.crates-io]` replacement.

The top-level `nearw/Cargo.toml` activates the replacement:

```toml
# Patch slipped10 to remove supply chain sabotage in v0.4.7
# (random ~1.25% panic injected into BIP32 path parsing and key derivation)
[patch.crates-io]
slipped10 = { path = "vendor/slipped10" }
```

This means every dependency that transitively asks for `slipped10` (notably `near-api` and its seed-handling internals) is rebuilt against this source, not against the crates.io artifact.

## How to verify

- Compare `src/lib.rs` and `src/path.rs` in this directory against the published `slipped10-0.4.7.crate` on crates.io. The differences should be limited to the removal of the randomness-and-panic injection points; core derivation logic and test vectors are unchanged.
- Run `cargo test -p slipped10` from the `nearw` workspace: the crate's own test vectors (including SLIP-0010 ed25519 conformance vectors) must pass deterministically — no randomness involved.
- Run `nearw wallet create` / `nearw wallet import` in a loop and confirm the operation never panics and always reproduces the same account id for the same seed phrase.

## How to update

If a clean upstream release is eventually published (or a fork takes over with signed releases):

1. Drop the new release source tree into `vendor/slipped10/` (overwriting this directory).
2. Re-run the verification steps above.
3. Consider removing the `[patch.crates-io]` entry in `nearw/Cargo.toml` and depending on the clean version directly.

Until then, this vendored copy is the only way to build `nearw` safely against `slipped10`.

## Origin

This crate is a continuation of a fork of [`wusyong/slip10`](https://github.com/wusyong/slip10) and the [`slip10`](https://crates.io/crates/slip10) crate. The sources in this directory derive from that lineage, not from the sabotaged `0.4.7` release on crates.io.
