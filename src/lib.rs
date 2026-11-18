//! Crash Utilities for Horizon: server-admin commands from Crash Utilities 9.0.4.
//!
//! Every rule (the command grammar, the item-clear schedule, the config, every
//! message) is in [`runtime`], which the port's own tests exercise with no game.
//! This file is the glue: where the game's moments meet those rules.
//!
//! The 0.1.0 bundle answered a raw command line from `perform_command`. That hook
//! is retired: a command now reaches a mod through the server's own dispatcher
//! (`run_mod_command`), so [`usages`] registers the tree, the dispatcher parses
//! the line, and [`command_line`] renders the parsed invocation back into the
//! canonical line [`runtime::parse`] owns. The one visible difference is that the
//! dispatcher's literals are lowercase, so `callItemClear` and `entityData` are
//! spelled `callitemclear` and `entitydata` at the prompt (see `notes.md`).
//!
//! The countdown and the queued self-teleports run at the end of a running
//! server tick, where the host's writes are allowed. `Config` is the one place
//! a mod's own settings live now (`register_configs` writes the default file,
//! `config_loading` hands the text over, `config_reloading` re-reads it), so the
//! config file keeps its 0.1.0 name and path, `config/crashutilities.toml`.
pub mod nbt_text;
pub mod runtime;

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use mod_api::{
    self, BlockShapeKind, Command, CommandArgumentValue, CommandInvocation, CommandSource,
    ConfigKind, ConfigLoadingEvent, ConfigReloadingEvent, EntityRef, EntityTickListenedEvent,
    EntityTickPreEvent, Flow, Hooks, HostSide, MobCategory, PlayerRef, RegisterCommandsEvent,
    RegisterConfigsEvent, RunModCommandEvent, ServerAboutToStartEvent, ServerHost, ServerTickEvent,
    ServerTickPhase, Tag, Text,
};

use crate::runtime::{
    Action, ClearSchedule, ColumnBlock, Config, HELP, ROOTS, already_scheduled_message,
    clear_schedule, cleared_message, count_message, dimension_bounds, disabled_message,
    entity_data_message, entity_found_message, kind_matches, no_entities_message,
    no_safe_spot_message, parse, parse_config, period_ticks, prevented_message, removed_message,
    safe_spot, short, teleport_failed_message, teleported_message, teleporting_message,
    title_command, to_toml, unknown_dimension_message, unsticking_message, unstuck_message,
    warning_message,
};

/// The mod's id: the config's owner, and the manifest's identity.
const MOD_ID: &str = "crashutilities";

/// The config file's name in the game's `config/` folder: the 0.1.0 path.
const CONFIG_FILE: &str = "crashutilities.toml";

/// The box the item count uses: the whole level (the world border is ±30M).
const ITEM_BOX: f64 = 30_000_000.0;

/// The longest NBT line `/cu e entityData` prints, in characters.
const NBT_LINE: usize = 300;

/// The answer to a self-teleport asked from the console or a command block. The
/// 0.1.0 hook only heard a player's own line; the dispatcher also reaches a
/// console, which has nobody to move.
const PLAYER_ONLY: &str = "Run this command as a player: it moves the one who typed it";

/// One scheduled item clear. The countdown is advanced at the server tick's end.
struct Countdown {
    dimension: String,
    schedule: ClearSchedule,
    elapsed: u64,
    fired: usize,
    title_pending: bool,
    title_text: String,
    warning_text: String,
    maximum: usize,
}

/// A self-teleport queued by a command, run at the server tick's end.
enum Pending {
    Teleport {
        player: PlayerRef,
        name: String,
        dimension: &'static str,
        pos: [i32; 3],
    },
    Unstuck {
        player: PlayerRef,
        name: String,
    },
}

/// The port's own state for one open world.
#[derive(Default)]
struct State {
    config: Config,
    pending: Vec<Pending>,
    countdown: Option<Countdown>,
    ticks: u64,
    next_periodic: u64,
}

/// The port.
///
/// `Hooks` is `Any + Send + Sync`, and the macro boxes one `Default` instance for
/// the session, so the state lives on it behind a `Mutex` and dies with the game.
#[derive(Default)]
struct Mod {
    state: Mutex<Option<State>>,
    /// The entities seen ticking, by id: `(dimension, kind)`.
    ///
    /// `entity_tick_pre` is the one enumeration of mobs, item entities, minecarts
    /// and boats the port needs for `/cu e remove|entityData|find`; the entries
    /// are pruned when a command finds the entity gone. A `BTreeMap` keeps the
    /// readouts in id order.
    tracked: Mutex<BTreeMap<u32, (String, String)>>,
}

