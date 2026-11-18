# Crash Utilities — a Horizon port

**Crash Utilities** is a Minecraft mod. Minecraft Mod Utilities for Server Owner. This repository is a **port of it to [Horizon](https://github.com/horizon-rust/horizon)**, the Rust engine for Minecraft Java 26.3. It carries the complete source of the port plus the content files it ships, and installs as a single `.hmod` package.

Upstream: [Darkere/CrashUtilities](https://github.com/Darkere/CrashUtilities) — licence **MIT**.

| | |
|---|---|
| Upstream mod | Crash Utilities ([source](https://github.com/Darkere/CrashUtilities)) |
| Ported to | Horizon 0b14f8a, Minecraft Java 26.3, mod API `0.2` |
| Package | `crashutilities.hmod` (install it with the Horizon launcher) |
| Licence | **MIT** — the upstream mod's licence; see [Licence and credit](#licence-and-credit) |

## What this port implements

- **The command literals are lowercase.** The dispatcher parses before a mod is asked, and its
- **The self-teleports use `EntityRef::teleport_to_level`.** 0.1.0 queued an
- **The config file is the game's, not the port's.** Same path and keys; `register_configs`
- **The safe-spot scan asks the state's collision shape**, not a "full cube" fact: a slab, a
- **The level comes from the invocation.** `execute in <dim> run cu e remove items` acts on that
- **The console and command blocks can run the level-wide commands** (help, `e …`,
- **No log lines.** Config parse warnings are dropped instead of logged.

## Not yet ported

- `e remove regex`: a regex filter needs a crate (`regex`), and a port may not add
- `blockentity *`: no block-entity enumeration/removal API.
- `i *` (inventory read/remove/open), `memoryCheck`, `chunks *`: no APIs
- `tp <player>` / offline playerdata: no playerdata load/save API. Only the
- `unstuck <name>` / overworld spawn: no shared-spawn API; the port's safe-spot
- ~~Config reload~~: **closed by the SDK format** — `config_reloading` re-reads the
- `cu help` text is English literals (a native package has no lang pipeline for a
- **No 0.2.0 in-game run.** `tools/e2e.sh` now needs a **launcher-built** game

## Installing

1. Install the [Horizon launcher](https://github.com/horizon-rust/horizon/releases) (0.2.70 or newer).
2. Add `crashutilities.hmod` to the instance's mods.
3. The package is pinned to a specific Horizon build; the launcher checks that pin.

## Licence and credit

Crash Utilities was created by its upstream authors; the code, models and assets in the original mod
remain theirs and are used here under **MIT**. The Rust in this repository is a clean-room
rewrite for Horizon, not a translation of the original source.

- `LICENSE` — the upstream licence text.
- `LICENSE-NOTICE.md` — what this port carries and where it came from.
- The upstream project: https://github.com/Darkere/CrashUtilities

## Technical details

```
horizon-mod.toml   package manifest (id, version, api, [native] pin)
Cargo.toml         the crate; depends on `horizon-mod-api` from the fetched SDK
src/               lib.rs (the one `impl Hooks`), runtime.rs (pure logic + tests), glue
assets/ data/      the content the package ships
tools/             content generators and the e2e scenario
```

- Build: `cargo horizon-mod build` (packages; it does not compile against the game).
- Test: `cargo test`, `cargo clippy --all-targets -- -D warnings`.
- In-game: `ports/tools/launcher-game.sh run crashutilities` in the campaign tree, or install the
  `.hmod` and play; the harness scenario is `tools/e2e.sh`.

This repository is generated from the campaign's port at `ports/crashutilities/`; the port's own
`notes.md` is the full record (decisions, traps, evidence).
