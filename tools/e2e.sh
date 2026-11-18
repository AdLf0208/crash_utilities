#!/usr/bin/env bash
# In-game check for Crash Utilities (SDK package, 0.2.0).
#
# The scenario uses two levels so the item clear has a control it cannot touch:
# five drops, a cow and a named sheep in The End, three control drops on a stone
# shelf in the Nether. `callitemclear` must clear the End drops after the
# configured two-second countdown, keep the cow, and leave the Nether drops
# alone. Then `tp` must refuse an unknown dimension (nobody moves), land the
# player in the Nether at the requested position, and `unstuck` must lift a
# buried player to the highest safe spot in the column. `e entityData`, `e find`
# and `e remove` must see the named sheep and leave the cow and the drops.
#
#   GAME=<launcher-built horizon-client> tools/e2e.sh
#   GAME=… COUNTERFACTUAL=1 tools/e2e.sh   # `cu help` instead of the clear: the
#                                          # five drops, the cow and the sheep stay
#   GAME=… SELFTEST=1 tools/e2e.sh         # expects the drops to SURVIVE the
#                                          # countdown: the run MUST fail there
#   GAME=… CONTROL_GAME=<a game without this package> tools/e2e.sh
#                                          # the differential: the same clear
#                                          # command must remove nothing
#
# SELFTEST is the fail-capability proof on the default run: the scenario and
# every other assertion are unchanged, only the expected item count after the
# countdown is inverted, so a run whose clear silently did nothing would pass
# the rest and fail here. CONTROL_GAME proves the countdown is the package's and
# not the game's own behaviour.
#
# The SDK package is one flat `.hmod` (code and, here, no content), so the game
# must be built **by the launcher** with this package installed — there is no
# `--dev-mod` folder and no staging build any more:
#
#   horizon-launcher native-build <this port> --instance <instance> --trust-native-code
#
# then point GAME at that instance's `horizon-client`. `horizon-launcher` is not
# installed in this environment, so no 0.2.0 in-game run is claimed (see
# notes.md, "Not re-run in this migration"); the 0.1.0 transcripts in
# `../_shared/evidence/crashutilities/` remain the behaviour's evidence for a
# `runtime.rs` that did not change.
#
# Harness notes that still hold:
#   * A mod's command is typed with `ctl chat "/cu ..."` (the player's command
#     path); `ctl command` runs the console's own path.
#   * The command tree's literals are lowercase, so the prompt spellings are
#     `callitemclear` and `entitydata` (see notes.md).
#   * The config is read from `config/crashutilities.toml` (written with the
#     defaults when missing; a world's `serverconfig/` copy wins when present).
#   * `ctl entities` reports mobs as `kind: "mob"`, so the cow control counts the
#     type-filtered reply instead of filtering on the entity id.
#   * An `execute in the_nether run fill` at an unloaded chunk answers "That
#     position is not loaded", so the player is teleported there first.
set -euo pipefail

CLONE=/home/adlf/Desktop/all_the_mods_10_rust_rewrite
PORT=$CLONE/ports/crashutilities
# The control binary is shared at the campaign pin; the game is the launcher's build.
CTL=${CTL:-$CLONE/ports/_shared/bin/horizon-1d63e0a5/horizon-ctl}
GAME=${GAME:-}
CONTROL_GAME=${CONTROL_GAME:-}
RES=${RES:-${HORIZON_RES:-/home/adlf/Desktop/horizon/.local/horizon-client/26.3}}
SOCK=${SOCK:-/tmp/opencode/crashutilities-e2e.sock}
LOG=${LOG:-$CLONE/ports/_shared/evidence/crashutilities/e2e.log}
COUNTERFACTUAL=${COUNTERFACTUAL:-0}
SELFTEST=${SELFTEST:-0}
WORLD=${WORLD:-CrashUtilsE2E}
CONTROL_WORLD=${CONTROL_WORLD:-CrashUtilsControl}

[ -n "$GAME" ] || {
    echo "GAME must name a launcher-built horizon-client carrying crashutilities.hmod" >&2
    echo "  horizon-launcher native-build $PORT --instance <i> --trust-native-code" >&2
    exit 1
}
[ -x "$CTL" ] || { echo "missing horizon-ctl: $CTL" >&2; exit 1; }
[ -x "$GAME" ] || { echo "missing horizon-client: $GAME" >&2; exit 1; }
[ -d "$RES/assets" ] || { echo "missing prepared resources: $RES" >&2; exit 1; }
[ -f "$PORT/horizon-mod.toml" ] || { echo "missing the SDK package: $PORT" >&2; exit 1; }
command -v jq >/dev/null || { echo "missing jq" >&2; exit 1; }
if [ -n "$CONTROL_GAME" ]; then
    [ -x "$CONTROL_GAME" ] || { echo "missing CONTROL_GAME: $CONTROL_GAME" >&2; exit 1; }
