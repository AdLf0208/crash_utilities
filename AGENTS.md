# Working on this Horizon mod (SDK layout, 2026-11-18)

This port is **one flat `.hmod`**: `horizon-mod.toml` + `Cargo.toml` + `src/` (the one
`impl Hooks`) — no `assets/`/`data/`, because the mod carries no content. The `.hnmod`
container, `mixins/<id>/`, the plan and the injection schema are **retired** (Horizon's
mod-API merge; see `notes.md` §"Migration to the SDK format"). Do not recreate them.

Read `README.md` and `notes.md` first. Keep this repository separate from Horizon,
horizon-mixins and the launcher; the only things fetched are the pinned SDK
(`.horizon/sdk/`, gitignored, never packed) and the cargo cache. Do not commit binaries,
the SDK or Minecraft assets.

- **The pure rules stay in `src/runtime.rs`** and must stay byte-identical to the recorded
  file (sha256 in `notes.md`): it has no `mod_api` type and is what `cargo test` exercises.
  The rules go there — this port's own helper for what the SDK does not export is
  `src/nbt_text.rs` (SNBT text and a custom name), kept separate for the same reason.
- **Read the surface, do not guess it.** `cargo horizon-mod hooks <text>` searches the
  catalog offline; the generated `Hooks`, every event struct and every handle method are in
  `.horizon/sdk/api/crates/horizon-mod-vocab/generated/mod_api_generated{,_host}.rs`.
- **A command reaches a mod through the dispatcher**, not as a raw line: register the
  usages in `register_commands` and answer in `run_mod_command`. Literals are lowercase.
- Run every compile through the campaign slot registry
  (`../tools/build-slot.sh run --job crashutilities-sdk --port ports/crashutilities
  --step <step> -- bash -c 'cd ports/crashutilities && …'`), which blocks until a slot is
  free; set `BUILD_LOCAL=1` — the forwarded copy does not carry the gitignored
  `.horizon/sdk/`, so a remote step fails at `cd`. Never start a build or an e2e in the
  background.
- Ladder: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`,
  `rm -f Cargo.lock`, `cargo horizon-mod build` (copy `target/horizon-mod/crashutilities.hmod`
  to the port root), `cargo horizon-mod check crashutilities.hmod`,
  `cargo horizon-mod fits crashutilities.hmod --build nightly`. Deleting `Cargo.lock`
  before the build is required: it is packed and changes the artifact's SHA-256.
- An in-game check needs a game the **launcher** built
  (`horizon-launcher native-build . --instance <i> --trust-native-code`); `cargo
  horizon-mod run` is content-only. `tools/e2e.sh` takes `GAME=`, an optional
  `CONTROL_GAME=`, and `SELFTEST=1`.
- Keep validation claims and supported revisions honest: the SHA-256 and the `check` line
  in `notes.md` are what reviewers reproduce.