impl Mod {
    fn state(&self) -> MutexGuard<'_, Option<State>> {
        self.state
            .lock()
            .expect("the crashutilities state lock is held for one short step")
    }

    fn tracked(&self) -> MutexGuard<'_, BTreeMap<u32, (String, String)>> {
        self.tracked
            .lock()
            .expect("the crashutilities entity table is held for one short step")
    }

    /// Take a config the game read (or re-read) for this mod.
    fn apply_config(&self, mod_id: &str, text: &str, reschedule: bool) {
        if mod_id != MOD_ID {
            return;
        }
        // Parse warnings are dropped: a native mod may not add a crate, the SDK
        // exports no logger, and the file's own comments carry the defaults.
        let (config, _warnings) = parse_config(text);
        let mut guard = self.state();
        let state = guard.get_or_insert_with(State::default);
        state.config = config;
        if reschedule {
            // Upstream restarts its task on a config reload; the next check is a
            // full period out, from now.
            state.next_periodic = state.ticks + period_ticks(state.config.timer_minutes);
        }
    }

    /// `/cu callItemClear`: count now, and start the configured countdown when the
    /// level is at or above the limit (upstream `ItemClearCommand.java:25-32`,
    /// `ClearItemTask.java:57-116`).
    fn start_manual_clear(&self, host: &mut dyn ServerHost, dimension: &str) {
        let config = {
            let guard = self.state();
            guard
                .as_ref()
                .map_or_else(Config::default, |state| state.config.clone())
        };
        if !config.enabled {
            feedback(host, disabled_message());
            return;
        }
        let count = count_items(host, dimension);
        feedback(host, &count_message(count, config.maximum));
        if count < config.maximum {
            return;
        }
        let Some(schedule) = clear_schedule(&config.warnings) else {
            return;
        };
        let mut guard = self.state();
        let Some(state) = guard.as_mut() else {
            return;
        };
        if state.countdown.is_some() {
            drop(guard);
            feedback(host, already_scheduled_message());
            return;
        }
        state.countdown = Some(Countdown {
            dimension: dimension.to_owned(),
            schedule,
            elapsed: 0,
            fired: 0,
            title_pending: config.title,
            title_text: config.title_text,
            warning_text: config.warning_text,
            maximum: config.maximum,
        });
    }

    /// Queue a self-teleport and answer with the line upstream prints when the
    /// command is taken.
    fn queue_teleport(
        &self,
        host: &mut dyn ServerHost,
        caller: Option<PlayerRef>,
        dimension: &'static str,
        pos: [i32; 3],
    ) {
        let Some(player) = caller else {
            feedback(host, PLAYER_ONLY);
            return;
        };
        let name = player.name(host).unwrap_or_default();
        feedback(host, &teleporting_message(&name, dimension, pos));
        let mut guard = self.state();
        if let Some(state) = guard.as_mut() {
            state.pending.push(Pending::Teleport {
                player,
                name,
                dimension,
                pos,
            });
        }
    }

    /// The background check (`ClearItemTask.java:43-60`): count the level in place
    /// and start a countdown when it reached the limit.
    fn periodic_check(&self, host: &mut dyn ServerHost) {
        let (due, config) = {
            let guard = self.state();
            let Some(state) = guard.as_ref() else {
                return;
            };
            (
                state.config.enabled
                    && state.countdown.is_none()
                    && state.ticks >= state.next_periodic,
                state.config.clone(),
            )
        };
        if !due {
            return;
        }
        let dimension = host.dimension().to_owned();
        let count = count_items(host, &dimension);
        let Some(schedule) = clear_schedule(&config.warnings) else {
            return;
        };
        let mut guard = self.state();
        let Some(state) = guard.as_mut() else {
            return;
        };
        state.next_periodic = state.ticks + period_ticks(state.config.timer_minutes);
        if count < config.maximum || state.countdown.is_some() {
            return;
        }
        state.countdown = Some(Countdown {
            dimension,
            schedule,
            elapsed: 0,
            fired: 0,
            title_pending: config.title,
            title_text: config.title_text,
            warning_text: config.warning_text,
            maximum: config.maximum,
        });
    }

    /// One command line, parsed by [`runtime::parse`], run against the caller.
    fn run_action(
        &self,
        host: &mut dyn ServerHost,
        caller: Option<PlayerRef>,
        dimension: &str,
        action: Action<'_>,
    ) {
        match action {
            Action::Help => {
                for line in HELP {
                    feedback(host, line);
                }
            }
            Action::RemoveItems => {
                let removed = clear_items(host, dimension);
                feedback(host, &removed_message(removed));
            }
            Action::RemoveEntities { filter } => {
                let removed = remove_entities(host, &mut self.tracked(), dimension, filter);
                feedback(host, &removed_message(removed));
            }
            Action::EntityData { filter } => {
                let printed = entity_data(host, &mut self.tracked(), dimension, filter);
                if printed == 0 {
                    feedback(host, &no_entities_message(filter));
                }
            }
            Action::FindEntities { needle } => {
                let found = find_entities(host, &mut self.tracked(), dimension, needle);
                if found == 0 {
                    feedback(host, &no_entities_message(needle));
                }
            }
            Action::CallItemClear => self.start_manual_clear(host, dimension),
            Action::Teleport { dimension, pos } => {
                self.queue_teleport(host, caller, dimension, pos);
            }
            Action::Unstuck => {
                let Some(player) = caller else {
                    feedback(host, PLAYER_ONLY);
                    return;
                };
                let name = player.name(host).unwrap_or_default();
                feedback(host, &unsticking_message(&name));
                let mut guard = self.state();
                if let Some(state) = guard.as_mut() {
                    state.pending.push(Pending::Unstuck { player, name });
                }
            }
            Action::BadDimension(word) => feedback(host, &unknown_dimension_message(word)),
            Action::Usage(line) => feedback(host, line),
            Action::Unknown => {}
        }
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    /// Declare `config/crashutilities.toml`; the game writes `default` there when
    /// the file is missing (`config_loading` then hands it back).
    fn register_configs(&self, _host: HostSide, event: &mut RegisterConfigsEvent<'_>) {
        let default = to_toml(&Config::default());
        event
            .configs
            .push(mod_api::Config::new(MOD_ID, ConfigKind::Server, &default).file(CONFIG_FILE));
    }

    /// The config was read: ours is the only one this mod parses. Every mod hears
    /// every config, so the id is checked.
    fn config_loading(&self, _host: HostSide, event: &mut ConfigLoadingEvent<'_>) {
        self.apply_config(&event.config.mod_id, &event.config.text, false);
    }

    /// The config file changed on disk: re-read it and restart the period, as
    /// upstream restarts its task on a config reload.
    fn config_reloading(&self, _host: HostSide, event: &mut ConfigReloadingEvent<'_>) {
        self.apply_config(&event.config.mod_id, &event.config.text, true);
    }

    /// Before the worlds open: a new world starts with no countdown, no pending
    /// teleport and a full period before its first background check.
    fn server_about_to_start(
        &self,
        _host: &mut dyn ServerHost,
        _event: &mut ServerAboutToStartEvent,
    ) {
        let mut guard = self.state();
        let state = guard.get_or_insert_with(State::default);
        state.pending.clear();
        state.countdown = None;
        state.ticks = 0;
        state.next_periodic = period_ticks(state.config.timer_minutes);
    }

    /// The `/cu` usages, at permission level 2 (upstream's `CommandUtils.java:21`).
    fn register_commands(&self, _host: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        for usage in usages() {
            event.commands.push(Command::op(&usage, 2));
        }
    }

    /// A line the dispatcher parsed and allowed: claim it when it is ours.
    fn run_mod_command(
        &self,
        host: &mut dyn ServerHost,
        event: &mut RunModCommandEvent<'_>,
    ) -> Flow {
        let line = command_line(event.command);
        let action = parse(&line);
        if matches!(action, Action::Unknown) {
            return Flow::Continue;
        }
        let caller = match event.source {
            CommandSource::Player { player, .. } => Some(*player),
            _ => None,
        };
        // `execute in` moves the execution level; a plain command runs in the
        // level the caller is in, which is what 0.1.0 read.
        let dimension = event
            .command
            .dimension
            .id(host)
            .unwrap_or_else(|| host.dimension().to_owned());
        self.run_action(host, caller, &dimension, action);
        Flow::Cancel
    }

    /// The end of a running server tick: run the queued teleports, advance the
    /// item-clear countdown, then run the background check.
    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        let world_running = match event.phase {
            ServerTickPhase::ServerTickEnd { world_running } => world_running,
            _ => return,
        };
        if !world_running {
            return;
        }
        let (pending, countdown) = {
            let mut guard = self.state();
            let Some(state) = guard.as_mut() else {
                return;
            };
            state.ticks += 1;
            (std::mem::take(&mut state.pending), state.countdown.take())
        };
        for item in pending {
            run_pending(host, item);
        }
        if let Some(countdown) = countdown {
            let next = advance_countdown(host, countdown);
            let mut guard = self.state();
            if let Some(state) = guard.as_mut() {
                state.countdown = next;
            }
        }
        self.periodic_check(host);
    }

    /// An entity about to tick: remember it, so `/cu e` has something to walk. The
    /// tick is never cancelled, and the table is only written when something
    /// changed, so this stays cheap for a world full of entities.
    fn entity_tick_pre(
        &self,
        _host: &mut dyn ServerHost,
        event: &mut EntityTickPreEvent<'_>,
    ) -> Flow {
        let mut tracked = self.tracked();
        match tracked.get_mut(&event.entity) {
            Some((dimension, kind)) => {
                if dimension != event.dimension {
                    dimension.clear();
                    dimension.push_str(event.dimension);
                }
                if kind != event.kind {
                    kind.clear();
                    kind.push_str(event.kind);
                }
            }
            None => {
                tracked.insert(
                    event.entity,
                    (event.dimension.to_owned(), event.kind.to_owned()),
                );
            }
        }
        Flow::Continue
    }

    /// The game tells entity ticks only while a mod listens; this mod does, or
    /// `entity_tick_pre` never fires and `/cu e` sees nothing.
    fn entity_tick_listened(
        &self,
        _host: &mut dyn ServerHost,
        event: &mut EntityTickListenedEvent,
    ) {
        event.listened = true;
    }
}