fi

# The count of End drops the default run must end with: 0, or 5 in SELFTEST,
# where the clear must NOT have happened, so the run fails.
if [ "$SELFTEST" = 1 ]; then EXPECT_AFTER=5; else EXPECT_AFTER=0; fi

mkdir -p "$(dirname "$LOG")" "$(dirname "$SOCK")"
exec > >(tee -a "$LOG") 2>&1

SCRATCH=$(mktemp -d /tmp/opencode/crashutilities-home.XXXXXX)
export HORIZON_HOME=$SCRATCH/game
export HORIZON_LAUNCHER_HOME=$SCRATCH/launcher
export XDG_DATA_HOME=$SCRATCH/data
export XDG_CONFIG_HOME=$SCRATCH/config
export HORIZON_NO_AUDIO=1
mkdir -p "$SCRATCH/config"
# A two-second countdown, one item enough to trigger it, no title command.
cat > "$SCRATCH/config/crashutilities.toml" <<'CFG'
enabled = true
timer = 30
maximum = 1
warnings = "2"
title = false
titletext = "ITEMCLEAR INCOMING!"
warningtext = "ITEMCLEAR IN % SECONDS"
CFG
rm -f "$SOCK"

ctl() { "$CTL" --socket "$SOCK" "$@"; }
# Mod commands go through the player's command path (see the note above).
say() { ctl chat "/$1"; }
items() { ctl entities 64 item | jq '[.entities[] | select(.kind == "item")] | length'; }
cows() { ctl entities 8 cow | jq '.entities | length'; }
sheep() { ctl entities 8 sheep | jq '.entities | length'; }
dimension() { ctl status | jq -r '.dimension'; }
coord() { ctl status | jq -r ".player.position[$1]"; }

# Print the run's real exit status: `PASS:` alone is not exit-status evidence.
finish() {
    local status=$?
    ctl quit >/dev/null 2>&1 || true
    sleep 2
    rm -rf "$SCRATCH" 2>/dev/null || true
    printf 'EXIT:%s\n' "$status" >> "$LOG"
    echo "EXIT:$status"
}
trap finish EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }

launch() {
    local game=$1 world=$2
    rm -f "$SOCK"
    ctl launch \
        --game "$game" --world "$world" --seed 20261102 --creative --headless --wait \
        --resources "$RES"
    ctl wait-ready 120
}

echo "== launch =="
cd "$SCRATCH"
launch "$GAME" "$WORLD"
ctl command "time set noon"
ctl command "kill @e[type=!player]"
ctl command "kill @e[type=item]"

# The End's main island is empty, so the five summoned drops are the only item
# entities there (the overworld keeps producing ambient drops while it ticks).
echo "== move to The End (an empty level) =="
ctl command "execute in minecraft:the_end run tp @s 0 100 0"
ctl wait 40
dim=$(dimension)
echo "dimension: $dim"
[ "$dim" = "minecraft:the_end" ] || fail "expected the_end, got $dim"
ctl command "fill -2 63 -2 8 63 2 minecraft:stone"
ctl command "execute in minecraft:the_end run tp @s 0.5 64 0.5"
ctl wait 20
ctl command "kill @e[type=item]"

echo "== Nether control shelf: three drops the End clear cannot touch =="
# Load the Nether chunk first (a fill at an unloaded position is refused), let
# the player fall onto the shelf, then place the control drops on it.
ctl command "execute in minecraft:the_nether run tp @s 10 100 10"
ctl wait 10
ctl command "fill 6 69 6 12 69 12 minecraft:stone"
ctl command "execute in minecraft:the_nether run tp @s 10 70 10"
ctl wait 10
ctl command "execute in minecraft:the_nether run kill @e[type=item]"
ctl command "execute in minecraft:the_nether run summon minecraft:item 8 70 10 {Item:{id:\"minecraft:diamond\",count:1},NoGravity:1b,PickupDelay:32767s}"
ctl command "execute in minecraft:the_nether run summon minecraft:item 9 70 10 {Item:{id:\"minecraft:diamond\",count:1},NoGravity:1b,PickupDelay:32767s}"
ctl command "execute in minecraft:the_nether run summon minecraft:item 8 70 11 {Item:{id:\"minecraft:diamond\",count:1},NoGravity:1b,PickupDelay:32767s}"
ctl command "execute in minecraft:the_end run tp @s 0.5 64 0.5"
ctl wait 40

echo "== five End drops + a cow control + a named sheep =="
for x in 3 4 5 6 7; do
    ctl command "summon minecraft:item $x 64 0 {Item:{id:\"minecraft:diamond\",count:1},NoGravity:1b,PickupDelay:32767s}"
