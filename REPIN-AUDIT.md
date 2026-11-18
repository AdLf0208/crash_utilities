> **Historical (audit at `83ad384f`, 0.1.0 format).** Nothing here describes the current package:
> the `.hnmod` container, `horizon-mixin.toml`, the named-hook catalog and the planner are retired
> (Horizon's mod-API merge). Read it as the reason each deferred feature is still deferred, not as
> the port's surface — the current one is in `notes.md` §"Migration to the SDK format". Every row's
> "pin surface" is the **old** catalog's file/line.

# Crash Utilities — re-pin audit at Horizon `83ad384f`

Read-only audit. No port code, build, e2e or campaign document was changed.

- Pin: Horizon **`83ad384f`** (427 hooks / 159 helpers) at `ports/_shared/pins/horizon-83ad384f`;
  mixins `e8cd0aa` at `ports/_shared/pins/horizon-mixins-e8cd0aa`.
- Compared against the port's pin `c9d474dd…` (108 hooks, 0 listed helpers).
- Sources: `notes.md` §Gaps (7 bullets) and ARCHITECTURE.md §8.
- All paths are relative to `ports/_shared/pins/horizon-83ad384f/`.

The port's four hooks are unchanged (`register_commands` v1, `server_about_to_start` v1,
`perform_command` v1, `server_tick` v1), so nothing breaks at the new pin. "New at this pin" below
means the hook name is absent from `c9d474d`'s `native-hooks.json` (the planner rejects unknown
names, `crates/horizon-mixins-core/src/native.rs:88-97`) or the helper did not exist in its tree.

## Deferred features

| # | Deferred feature (notes §Gaps) | Pin surface (symbol, file:line) | Class | Verdict | Exact next step |
|---|---|---|---|---|---|
| 1 | `cu e remove <type>` / `hostile` / `regex` (no generic entity enumeration) | **`entity_tick_pre` (listen) `.../native_hooks.rs:7336` — new** gives `(dimension, entity, kind)` for every ticking mob/item/minecart/boat; `native_entity` `.../native_entity_refs.rs:41`, `native_entity_kind` `:76`; removal `native_remove_mob` `.../native_mobs.rs:302` (silent) / `native_kill_mob` `:293` (loot), `native_remove_entity` `.../native_entities.rs:254` (items/others) | usable (new) | **realizable now** | Track `id → (dimension, kind)` in a map updated from `entity_tick_pre` (it is per-tick, so a plain overwrite, no allocation per tick). On `cu e remove <type>` walk it, filter by kind (a literal list at first; a `regex` crate only if the campaign allows the dependency) and call `remove_mob`/`remove_entity` per id; keep the existing items shortcut. e2e: add a zombie + an item, remove by type, cow control stays. |
| 2 | `cu e entityData` / `cu e find` | `native_entity_nbt` `.../native_entity_data.rs:16`, `native_entity_name` `:79`, `native_entity_tags` `:48`, `native_entity_position` `.../native_entity_refs.rs:60`, `native_entity` `:41` | usable (new helpers) | **realizable now** | Same tracked ids as row 1; `entityData` prints the compound via `native_entity_nbt`, `find <name>` filters `native_entity_name`. |
| 3 | `cu blockentity entityData/find/delete` | Only per-mod block entities: `block_entity` hook `.../native_hooks.rs:2985`, `native_block_entity_sync` `:8754`; no enumeration/removal helper | absent | **blocked** | Pin request for a block-entity list/remove API (the level stores them per chunk). Keep the command out of the tree until then. |
| 4 | `cu i read <player>` | `native_player_stacks(uuid)` `.../native_menus.rs:165` (36 slots of any online player; the fn exists at `c9d474d` too, it is only listed as a helper here) | usable (already available) | **realizable now** | Resolve the name with `native_server_players` (row 10), `native_player_stacks(uuid)` and print slot → item × count. Read-only, no dupe risk. |
| 5 | `cu i remove <player>` | No write to another player's inventory; only `native_exchange_items` `.../native_menus.rs:183` (swaps two players' slots, all-or-nothing) | absent | **blocked** | Pin request (a `PlayerInventory` write API, or `native_open_menu` with write-back). Do not fake it with `exchange_items` — it can dupe/lose. |
| 6 | `cu i open` / `i enderchest` | `native_open_menu` `.../native_menus.rs:124` opens a **mod-owned** chest menu of supplied stacks (taking from it would create items); no ender-chest API (`grep ender_chest` over `game/native_*` finds nothing) | absent | **blocked** | Pin request: open a player's real inventory/ender chest as a handled menu. Display-only via `open_menu` is possible but misleading. |
| 7 | `cu memoryCheck` | No heap/memory read; `native_tick_stats` `.../native_server.rs:322` (new) gives tick rate + tick milliseconds only | absent | **blocked** | Report `/tick query`-style numbers if useful, but do not call it `memoryCheck`. |
| 8 | `cu chunks byLocation` | `chunk_load` (listen) `.../native_hooks.rs:6933`, `chunk_unload` `:6942` (already available); `native_surface_heights` `.../native_world_columns.rs:22` (new) | usable (already available) | **realizable now** | Track loaded chunks from the two hooks (dimension → set); print them sorted with the player's distance. No new pin API needed. |
| 9 | `cu chunks byTicket` | `native_forced_chunks(owner)` `.../native_chunk_tickets.rs:56` (new) lists only tickets **the mod holds** (`native_force_chunk` `:23`); no ticket enumeration | absent | **blocked** | Pin request for an all-owners ticket list. |
| 10 | `cu tp <player>` / offline playerdata | `native_server_players` `.../native_server.rs:204` (`NativeServerPlayer { name, dimension, position, … }`, `.../mod_api.rs:365-389`; fn exists at `c9d474d` too); `native_run_command` `.../native_server.rs:479` (already used by `tick.rs`); offline playerdata: only `player_load_from_file`/`player_save_to_file` hooks (listen, acting player) | usable for online players; absent for offline | **realizable now (online)**; **blocked (offline)** | Extend `perform.rs`: parse `cu tp <player> <x> <y> <z> [dim]`, find the name in `native_server_players`, queue `execute in <dim> run tp <player> …` for `tick.rs`; refuse an unknown (offline) name explicitly. e2e: the self-name path and a wrong-name refusal (a second LAN player only if the rig already has one). |
| 11 | `cu unstuck <name>` to the overworld spawn | `native_server_players` `:204` + `native_entity_position`/`native_entity_block_position` `.../native_entity_refs.rs:60,65`; column scan `native_surface_heights`/`native_read_columns` `.../native_world_columns.rs:22,49`; **no world-spawn read** (`spawn_change` hook `.../native_hooks.rs:5841` only reports changes) | usable (safe spot) / absent (spawn) | **partial** | Same safe-spot rule as today for a named online player: read the position, `read_columns` in that dimension (call from `level_tick_post`, where the level is in place), queue the `tp`. Shared-spawn target stays blocked until a spawn read exists. |
| 12 | Config reload | **`register_configs` (collect) `.../native_hooks.rs:2424` and `config_reloading` (listen) `:2440` — both new** (API 397; file watch with the new text) | usable (new) | **realizable now** | Replace the hand-rolled read in `about.rs` with a `register_configs` registration of `crashutilities.toml`, then rebuild the `Config` in `config_reloading` (paste the new text into the pure parser in `runtime.rs`). Drop the "restart to apply" gap; e2e: edit the file while running, `maximum` changes. |
| 13 | `cu help` English literals | Chat text can be built from translation keys in-crate; lang loads every namespace (`crates/horizon-client-core/src/lang.rs:57-60`), and a content mod supplies `assets/<ns>/lang/en_us.json` | usable (content) | **realizable now (content only)** | Add a tiny content companion (or move `help` strings to `assets/crashutilities/lang/*.json`) and build `Text::tr("crashutilities.help.…")`. Low value; not a pin gap. |