/// Every usage the tree declares: both upstream roots, the three entity aliases,
/// and the spellings the dispatcher's lowercase literals allow.
fn usages() -> Vec<String> {
    let mut usages = Vec::new();
    for root in ROOTS {
        for entities in ["e", "entities", "entity"] {
            // The `items` literal wins over the `<type>` argument when it is
            // typed, as vanilla's tree gives a literal node priority.
            usages.push(format!("{root} {entities} remove items"));
            usages.push(format!("{root} {entities} remove <type:word>"));
            usages.push(format!("{root} {entities} entitydata <type:word>"));
            usages.push(format!("{root} {entities} find <name:word>"));
        }
        usages.push(format!("{root} help"));
        usages.push(format!("{root} callitemclear"));
        usages.push(format!(
            "{root} tp <dimension:word> <x:integer> <y:integer> <z:integer>"
        ));
        usages.push(format!("{root} unstuck"));
    }
    usages
}

/// The canonical line for a parsed invocation, so [`runtime::parse`] stays the
/// port's only parser (`Action` carries borrowed words, so the line is built
/// first and parsed from it).
fn command_line(invocation: &CommandInvocation) -> String {
    let mut line = invocation.literals.join(" ");
    for argument in &invocation.arguments {
        match &argument.value {
            CommandArgumentValue::String(text) => {
                line.push(' ');
                line.push_str(text);
            }
            CommandArgumentValue::Integer(value) => {
                line.push(' ');
                line.push_str(&value.to_string());
            }
            // The tree declares no other kind.
            _ => {}
        }
    }
    line
}