done
ctl command "summon minecraft:cow 0 64 0 {NoAI:1b}"
ctl command "summon minecraft:sheep 2 64 0.5 {CustomName:{text:\"Flossy\"},NoAI:1b}"
ctl wait 40
before=$(items)
cow_before=$(cows)
sheep_before=$(sheep)
echo "End items after waiting: $before (cow: $cow_before, sheep: $sheep_before)"
[ "$before" -eq 5 ] || fail "expected 5 End drops before the command, got $before"
[ "$cow_before" -eq 1 ] || fail "expected the cow control, got $cow_before"
[ "$sheep_before" -eq 1 ] || fail "expected the named sheep, got $sheep_before"

if [ "$COUNTERFACTUAL" = 1 ]; then
    echo "== counterfactual: cu help must not remove them =="
    say "cu help"
    ctl wait 80
    after=$(items)
    cows_after=$(cows)
    sheep_after=$(sheep)
    echo "End items after cu help: $after (cow: $cows_after, sheep: $sheep_after)"
    ctl messages 10 | jq -r '.chat[]' | grep -q "Crash Utilities:" \
        || fail "cu help produced no help text"
    [ "$after" -eq 5 ] || fail "items vanished without the clear command"
    [ "$cows_after" -eq 1 ] || fail "the cow vanished without a remove command"
    [ "$sheep_after" -eq 1 ] || fail "the sheep vanished without a remove command"
    echo "COUNTERFACTUAL PASS: cu help leaves the five drops, the cow and the sheep in place"
    exit 0
fi

echo "== cu e entitydata sheep / cu e find =="
say "cu e entitydata sheep"
ctl wait 20
msgs=$(ctl messages 20 | jq -r '.chat[]')
echo "$msgs" | grep -q "minecraft:sheep" || fail "entitydata printed no sheep"
echo "$msgs" | grep -q "Flossy" || fail "entitydata lost the sheep's custom name"
say "cu e find Flossy"
ctl wait 20
msgs=$(ctl messages 20 | jq -r '.chat[]')
echo "$msgs" | grep -q 'name "Flossy"' || fail "find did not list the named sheep"
say "cu e find Nobody"
ctl wait 20
msgs=$(ctl messages 20 | jq -r '.chat[]')
echo "$msgs" | grep -q "No entities matching Nobody found" \
    || fail "find did not report that nothing matched"

echo "== cu e remove sheep (the cow and drops must stay) =="
say "cu e remove sheep"
ctl wait 20
msgs=$(ctl messages 20 | jq -r '.chat[]')
echo "$msgs" | grep -q "Removed 1 Entities" || fail "missing the remove message"
sheep_after=$(sheep)
cows_after=$(cows)
before_clear=$(items)
echo "after remove: sheep $sheep_after, cow $cows_after, items $before_clear"
[ "$sheep_after" -eq 0 ] || fail "$sheep_after sheep remain after cu e remove sheep"
[ "$cows_after" -eq 1 ] || fail "the cow control was removed with the sheep"
[ "$before_clear" -eq 5 ] || fail "cu e remove sheep touched the item drops"

echo "== cu callitemclear (configured two-second countdown) =="
say "cu callitemclear"
ctl wait 20
msgs=$(ctl messages 20 | jq -r '.chat[]')
echo "$msgs" | grep -q "5 Item Entities in World. Limit is set to 1" \
    || fail "missing the item-count message"
echo "$msgs" | grep -q "ITEMCLEAR IN 2 SECONDS" \
    || fail "missing the countdown warning"
ctl wait 80
msgs=$(ctl messages 40 | jq -r '.chat[]')
echo "$msgs" | grep -q "5 Items cleared" || fail "missing the cleared message"
after=$(items)
cows_after=$(cows)
echo "End items after countdown: $after (expect $EXPECT_AFTER; cow: $cows_after)"
[ "$after" -eq "$EXPECT_AFTER" ] || fail "$after End item entities remain, expected $EXPECT_AFTER"
[ "$cows_after" -eq 1 ] || fail "the cow control was removed too"

echo "== cu tp refuses an unknown dimension and moves nobody =="
bx=$(coord 0)
by=$(coord 1)
bz=$(coord 2)
say "cu tp nowhere 0 70 0"
ctl wait 20
msgs=$(ctl messages 20 | jq -r '.chat[]')
echo "$msgs" | grep -q "Unknown dimension nowhere" \
    || fail "missing the unknown-dimension message"