Not listed in the port's gaps but also absent today: upstream's `cu activity` (player activity history)
has no helper and is not in the command tree. It is left out of the counts above.

## Summary

- **Deferred features audited:** 13 (across the 7 gap bullets in `notes.md`).
- **Newly realizable because of the re-pin:** 3 — **rows 1 and 2** (the `cu e` family: the
  `entity_tick_pre` hook and the `native_entity*` handles did not exist at `c9d474d`) and **row 12**
  (config reload: `register_configs`/`config_reloading` are new).
- **Realizable with capabilities already present at `c9d474d`:** 3 — row **4** (`i read`,
  `native_player_stacks` exists in the old tree but was not in its helper catalog), row **8**
  (`chunks byLocation` from `chunk_load`/`chunk_unload`), row **10** (`tp <player>` for online
  players: `native_server_players` + the queued `native_run_command` the port already uses).
- **Partial:** 1 — row 11 (safe spot for an online player yes; the shared spawn is blocked).
- **Still blocked:** 6 — rows 3 (block entities), 5 (`i remove`), 6 (`i open`/ender chest), 7
  (`memoryCheck`), 9 (`chunks byTicket`), and the offline half of row 10. Row 13 is content-only.
- **Highest-value next step:** rows **1 + 2 together** — track ticking entities from
  `entity_tick_pre` and implement `cu e remove <type>`, `cu e entityData`, `cu e find`. Upstream's
  `cu e` family is the mod's main reason for existing for server ops, the pin now has every piece
  (`entity_tick_pre`, `native_entity_nbt/name/kind`, `remove_mob`/`remove_entity`), and it is
  fail-capable in e2e (zombie removed, cow kept, counterfactual leaves both). Row 12 (config
  reload) is the smallest self-contained follow-up.

## How to re-check (read-only)

```sh
P=ports/_shared/pins/horizon-83ad384f
python3 - <<'EOF'
import json; d=json.load(open('ports/_shared/pins/horizon-83ad384f/native-hooks.json'))
for n in ('entity_tick_pre','register_configs','config_reloading','chunk_load','chunk_unload'):
    print(n, [h['mod_api'] for h in d['hooks'] if h['name']==n])
print([h['name'] for h in d['helpers'] if h['name'] in
       ('server_players','player_stacks','entity_kind','entity_nbt','tick_stats','forced_chunks')])
EOF
grep -n "fn native_server_players" -A12 $P/crates/horizon-client/src/game/native_server.rs
grep -n "fn native_player_stacks\|fn native_open_menu\|fn native_exchange_items" \
  $P/crates/horizon-client/src/game/native_menus.rs
grep -n "fn native_entity_nbt\|fn native_entity_name" \
  $P/crates/horizon-client/src/game/native_entity_data.rs
grep -n "fn native_entity_tick_pre" -A10 $P/crates/horizon-client/src/game/native_hooks.rs
```

## Not in scope

No `.hmod` build, no `stage/check/test/e2e`, no `compatibility.json` edit. The port's hook versions
are unchanged, so the shared repin recipe (`_shared/repin-83ad384f.md`) is the only migration step.
