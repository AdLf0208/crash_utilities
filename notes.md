# Crash Utilities — port notes (P3 logic tier, sixth port)

Ported from **Crash Utilities 9.0.4** (`Darkere/CrashUtilities`, MIT) for
Minecraft Java 1.21.1 NeoForge. Local-only port.

Status: `ported-partial` — slice 1 implements `cu e remove items` and `cu help`;
slice 2 adds `cu callItemClear`, `cu tp <dimension> <x> <y> <z>` and
`cu unstuck`. Permission level 2, both roots `cu` and `crashutilities`, entity
aliases `e`/`entities`/`entity`. The rest of the upstream command set is
deferred or blocked by pin APIs (table below).

**Format (2026-11-18):** the port is one flat SDK package (`api = "0.2"`, version
**0.2.0**), not the retired `.hnmod`. Everything below §"Behaviour implemented" was
written for the 0.1.0 bundle and is kept as history; §"Migration to the SDK format"
at the end records what moved, what maps where, and the numbers to reproduce.


## Behaviour implemented

### Slice 1

- **`cu e remove items`** — removes every item entity in the command player's
  level and answers `Removed N Entities`. The pin lists item entities by box and
  the level keeps its dropped items in one list, so one very large box around
  the player covers all loaded drops. Upstream walks every level; ours is the
  level in place (documented deviation).
- **`cu help`** — prints the implemented lines (upstream's own help list is
  stale and advertises commands this slice does not have).

### Slice 2 — `cu callItemClear`

Upstream semantics (file:line at commit `8ac8a96`):

| Behaviour | Evidence |
|---|---|
| Literal `callItemClear`, no arguments, registered under the level-2 root | `CrashUtilCommands/ItemClearCommand.java:17-21`; `CrashUtils.java:112` |
| Disabled config answers `ItemClears are not enabled in the config` | `ItemClearCommand.java:25-32` |
| Manual run counts item entities and broadcasts `<N> Item Entities in World. Limit is set to <max>` on the next tick | `ItemClearCommand.java:26-29`; `ClearItemTask.java:62-70` |
| Below the limit: nothing else happens | `ClearItemTask.java:70` |
| At/above the limit: warnings at `(last - warning)` seconds and a clear at `last` seconds, where `last` is the largest warning | `ClearItemTask.java:49-50,71-106` |
| At clear time it re-counts and clears only when `size > maximum`; broadcasts `N Items cleared` or `Item Clear prevented. Only N items on the ground` | `ClearItemTask.java:78-92` |
| A `title @a` `ITEMCLEAR INCOMING!` (dark red) runs when the countdown starts | `ClearItemTask.java:109-115` |
| Defaults: enabled `false`, timer 30 min, maximum 1000, warnings `5,20`, title true, titletext `ITEMCLEAR INCOMING!`, warningtext `ITEMCLEAR IN % SECONDS` | `ServerConfig.java:30-39` |
| `timer = 0` means a 10-second period | `ClearItemTask.java:51-54` |
| The periodic task starts on server start | `CrashUtils.java:146-152`; `ClearItemTask.java:43-55` |
| The shared next-tick queue always supplies the overworld, so a command typed elsewhere still clears the overworld | `CrashUtils.java:198` |

There is **no radius** and no argument: the brief's "radius" does not exist in
9.0.4. The count is the whole level (upstream: the overworld). The port keeps
that shape: the manual command counts and clears the command player's level (the
level in place at the pin).

Port shape:

- `config/crashutilities.toml` (beside `options.txt`, via `native_config_dir`,
  API 26) is read/written at `server_about_to_start` (API 44). Keys keep the
  upstream names (`enabled`, `timer`, `maximum`, `warnings`, `title`,
  `titletext`, `warningtext`). Defaults are upstream's, including
  `enabled = false`.
- `/cu callItemClear` answers the disabled message when disabled; otherwise it
  counts the acting player's level, answers the count message, and — at or above
  the limit — starts a tick countdown (`warnings` seconds, 20 ticks/second).
  Warnings are broadcast, the title command runs at the first tick (where
  `native_run_command` is allowed), and the clear re-counts and removes with
  `native_item_entities`/`native_set_item_entity` (API 42). One countdown at a
  time; a second command answers `Item clear already scheduled`.
- The periodic check uses the same cycle every `timer` minutes (10 seconds when
  0) on the level in place. It starts at the configured interval, not
  immediately, so a fresh world is not checked at tick 0.
- Config is loaded once at server start; the pin has no config-reload hook in
  this slice (upstream restarts its task on reload).

### Slice 2 — `cu tp` and `cu unstuck`

Upstream semantics:

| Behaviour | Evidence |
|---|---|
| `cu tp <player> <pos> [dim]` teleports an online player (or an offline fake player) and reports `Teleported <name> to <pos>` | `PlayerCommands/TeleportCommand.java:23-33,39-113` |
| `pos` is a block pos; Y=0 is replaced by the heightmap's motion-blocking height; the teleport uses `x + 1` | `WorldUtils.java:58-69` |
| `cu unstuck <name>` sends the player to the overworld shared spawn, reducing to `tp` semantics; reports `Sent Player <name> to Spawn` | `PlayerCommands/UnstuckCommand.java:22-27,31-54` |
| `unstuck` on an **online** player never sets its success reference and then unboxes it (an NPE) | `UnstuckCommand.java:34,48` |
| Both commands require permission level 2 | `CommandUtils.java:21`; `CrashUtils.java:107-142` |

The pin has no direct player-position write API and no shared-spawn API.
`native_run_command(line)` (API 21) exists but returns false inside a player
hook (`lan_acting` is set), so the port queues the console line from
`perform_command` and runs it at `server_tick` (API 4), where no player's state
is swapped in. This is the same deferred route ElevatorID/FTB Essentials use.

Port shape and deviations:

- `/cu tp <dimension> <x> <y> <z>` acts on the calling player (the brief's
  syntax), not on an arbitrary player name: the pin cannot load offline
  playerdata, and a name would be a silent no-op for offline players. The
  dimension word accepts the bare and `minecraft:`-prefixed spellings of the
  pin's three levels. Unknown dimensions are refused with
  `Unknown dimension <word>` and nothing is queued. Out-of-range coordinates
  are refused with the usage line.
- `/cu unstuck` acts on the calling player and moves them to the **highest safe
  standing spot in their current column** (feet and head passable, solid block
  below, no fluid), not the overworld spawn: the pin has no spawn-position API.
  A buried player is lifted out; a player already on the surface is re-placed at
  the same spot. If the column has no safe spot (void, lava surface), the
  command answers `No safe spot found for <name>`.
- The teleport line is `execute in <dimension> run tp <player> <x> <y> <z>`;
  unstuck uses `x.5 <y> z.5`. Success is reported to the player as
  `Teleported <name> to <dimension> <x> <y> <z>` /
  `Sent Player <name> to a safe spot at <x> <y> <z>`; a refused
  `native_run_command` answers `Unable to teleport <name>`.
- `callItemClear`'s uppercase literal cannot be a pin command-tree literal
  (literals are lowercase only), so the tree declares `callitemclear`; the
  handler accepts both spellings. The raw line reaches `perform_command` because
  the server routes by the registered first word (`cu`).

## Hooks

The 0.1.0 bundle's plan (historical; the SDK package has no plan — see
§"Migration to the SDK format"):

| Hook | Helper | What it does |
|---|---|---|
| `register_commands` (v1, API 8) | `register.rs` | Pushes the `/cu` usages at permission level 2. |
| `server_about_to_start` (v1, API 44) | `about.rs` | Loads or writes `config/crashutilities.toml`. |
| `perform_command` (v1, API 8) | `perform.rs` | Parses the raw line, answers help/clear/tp/unstuck, queues teleports. |
| `server_tick` (v1, API 4) | `tick.rs` | At `ServerTickEnd` of a running tick: runs queued `/tp` lines, advances the countdown, runs the periodic check. |
| `entity_tick_listened` (v1, API 146) | `tick_entity_listened.rs` | Says yes, so the game tells entity ticks at all. |
| `entity_tick_pre` (v1, API 146) | `tick_entity.rs` | Tracks `(dimension, kind)` per ticking entity for `/cu e`. |


## Gaps