dim=$(dimension)
[ "$dim" = "minecraft:the_end" ] || fail "the refused teleport changed the level to $dim"
ax=$(coord 0)
ay=$(coord 1)
az=$(coord 2)
awk -v bx="$bx" -v ax="$ax" -v by="$by" -v ay="$ay" -v bz="$bz" -v az="$az" \
    'BEGIN { exit !((bx-ax)^2 <= 0.25 && (by-ay)^2 <= 1 && (bz-az)^2 <= 0.25) }' \
    || fail "the refused teleport moved the player from $bx $by $bz to $ax $ay $az"
echo "refused: still at $ax $ay $az in $dim"
dim_refused=$dim
rx=$ax
ry=$ay
rz=$az

echo "== cu tp the_nether 10 70 10 =="
say "cu tp the_nether 10 70 10"
ctl wait 60
dim=$(dimension)
echo "dimension after tp: $dim"
[ "$dim" = "minecraft:the_nether" ] || fail "expected the_nether, got $dim"
x=$(coord 0)
y=$(coord 1)
z=$(coord 2)
echo "position after tp: $x $y $z"
dim_tp=$dim
tx=$x
ty=$y
tz=$z
awk -v x="$x" -v y="$y" -v z="$z" \
    'BEGIN { exit !((x-10)^2 <= 2.25 && (y-70)^2 <= 2.25 && (z-10)^2 <= 2.25) }' \
    || fail "landed at $x $y $z instead of 10 70 10"
nether_items=$(items)
echo "Nether control items after the End clear: $nether_items"
[ "$nether_items" -ge 3 ] || fail "the control shelf lost its items ($nether_items remain)"

echo "== cu unstuck lifts a buried player =="
ctl command "fill 9 70 9 11 71 11 minecraft:stone"
ctl command "execute in minecraft:the_nether run tp @s 10.5 70 10.5"
ctl wait 10
say "cu unstuck"
ctl wait 60
msgs=$(ctl messages 20 | jq -r '.chat[]')
y=$(coord 1)
x=$(coord 0)
z=$(coord 2)
echo "position after unstuck: $x $y $z"
echo "$msgs" | grep -q "Sent Player .* to a safe spot at 10 72 10" \
    || fail "missing the safe-spot message"
awk -v y="$y" 'BEGIN { exit !(y >= 71.5 && y <= 73.5) }' \
    || fail "unstuck left the player at y=$y instead of the safe spot y=72"

echo "== table =="
printf 'phase               | End items | Nether items | dimension        | position\n'
printf 'before clear        | %9s | %12s | %-16s | 0.5 64 0.5\n' "$before" "3" "minecraft:the_end"
printf 'after countdown     | %9s | %12s | %-16s | 0.5 64 0.5\n' "$after" "3" "minecraft:the_end"
printf 'after refused tp    | %9s | %12s | %-16s | %s %s %s\n' "$after" "3" "$dim_refused" "$rx" "$ry" "$rz"
printf 'after tp the_nether | %9s | %12s | %-16s | %s %s %s\n' "$after" "$nether_items" "$dim_tp" "$tx" "$ty" "$tz"
printf 'after unstuck       | %9s | %12s | %-16s | %s %s %s\n' "$after" "$nether_items" "$dim_tp" "$x" "$y" "$z"

# The differential: a game without this package. The same clear command must
# remove nothing, which proves the countdown above is the package's.
if [ -n "$CONTROL_GAME" ]; then
    echo "== control: the same commands on a game without this package =="
    launch "$CONTROL_GAME" "$CONTROL_WORLD"
    ctl command "kill @e[type=!player]"
    ctl command "execute in minecraft:the_end run tp @s 0 100 0"
    ctl wait 40
    ctl command "fill -2 63 -2 8 63 2 minecraft:stone"
    ctl command "execute in minecraft:the_end run tp @s 0.5 64 0.5"
    ctl wait 20
    ctl command "kill @e[type=item]"
    for x in 3 4 5 6 7; do
        ctl command "summon minecraft:item $x 64 0 {Item:{id:\"minecraft:diamond\",count:1},NoGravity:1b,PickupDelay:32767s}"
    done
    ctl wait 40
    control_before=$(items)
    say "cu callitemclear"
    ctl wait 80
    control_after=$(items)
    echo "control End items: before $control_before, after cu callitemclear $control_after"
    [ "$control_before" -eq 5 ] || fail "the control world lost its drops before the command"
    [ "$control_after" -eq 5 ] || fail "the clear ran without the package installed"
    if ctl messages 20 | jq -r '.chat[]' | grep -q "Crash Utilities:"; then
        fail "the help text answered on a game without the package"
    fi
fi

echo "PASS: callitemclear cleared the End drops after the countdown (cow and Nether control kept), \
cu e entitydata/find/remove worked on the named sheep (cow kept), tp refused an unknown dimension \
and landed in the Nether, and unstuck lifted the buried player"
