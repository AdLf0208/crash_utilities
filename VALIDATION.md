> **Historical (0.1.0, retired `.hnmod`).** These numbers were measured on the retired container
> against `compatibility.json` and a staging build. The port is now one flat SDK package
> (`api = "0.2"`, version 0.2.0): see `notes.md` §"Migration to the SDK format" for the current
> ladder (`cargo test` / `clippy` / `fmt` / `cargo horizon-mod build|check|fits`) and the artifact's
> sha256. The behaviour evidence below still describes this port's rules, because `src/runtime.rs`
> is byte-identical (sha256 `777239b6…7fb1`).

# Validation

Validated on Linux with Rust 1.95.0 on 2026-11-05, against the exact revisions
in `compatibility.json`, the prepared 26.3 resources
(`/home/adlf/Desktop/horizon/.local/horizon-client/26.3`) and the real bundle
committed here.

| Check | Result |
|---|---|
| `cargo fmt --all --check` | Pass |
| `cargo clippy --all-targets --locked --offline -- -D warnings` | Pass, no warnings |
| `cargo test --locked --offline` | 19 runtime tests pass |
| `tools/horizon.py stage` | Pass, `stage_changed: true` |
| `cargo check -p horizon-client --bin horizon-client --no-default-features` in `target/mixins/modded` | Pass, exit 0 |
| `cargo test ... game::crashutilities::tests -- --nocapture` in `target/mixins/modded` | 19 embedded tests pass (531 filtered), exit 0 |
| `cargo build --profile fast ...` in `target/mixins/modded` | Pass, exit 0, 81,654,544 B `horizon-client` |
| `tools/e2e.sh` (positive) | Pass: `callItemClear` countdown → 0 End drops, cow and 3 Nether control drops kept; unknown dimension refused; `/cu tp the_nether 10 70 10` lands at `10.5 70 10.5`; `unstuck` lifts a buried player to `y = 72` |
| `COUNTERFACTUAL=1 tools/e2e.sh` | Pass: `cu help` leaves the five drops |

The staged `check`/`test`/`build` steps were run with the same crate, bin and
profile flags as `tools/horizon.py` directly against `target/mixins/modded`,
because the campaign build lock was saturated and its waiters were repeatedly
killed by the machine's OOM pressure. Every command used `CARGO_INCREMENTAL=0
CARGO_BUILD_JOBS=2`; no warnings were emitted. The in-game command and output
are recorded in `notes.md`.

`tools/test_template.py` is the scaffold's tiny-fixture test: it asserts one
injection and one module (`break_block`) and no longer describes this bundle
(four injections, two modules); it is not part of the slice-2 ladder.