/// Advance one countdown by a tick; `Some` puts it back when it has not finished.
fn advance_countdown(host: &mut dyn ServerHost, mut countdown: Countdown) -> Option<Countdown> {
    countdown.elapsed += 1;
    if countdown.title_pending {
        countdown.title_pending = false;
        let _ = host
            .commands()
            .run_command(&title_command(&countdown.title_text));
    }
    while countdown.fired < countdown.schedule.warnings.len() {
        let warning = countdown.schedule.warnings[countdown.fired];
        if countdown.elapsed < warning.due_tick {
            break;
        }
        let line = warning_message(&countdown.warning_text, warning.seconds);
        let _ = host.messages().broadcast(Text::literal(line));
        countdown.fired += 1;
    }
    if countdown.elapsed >= countdown.schedule.total_ticks {
        let count = count_items(host, &countdown.dimension);
        if count > countdown.maximum {
            clear_items(host, &countdown.dimension);
            let line = cleared_message(count);
            let _ = host.messages().broadcast(Text::literal(line));
        } else {
            let line = prevented_message(count);
            let _ = host.messages().broadcast(Text::literal(line));
        }
        return None;
    }
    Some(countdown)
}

/// Run one queued self-teleport. A player who logged off in between is skipped.
fn run_pending(host: &mut dyn ServerHost, pending: Pending) {
    match pending {
        Pending::Teleport {
            player,
            name,
            dimension,
            pos,
        } => {
            if player.name(host).is_none() {
                return;
            }
            let target = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
            if player
                .entity()
                .teleport_to_level(host, dimension, target, None)
            {
                let line = teleported_message(&name, dimension, pos);
                let _ = player.tell(host, Text::literal(line));
            } else {
                let line = teleport_failed_message(&name);
                let _ = player.tell(host, Text::literal(line));
            }
        }
        Pending::Unstuck { player, name } => {
            let Some(dimension) = player.dimension(host).map(|id| id.to_string()) else {
                return;
            };
            let Some([x, _, z]) = player.entity().position(host) else {
                return;
            };
            let block_x = x.floor() as i32;
            let block_z = z.floor() as i32;
            let column = sample_column(host, &dimension, block_x, block_z);
            let Some(y) = safe_spot(&column) else {
                let line = no_safe_spot_message(&name);
                let _ = player.tell(host, Text::literal(line));
                return;
            };
            // `/tp` with `.5` coordinates centres the player in the block, which
            // is what 0.1.0's console line did.
            let target = [
                f64::from(block_x) + 0.5,
                f64::from(y),
                f64::from(block_z) + 0.5,
            ];
            if player
                .entity()
                .teleport_to_level(host, &dimension, target, None)
            {
                let line = unstuck_message(&name, [block_x, y, block_z]);
                let _ = player.tell(host, Text::literal(line));
            } else {
                let line = teleport_failed_message(&name);
                let _ = player.tell(host, Text::literal(line));
            }
        }
    }
}

