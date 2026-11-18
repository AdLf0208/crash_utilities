# Crash Utilities for Horizon — server-admin commands

A native Horizon port of [Crash Utilities](https://github.com/Darkere/CrashUtilities)
9.0.4 (MIT). It puts the admin side of a server on the command line, under the
roots `/cu` and `/crashutilities`, at permission level 2:

| Command | What it does |
|---|---|
| `/cu e remove items` | remove every item entity in the level |
| `/cu e remove <type\|hostile>` | remove every entity of that kind (a mob, item, orb, minecart or boat id), or every monster |
| `/cu e entityData <type>` | print each matching entity's saved NBT, one chat line each (cut to 300 characters) |
| `/cu e find <name>` | list the entities whose **custom** name holds `<name>` |
| `/cu callItemClear` | count the level's ground items and, at/above the configured limit, clear them after a countdown |
| `/cu tp <overworld\|the_nether\|the_end> <x> <y> <z>` | teleport yourself, in or across dimensions |
| `/cu unstuck` | lift yourself to the highest safe standing spot in your column |
| `/cu help` | the commands above |

`e`, `entities` and `entity` are all accepted as the entity alias.

The command tree's literals are lowercase, as vanilla's dispatcher requires, so
the two mixed-case names of the original are spelled **`callitemclear`** and
**`entitydata`** at the prompt (tab completion offers them). Every other spelling,
argument and message is the original's.

## The item clear (upstream's `callItemClear`)

The periodic task and the manual command share one rule set, from
`config/crashutilities.toml` (written with these defaults when the file is missing
— a world's `serverconfig/` copy, when present, wins):

```toml
enabled = false          # upstream's default: clears are off until an admin turns them on
timer = 30               # minutes between background checks (0 = every 10 seconds)
maximum = 1000           # a clear starts when the level holds at least this many items
warnings = "5,20"        # chat warnings, seconds before the clear
title = true             # the countdown title
titletext = "ITEMCLEAR INCOMING!"
warningtext = "ITEMCLEAR IN % SECONDS"
```

At the clear's due tick the level is **re-counted** and only cleared when it is
still above the limit (`Item Clear prevented. Only N items on the ground`
otherwise); a clear is never started twice at once. Editing the file mid-game
re-reads it and restarts the period, as upstream restarts its task on a reload.

## Not in this port

The original's inventory, chunk, memory and block-entity readouts are not ported:
the API surface for them (another player's inventory write, opening a real
inventory as a menu, heap sampling, chunk/ticket enumeration, a block-entity
list) does not exist at this pin. The full list is in [notes.md](notes.md#gaps),
with the shape each would need.

## Building and installing

This is one flat SDK package (`api = "0.2"`): `horizon-mod.toml`, `Cargo.toml`,
`src/` (the one `impl mod_api::Hooks` plus the pure `runtime`), and nothing to
carry. It builds against the pinned SDK — nightly
`0b14f8acfd97fa2c750e42795249dcc981178ca4`, mixins
`e59da9094f0c2fdda6c627fe9eff22dec6a906f0`, for 26.3:

```sh
cargo test --offline                                   # the pure rules
cargo clippy --all-targets -- -D warnings
cargo fmt --check
rm -f Cargo.lock && cargo horizon-mod build            # -> target/horizon-mod/crashutilities.hmod
cargo horizon-mod check crashutilities.hmod
cargo horizon-mod fits crashutilities.hmod --build nightly
horizon-launcher native-build . --instance <instance> --trust-native-code
```

`tools/e2e.sh` is the in-game scenario; it needs the launcher-built
`horizon-client` (set `GAME=`, and optionally `CONTROL_GAME=` for the
differential and `SELFTEST=1` for the run that must fail).

## Licence

MIT, from the original mod (Copyright (c) Darkere); see [LICENSE](LICENSE) and
[LICENSE-NOTICE.md](LICENSE-NOTICE.md). Local-only until the maintainer approves
publishing (ARCHITECTURE.md §8).