- `e remove regex`: a regex filter needs a crate (`regex`), and a port may not add
  one; `e remove <type>`/`hostile` and `e entityData`/`find` are implemented
  (slice 3, over `entity_tick_pre`'s tracked ids).
- `blockentity *`: no block-entity enumeration/removal API.
- `i *` (inventory read/remove/open), `memoryCheck`, `chunks *`: no APIs
  (other-player inventories, heap sampling, chunk/ticket enumeration).
- `tp <player>` / offline playerdata: no playerdata load/save API. Only the
  calling player can be teleported.
- `unstuck <name>` / overworld spawn: no shared-spawn API; the port's safe-spot
  rule is the documented substitute.
- ~~Config reload~~: **closed by the SDK format** — `config_reloading` re-reads the
  file and restarts the period (see §"Migration to the SDK format").
- `cu help` text is English literals (a native package has no lang pipeline for a
  readout line); translation keys are a follow-up.


## In-game check

`tools/e2e.sh` launches a creative headless world with
`config/crashutilities.toml` (`enabled = true`, `maximum = 1`, `warnings = "2"`),
moves to The End, and:

1. summons five drops + a `NoAI` cow in The End and three control drops on a
   stone shelf in the Nether;
2. runs `cu callItemClear` and asserts the count message, the
   `ITEMCLEAR IN 2 SECONDS` warning, `5 Items cleared`, 0 End items, the cow
   still present, and the Nether shelf still holding its drops;
3. runs `cu tp nowhere 0 70 0` and asserts `Unknown dimension nowhere`, the same
   dimension and position;
4. runs `cu tp the_nether 10 70 10` and asserts the Nether and a position near
   `10 70 10`;
5. buries the player in stone and runs `cu unstuck`, asserting the safe-spot
   message and a position at `y = 72`.

`COUNTERFACTUAL=1` runs `cu help` instead of the clear; the five End drops must
stay, proving the countdown (not the wait) removes them.

## Upstream source

**Crash Utilities 9.0.4** — <https://github.com/Darkere/CrashUtilities>, analysed at
commit `8ac8a96341dc06285af501ee83cc7f254a00dbfc` (2026-01-21, "add missing
version change"). The repo has no tags (`git ls-remote --tags` empty); the commit
was selected because `gradle.properties` reads `mod_version=9.0.4`,
`minecraft_version=1.21.1`, `neo_version=21.1.43` — the pack version. MIT; the
port stays local-only until the maintainer approves publishing (ARCHITECTURE §8).
Full evidence, command table and e2e sketch: `ports/_shared/upstream/crashutilities/FINDINGS.md`
(analysis scratch, deleted with the clone).

**Areas read:** `CrashUtils.java` (registration + aliases), `CommandUtils.java`,
`ServerConfig.java`, `ClearItemTask.java`, `ItemClearCommand.java`,
`PlayerCommands/{TeleportCommand,UnstuckCommand}.java`, `WorldUtils.java`,
`MemoryChecker.java`, every `CrashUtilCommands/**` command class, and
`DataStructures/{EntityData,TileEntityData,LocationData,LoadedChunkData,PlayerActivityHistory}.java`.

**What the mod does (9.0.4):** `/crashutilities` (alias `/cu`, permission level 2
for the whole tree) exposes server-ops inspection and cleanup: `tp`, `unstuck`,
`memoryCheck`, `callItemClear`, `help`, `chunks [byLocation|byTicket]`,
`activity`, `e entityData|find|remove …`, `blockentity entityData|find|delete`,
and `i read|remove|open|enderchest`. There is **no `/cu clearitems` and no
`/cu tps`**; `callItemClear` is the item-clear trigger, and profiling is
commented out (`CrashUtils.java:115`). `callItemClear` has no radius or
arguments: it counts item entities and, if the configured maximum (default
1000) is reached, warns at the configured seconds-remaining marks (`5,20` by
default), re-counts, and clears only if still above the maximum; it acts on the
overworld only because the shared next-tick queue always supplies the overworld
(`CrashUtils.java:198`). The `e remove items <name>` grammar is buggy (the name
lands in the `force` slot unless a dummy word precedes it), `unstuck` throws on
online players (unset `AtomicReference`), and `blockentity find` never reports
"ticking". These are recorded, not copied.

Skills: superpowers/using-superpowers,verification-before-completion; BeautifulRust/rust-best-practices

## Verification results

Validated on 2026-11-05 with the pin `c9d474dd9116e84c243f8992d75c11314df56f06`
(mixins `d9c83d7`), Rust 1.95.0, prepared 26.3 resources at
`/home/adlf/Desktop/horizon/.local/horizon-client/26.3`.

| Check | Command | Result |
|---|---|---|
| standalone format | `cargo fmt --all --check` | pass |
| standalone test | `cargo test --locked --offline` | `19 passed; 0 failed` |
| standalone clippy | `cargo clippy --all-targets --locked --offline -- -D warnings` | exit 0, no warnings |
| stage | `python3 tools/horizon.py stage --horizon <campaign>/horizon` | `cargo_exit: 0`, `stage_changed: true`, 221.8 s |
| check | `cargo check -p horizon-client --bin horizon-client --no-default-features` in `target/mixins/modded` | exit 0, `Finished dev profile` (2 m 14 s) |
| embedded test | `cargo test ... game::crashutilities::tests -- --nocapture` | `19 passed; 0 failed` (531 filtered), exit 0 |
| build | `cargo build --profile fast ...` | exit 0, `Finished fast profile` (12 m 54 s), `target/horizon-cargo/fast/horizon-client` 81,654,544 B |

The three build steps after `stage` were run with the same crate/bin/profile
flags as `tools/horizon.py` directly against the staged tree, because the
campaign build lock was saturated and its waiters were repeatedly killed by the
machine's OOM pressure; every command used `CARGO_INCREMENTAL=0
CARGO_BUILD_JOBS=2`. No warnings were emitted by the staged check/test/build.

### In-game (fail-capable)

`SOCK=/tmp/opencode/cu-e2e-pos.sock tools/e2e.sh` — exit 0:

```text
End items after waiting: 5 (cow: 1)
End items after countdown: 0 (cow: 1)
refused: still at 0.5 64.0 0.5 in minecraft:the_end
dimension after tp: minecraft:the_nether
position after tp: 10.5 70.0 10.5
Nether control items after the End clear: 3
position after unstuck: 10.5 72.0 10.5
PASS: callItemClear cleared the End drops after the countdown (cow and Nether control kept),
tp refused an unknown dimension and landed in the Nether, unstuck lifted the buried player
```

Phase table (recorded by the script):

```text
phase               | End items | Nether items | dimension          | position
before clear        |         5 |            3 | minecraft:the_end  | 0.5 64 0.5
after countdown     |         0 |            3 | minecraft:the_end  | 0.5 64 0.5
after refused tp    |         0 |            3 | minecraft:the_end  | 0.5 64.0 0.5
after tp the_nether |         0 |            3 | minecraft:the_nether | 10.5 70.0 10.5
after unstuck       |         0 |            3 | minecraft:the_nether | 10.5 72.0 10.5
```

The chat log also shows `5 Item Entities in World. Limit is set to 1`,
`ITEMCLEAR IN 2 SECONDS` and `5 Items cleared`; the five End drops are gone, the
cow control and the three Nether control drops remain.

`SOCK=/tmp/opencode/cu-e2e-cf.sock COUNTERFACTUAL=1 tools/e2e.sh` — exit 0:

```text
End items after waiting: 5 (cow: 1)
End items after cu help: 5 (cow: 1)
COUNTERFACTUAL PASS: cu help leaves the five drops in place
```

The two runs are the differential: the same setup, only the command differs
(`callItemClear` → 0 End drops, `cu help` → 5), so the clear assertion is
fail-capable.

Pin quirks the check had to account for:

- `ctl command` runs through the owner console and bypasses `perform_command`;
  the script uses `ctl chat "/cu ..."`.
- An `execute in the_nether run fill` at an unloaded chunk answers `That
  position is not loaded`; the script teleports the player into the Nether
  first (loading the chunk), builds the shelf, then seats the player on it.
- `NoGravity:1b` on item entities is effectively ignored; the shelves give the
  drops a floor.
- The Nether's bedrock ceiling at y=127 is outside the logical height, so the
  safe-spot scan stops at 128; without that cap `unstuck` put the player on top
  of the ceiling.
- Item entities in another level are not touched: `native_item_entities` is
  level-in-place only, which is also the control in the e2e (three Nether drops
  survive the End clear).


## Migration to the SDK format (0.2.0, 2026-11-18)

Horizon's mod-API merge (#16941, launcher ≥ 0.2.70; PR #17992 cleans up the old files) retired
the `.hnmod` container, and the registry closed every `.hnmod` submission on 2026-11-18 with one
reason. **0.2.0** is the same behaviour as one flat SDK `.hmod`, and this is its **first
publication** (the bundle was never submitted, so there is no closed PR to supersede).

Pins: SDK nightly **`0b14f8acfd97fa2c750e42795249dcc981178ca4`** (2026-11-18, `package_format` 6,
`content_api` "0.2"), mixins **`e59da9094f0c2fdda6c627fe9eff22dec6a906f0`**, launcher 0.2.78, for
26.3. The pinned SDK is cached in-tree at `../_shared/sdk/0b14f8a/`; the build found its copy in
`./.horizon/sdk/` (gitignored, never packed).

### What moved, and where the retired tree is

| 0.1.0 path | Now | Evidence |
|---|---|---|
| `mixins/crashutilities/runtime.rs` | `src/runtime.rs`, **copied with `cp`, `cmp`-checked** (sha256 `777239b6…7fb1`) | `../_shared/evidence/crashutilities/retired-hnmod-0.1.0/mixins/crashutilities/runtime.rs` |
| `mixins/crashutilities/support.rs` (693 lines of glue) | `src/lib.rs`: the `Mod` struct, its state, one `impl Hooks` and the command/tick helpers | `retired-hnmod-0.1.0/mixins/crashutilities/support.rs` |
| 6 per-hook helper `.rs` files (`register`, `about`, `perform`, `tick`, `tick_entity_listened`, `tick_entity`) | one `Hooks` method each; **no per-hook files** | `retired-hnmod-0.1.0/mixins/crashutilities/*.rs` |
| `mixins/crashutilities/horizon-mixin.toml` (2 `[[module]]`, 6 `[[injection]]`) | `horizon-mod.toml` + `src/lib.rs`; there is no plan | `retired-hnmod-0.1.0/mixins/crashutilities/horizon-mixin.toml` |
| `compatibility.json`, `rust-toolchain.toml` | `[native]` in `horizon-mod.toml`; the launcher's toolchain builds | `retired-hnmod-0.1.0/` |
| `tools/horizon.py`, `tools/package.py`, `tools/test_stage.py`, `tools/test_template.py`, `tools/audit/`, `.github/workflows/ci.yml`, the old repo `AGENTS.md` | deleted: staging, `.hnmod` packaging and the plugin-injection audit belong to the retired format | `retired-hnmod-0.1.0/tools/`, `…/github/`, `…/AGENTS.md` |
| `Cargo.lock` | not kept: it is packed and changes the artifact (trap 5) | not kept |

There was **no `content/`** (the mod ships no blocks, items, models or recipes) and **no `.hnmod`
in the port** (the built bundle lived in the publish tree), so nothing has to move to the package
root and nothing is carried. **Kept and updated:** `tools/e2e.sh`, `notes.md`, `REPIN-AUDIT.md`,
`VALIDATION.md`, `LICENSE` (the template's copyright lines replaced by the upstream author's),
`README.md` (**rewritten** — it was still the Veinminer template's page), and `LICENSE-NOTICE.md`
(**added**; the package carries it).

### File map (0.2.0)

| Path | What it is |
|---|---|
| `horizon-mod.toml` | the only manifest and identity: `id = "crashutilities"`, `version = "0.2.0"`, `api = "0.2"`, `side = "both"`, `capabilities = ["registries"]`, `[native]` pinned to `0b14f8a`/`e59da90` |
| `Cargo.toml` | one crate, `edition = "2024"`; its only dependency is `horizon-mod-api` from `.horizon/sdk/` |
| `src/lib.rs` | the one `#[mod_api::horizon_mod] impl Hooks` (9 methods) and the glue behind them |
| `src/runtime.rs` | the pure rules (grammar, config, schedule, messages, safe-spot rule), **byte-identical**, 22 tests |
| `src/nbt_text.rs` | what the SDK does not export: vanilla's SNBT rendering and a custom name out of a text component, 8 tests |
| `LICENSE`, `LICENSE-NOTICE.md`, `README.md` | **inside** the package |
| `crashutilities.hmod` | the deliverable (32,356 B) |

### Helper → hook table (every injection, every module)

| 0.1.0 piece | 0.2.0 | Notes |
|---|---|---|
| `[[module]] runtime` (`runtime.rs`) | `pub mod runtime;` | an ordinary module; the file is unchanged |
| `[[module]] state` (`support.rs`) | the `Mod` struct in `src/lib.rs` | state is `Mutex<Option<State>>` + `Mutex<BTreeMap<…>>` on the mod instance, which the macro boxes once for the session (`Hooks: Send + Sync`) |
| `register_commands` (v1, HEAD) → `register.rs` | `Hooks::register_commands` → `usages()` | the same 8 usages per root at level 2, now also for the `entity` alias |
| `server_about_to_start` (v1, HEAD) → `about.rs` | `Hooks::server_about_to_start` | only resets the per-world state and the period now; the **file** moved to the SDK's config pair |
| — (the config file was `std::fs` + `native_config_dir`) | `Hooks::register_configs` + `Hooks::config_loading` + `Hooks::config_reloading` | `config/crashutilities.toml` unchanged: written with the defaults when missing, parsed in the Config phase, re-read on change |
| `perform_command` (v1, HEAD) → `perform.rs` | `Hooks::run_mod_command` | typed `CommandInvocation`: `command_line` renders it back to the canonical line and `runtime::parse` stays the only parser; `Flow::Cancel` = handled |
| `server_tick` (v1, HEAD) → `tick.rs` | `Hooks::server_tick` | the same `ServerTickEnd { world_running }` gate, the same order (teleports, countdown, periodic check) |
| `entity_tick_listened` (v1, HEAD) → `tick_entity_listened.rs` | `Hooks::entity_tick_listened` | `event.listened = true` |
| `entity_tick_pre` (v1, HEAD) → `tick_entity.rs` | `Hooks::entity_tick_pre` | tracks `(dimension, kind)` per id, now via `level.entities_in` + `EntityRef::discard` for the work |
| `native_feedback` / `native_broadcast` / `native_tell` | `Commands::feedback` / `Messages::broadcast` / `PlayerRef::tell` | the same lines, byte for byte |
| `native_run_command("execute in … run tp …")` | `EntityRef::teleport_to_level` | a real API replaced the console round-trip (behaviour change 2) |
| `native_item_entities` + `native_set_item_entity(empty)` | `LevelRef::entities_in` + `host.item_entity(..)` + `EntityRef::discard` | the count is the same; removal is the documented one |
| `native_entity` / `_nbt` / `_name` / `_position` / `remove_mob` | `host.entity` / `EntityRef::{nbt, position, discard}` | `e find` reads `CustomName` out of the compound (trap 6) |
| `game.res.mob_types().get(kind).category == "monster"` | `EntityRef::classification == MobCategory::Monster` | `hostile` |
| `native_server_block` + `res.facts()` (`full_collision`, `fluid`) | `LevelRef::block_state` + `block_shape(Collision)` + `fluid_height` | behaviour change 4 |
| `native_config_dir` + `std::fs` | `register_configs`/`config_loading` | no path, no `std::fs` in the port at all |
| `log::warn!` diagnostics | dropped | a port may not add a crate and the SDK exports no logger (trap 8) |

**Gap with no home:** none. Every 0.1.0 injection is a typed hook or a `Registrar`-free declaration
above; the one helper with no SDK counterpart is the SNBT printer, which is the port's own
`src/nbt_text.rs` (vanilla's `StringTagVisitor` rules, not an API the SDK lost).

### Behaviour changes with the format (deliberate, and why)

1. **The command literals are lowercase.** The dispatcher parses before a mod is asked, and its
   literals cannot carry capitals, so the prompt spellings are `callitemclear` and `entitydata`
   (0.1.0's raw-line hook accepted `callItemClear`/`entityData` because it saw the line before
   the tree). `runtime::parse` still accepts both spellings, and the aliases `e`/`entities`/
   `entity` are all registered.
2. **The self-teleports use `EntityRef::teleport_to_level`.** 0.1.0 queued an
   `execute in <dim> run tp …` console line because no position-write API existed; the SDK has
   one (it arrives after the current tick, as `/tp` does). The messages are unchanged, and no
   console line appears in the log.
3. **The config file is the game's, not the port's.** Same path and keys; `register_configs`
   writes the defaults when the file is missing and a world's `serverconfig/` copy wins when
   present. **Config reload now works** (recorded as a 0.1.0 gap): a changed file is re-read and
   the check period restarts, as upstream restarts its task.
4. **The safe-spot scan asks the state's collision shape**, not a "full cube" fact: a slab, a
   pane or a fence now counts as solid, so `unstuck` can no longer place a player inside one.
5. **The level comes from the invocation.** `execute in <dim> run cu e remove items` acts on that
   level; a plain command acts on the caller's, which is what 0.1.0 read.
6. **The console and command blocks can run the level-wide commands** (help, `e …`,
   `callitemclear`), which 0.1.0's player-only hook could not. The two self-teleports answer
   `Run this command as a player: it moves the one who typed it`, a line with no 0.1.0 original.
7. **No log lines.** Config parse warnings are dropped instead of logged.

`## Upstream defects fixed`: none found in this migration. The 0.1.0 records stand (the
`e remove items <name>` grammar and `unstuck`'s NPE are not reproduced); nothing new was proved
from the artefact, so nothing new was fixed.

### The ladder (all inside one build slot, `BUILD_LOCAL=1`, `CARGO_INCREMENTAL=0`, no `--target-dir`)

| Step | Command | Result |
|---|---|---|
| runtime first | `cargo test --offline` with only `pub mod runtime;` | **22 passed**, 0 failed — the file is green before any glue exists |
| glue red | `src/nbt_text.rs` written test-first with `todo!()` | **7 failed** (`not yet implemented`), 22 runtime tests green |
| tests | `cargo test --offline` | **30 passed**, 0 failed (22 `runtime::`, 8 `nbt_text::`) — `evidence/crashutilities/sdk-test.log` |
| lints | `cargo clippy --all-targets -- -D warnings` | exit 0, **0 warnings** — `sdk-clippy.log` |
| format | `cargo fmt --check` (**not** `--all`: that checks the fetched SDK) | clean — `sdk-fmt.log`; `cmp` + sha256 after `cargo fmt` prove `src/runtime.rs` was not rewritten |
| package | `rm -f Cargo.lock && cargo horizon-mod build` | `SDK 0b14f8a … in ./.horizon/sdk`, `Built ./target/horizon-mod/crashutilities.hmod` — `sdk-build.log` |
| second build | the same again | **byte-identical**: 32,356 B, sha256 `962a5cb5…d0c2` — `sdk-build-second.log` |
| registry check | `cargo horizon-mod check crashutilities.hmod` | see below, **EXIT 0** — `sdk-check.log`, `sdk-check-json.log` |
| fits the pinned build | `cargo horizon-mod fits crashutilities.hmod --build nightly` | `Crash Utilities:` / `0b14f8a  made for`, **EXIT 0** — `sdk-fits.log` |
| runtime untouched | `unzip -p crashutilities.hmod src/runtime.rs \| sha256sum` | `777239b6…7fb1` = the retired file, byte for byte |
| entries | `unzip -l crashutilities.hmod` | **9 entries** — `sdk-entries.log` |

`check` output:

```
crashutilities 0.2.0 (native-mod, format native-1/0b14f8acfd97fa2c750e42795249dcc981178ca4/e59da9094f0c2fdda6c627fe9eff22dec6a906f0, for 26.3)
  32356 bytes, SHA-256 962a5cb5b7f939e5eaf7ba0b8a5755adec8a30aaf139fd0aeac6a1ae43bbd0c2, unsigned
  native source: unsandboxed; not executed by the registry check. Compile and test against the exact pinned Horizon and Mixins revisions before review.
```

The archive: `horizon-mod.toml`, `Cargo.toml`, `LICENSE`, `LICENSE-NOTICE.md`, `README.md`,
`src/lib.rs`, `src/nbt_text.rs`, `src/runtime.rs`, `uses.json` — **no `Cargo.lock`**, no
`assets/`/`data/` (nothing is carried), and `LICENSE` + `LICENSE-NOTICE.md` are inside.

`uses.json` (in the package) is the migration's summary: **247 surface entries** — the nine hooks
this mod implements (`register_configs`, `config_loading`, `config_reloading`,
`register_commands`, `run_mod_command`, `server_tick`, `server_about_to_start`, `entity_tick_pre`,
`entity_tick_listened`) with their event fields, 22 methods (`Hooks::*`, `Commands::feedback`,
`Commands::run_command`, `Messages::broadcast`, the `Host` services the commands read through),
56 handles (`EntityRef::{nbt,position,classification,discard,teleport_to_level,name,id}`,
`LevelRef::{entities_in,block_state,block_shape,fluid_height}`, `PlayerRef::{tell,dimension,name}`,
`Command::{new,op}`, `Config::{new,file}`, `Tag::*`), one capability (`registries`), one macro,
12 traits and 127 types.

### Traps this migration hit (the next port must know)

1. **`build-slot.sh run` forwards `test`/`build`/`check` to the second machine**, whose synced copy
   leaves out the gitignored `.horizon/sdk/` — the first attempt died on
   `cd: ports/crashutilities: No such file or directory`. Use `BUILD_LOCAL=1` for the SDK ladder;
   the slot is then a local guard, not a driver.
2. **Write evidence logs to `ports/_shared/evidence/<id>/`, not `/tmp`.** The first test run's
   `tee` failed with `No space left on device` (the shared `/tmp` tmpfs was 97 % full), so that
   log had to be taken from the console. Never delete another job's scratch to make room.
3. **`cargo fmt` rewrites every file in the crate, including `src/runtime.rs`.** Here it left the
   runtime byte-identical (proved with `cmp` + sha256 immediately after), but always check: a
   formatted runtime breaks the campaign's rule, and `--check` alone does not tell you which file
   it would rewrite.
4. **A command reaches a mod only after the dispatcher parsed it**, so `perform_command`'s
   raw-line freedom is gone: literals are lowercase (behaviour change 1) and an unregistered
   spelling is refused *before* the mod is asked. Anything the handler accepted but the tree did
   not (`entityData`, `callItemClear`) now needs a registered lowercase literal.
5. **A literal and an argument may be siblings** (`e remove items` vs `e remove <type:word>`), which
   is ambiguous in Brigadier and cannot be resolved by counting arguments. Rendering the *parsed*
   invocation back into a line makes both routes land on the same `runtime::parse` result
   (`items` is checked before the kind), so the ambiguity is harmless — do not branch on
   `invocation.arguments` alone.
6. **`EntityRef::name` is the display name**, not the custom one: it falls back to the type's name,
   so `e find sheep` would list every sheep. Read `CustomName` from `EntityRef::nbt`, and note the
   SDK exports **no SNBT printer** (`to_snbt`/`parse_snbt` are absent): `{:?}` of a `Tag` is Rust
   syntax, not vanilla's, so `src/nbt_text.rs` carries vanilla's `StringTagVisitor` rules,
   including Java's `Float.toString`/`Double.toString` text (`1.0E-4d`, `NaNd`, `-0.0d`).
7. **`JavaString` is not exported**, so a compound lookup by `&str` cannot use `BTreeMap::get`:
   `compound_get` compares the key's UTF-16 units (`JavaString.0`).
8. **No logger.** The SDK exports none and a port may not add the `log` crate, so the 0.1.0
   `log::warn!` lines (config parse warnings, write failures) are gone. The config's own comments
   carry the defaults instead.
9. **`server_about_to_start` is no longer the config moment.** The config is a declaration now:
   `register_configs` (Config phase) writes the defaults, `config_loading`/`config_reloading`
   hand the text over, and `server_about_to_start` only resets per-world state. Reading the file
   in `server_about_to_start` would need the `ConfigFiles` service in the World phase and would
   miss a world's `serverconfig/` copy.
10. **`Config::new(id, kind, default)` names the file `<id>-<kind>.toml`.** `.file("crashutilities.toml")`
    keeps 0.1.0's path; a `Server` kind also looks in a world's `serverconfig/` first.
11. **The package packs `README.md` and every root `LICENSE*`**, not `notes.md`, `VALIDATION.md`,
    `FINDINGS.md` or `AGENTS.md`. The 0.1.0 README was the Veinminer template's page (it tells the
    reader to run `tools/horizon.py`), so it was rewritten before the build; check `unzip -l`
    rather than assuming.
12. **`check --json` reports `side: null` and `capabilities: []` for every port** (enderstorage and
    naturesaura report the same), because it does not echo the manifest. The manifest's
    `side = "both"` and `capabilities = ["registries"]` are packed verbatim — do not "fix" them.
13. **Rust 1.97.0 compiles the crate although `sdk.json` declares 1.99.1** (as the other
    migrations did). There is deliberately no `rust-toolchain.toml`: the launcher's toolchain and
    the pinned SDK decide.

### Gaps after the migration

Everything in §Gaps above still stands (`e remove regex`, `blockentity *`, `i *`, `memoryCheck`,
`chunks *`, `tp <player>`, `unstuck <name>`, English help text). Closed by the format: the config
reload. Added by it: nothing.

- **No 0.2.0 in-game run.** `tools/e2e.sh` now needs a **launcher-built** game
  (`horizon-launcher native-build . --instance <i> --trust-native-code`, `GAME=`, optional
  `CONTROL_GAME=`, `SELFTEST=1` for the run that must fail), and `horizon-launcher` is not
  installed in this environment. The 0.1.0 transcripts in `../_shared/evidence/crashutilities/`
  remain the behaviour's evidence for a `runtime.rs` that did not change, and the migration
  changed only the transport. Claiming `ported` needs that run.
- **`e remove items` and `e remove minecraft:item` still both route through the item helper**
  (they did in 0.1.0); a modded item entity from another rename is not a separate kind here.

Skills: superpowers/using-superpowers, brainstorming, writing-plans, test-driven-development, systematic-debugging, verification-before-completion; BeautifulRust/rust-best-practices

### Changelog for the submission (first publication)

> **First publication** — `crashutilities` 0.2.0, the flat SDK package format (`api = "0.2"`) that
> replaces the retired `.hnmod` container (Horizon's mod-API merge; launcher ≥ 0.2.70). It pins
> nightly `0b14f8acfd97fa2c750e42795249dcc981178ca4` and mixins
> `e59da9094f0c2fdda6c627fe9eff22dec6a906f0` for 26.3. Behaviour is the same slice: `/cu` and
> `/crashutilities` (level 2) for `e remove items|hostile|<type>`, `e entityData`, `e find`,
> `callItemClear` (the configured countdown, `config/crashutilities.toml`), `tp`, `unstuck` and
> `help`. What changed for `api = "0.2"`: the commands are registered in the server's own tree and
> answered through the typed `run_mod_command`, so the two mixed-case names are spelled
> `callitemclear` and `entitydata` at the prompt; the teleports use the SDK's
> `EntityRef::teleport_to_level` instead of a queued console line; the config is declared through
> `register_configs`/`config_loading` and now **reloads** when the file changes; `unstuck` asks the
> block's collision shape, so it never places you inside a slab. `src/runtime.rs` is byte-identical
> to the 0.1.0 file (sha256 `777239b6…7fb1`), so the rules and every message are unchanged.
> `tools/e2e.sh` now drives a launcher-built game and has not been re-run here.