/// Sample the player's whole column. A block that cannot be read (an unloaded
/// chunk, a position outside the world) is neither solid nor passable, so it never
/// becomes a landing spot and breaks the column as 0.1.0's gap rule did.
fn sample_column(host: &mut dyn ServerHost, dimension: &str, x: i32, z: i32) -> Vec<ColumnBlock> {
    let Some(level) = host.level(dimension) else {
        return Vec::new();
    };
    let (min_y, max_y) = dimension_bounds(dimension);
    let mut column = Vec::with_capacity((max_y - min_y) as usize);
    for y in (min_y..max_y).rev() {
        let pos = [x, y, z];
        let Some(state) = level.block_state(host, pos) else {
            column.push(ColumnBlock {
                y,
                solid: false,
                passable: false,
            });
            continue;
        };
        // 0.1.0 asked the block facts for a full cube; the state's own collision
        // shape is the rule a player actually stands on, so a slab or a pane is
        // solid here and a landing spot is never inside one.
        let solid = level
            .block_shape(host, state, pos, BlockShapeKind::Collision)
            .is_some_and(|boxes| !boxes.is_empty());
        let fluid = level
            .fluid_height(host, pos)
            .is_some_and(|height| height > 0.0);
        column.push(ColumnBlock {
            y,
            solid,
            passable: !solid && !fluid,
        });
    }
    column
}

/// The item entities of `dimension`: the level's own list, the box covering it.
fn item_entities(host: &mut dyn ServerHost, dimension: &str) -> Vec<EntityRef> {
    let Some(level) = host.level(dimension) else {
        return Vec::new();
    };
    let Some(entities) = level.entities_in(host, [-ITEM_BOX; 3], [ITEM_BOX; 3]) else {
        return Vec::new();
    };
    entities
        .into_iter()
        .filter(|entity| host.item_entity(*entity).is_some())
        .collect()
}

/// Count the item entities of `dimension`.
fn count_items(host: &mut dyn ServerHost, dimension: &str) -> usize {
    item_entities(host, dimension).len()
}

/// Remove every item entity in `dimension`, returning how many went.
fn clear_items(host: &mut dyn ServerHost, dimension: &str) -> usize {
    let mut removed = 0;
    for entity in item_entities(host, dimension) {
        if entity.discard(host) {
            removed += 1;
        }
    }
    removed
}

/// Remove every tracked entity of `dimension` matching `filter`; returns how many
/// went. Everything leaves without loot (`Entity.discard`), as upstream's `e
/// remove` does.
fn remove_entities(
    host: &mut dyn ServerHost,
    tracked: &mut BTreeMap<u32, (String, String)>,
    dimension: &str,
    filter: &str,
) -> usize {
    // Item entities are the level's own list; the item helper is their removal
    // path, exactly as `/cu e remove items` uses it.
    if kind_matches("minecraft:item", filter) {
        return clear_items(host, dimension);
    }
    let ids = matching_ids(host, tracked, dimension, filter);
    let mut removed = 0;
    for id in ids {
        let Some(entity) = host.entity(id) else {
            tracked.remove(&id);
            continue;
        };
        if entity.discard(host) {
            tracked.remove(&id);
            removed += 1;
        } else if host.entity(id).is_none() {
            // Gone since it last ticked (died, unloaded): forget it.
            tracked.remove(&id);
        }
    }
    removed
}

/// Print each matching entity's saved NBT as SNBT, one chat line each; returns
/// how many were printed.
fn entity_data(
    host: &mut dyn ServerHost,
    tracked: &mut BTreeMap<u32, (String, String)>,
    dimension: &str,
    filter: &str,
) -> usize {
    let ids = matching_ids(host, tracked, dimension, filter);
    let mut printed = 0;
    for id in ids {
        let Some(kind) = tracked.get(&id).map(|(_, kind)| kind.clone()) else {
            continue;
        };
        let Some(entity) = host.entity(id) else {
            tracked.remove(&id);
            continue;
        };
        let Some(nbt) = entity.nbt(host) else {
            continue;
        };
        let text = nbt_text::to_snbt(&Tag::Compound(nbt));
        feedback(
            host,
            &entity_data_message(id, &kind, &short(&text, NBT_LINE)),
        );
        printed += 1;
    }
    printed
}

/// List the entities of `dimension` whose **custom** name holds `needle`
/// (case-insensitive); returns how many were found.
fn find_entities(
    host: &mut dyn ServerHost,
    tracked: &mut BTreeMap<u32, (String, String)>,
    dimension: &str,
    needle: &str,
) -> usize {
    let ids: Vec<u32> = tracked
        .iter()
        .filter(|(_, (known, _))| known == dimension)
        .map(|(id, _)| *id)
        .collect();
    let needle = needle.to_lowercase();
    let mut found = 0;
    for id in ids {
        let Some(kind) = tracked.get(&id).map(|(_, kind)| kind.clone()) else {
            continue;
        };
        let Some(entity) = host.entity(id) else {
            tracked.remove(&id);
            continue;
        };
        // A custom name only: `EntityRef::name` falls back to the type's name,
        // which would make `/cu e find sheep` list every sheep.
        let Some(name) = entity.nbt(host).as_ref().and_then(nbt_text::custom_name) else {
            continue;
        };
        if !name.to_lowercase().contains(&needle) {
            continue;
        }
        let pos = entity.position(host).unwrap_or([0.0; 3]);
        feedback(
            host,
            &entity_found_message(id, &kind, pos, Some(name.as_str())),
        );
        found += 1;
    }
    found
}

/// The tracked ids of `dimension` whose kind matches `filter`.
fn matching_ids(
    host: &mut dyn ServerHost,
    tracked: &BTreeMap<u32, (String, String)>,
    dimension: &str,
    filter: &str,
) -> Vec<u32> {
    tracked
        .iter()
        .filter(|(_, (known, _))| known == dimension)
        .filter(|(id, (_, kind))| kind_matches_filter(host, **id, kind, filter))
        .map(|(id, _)| *id)
        .collect()
}

/// `hostile` is every monster category; anything else is a kind (a bare word
/// matches the `minecraft:` kind of the same name).
fn kind_matches_filter(host: &mut dyn ServerHost, id: u32, kind: &str, filter: &str) -> bool {
    if filter == "hostile" {
        host.entity(id)
            .and_then(|entity| entity.classification(host))
            .is_some_and(|category| category == MobCategory::Monster)
    } else {
        kind_matches(kind, filter)
    }
}

/// A chat line to whoever ran the command.
fn feedback(host: &mut dyn ServerHost, line: &str) {
    let _ = host.commands().feedback(Text::literal(line));
}
