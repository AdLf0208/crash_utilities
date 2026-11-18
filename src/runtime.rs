// Crash Utilities for Horizon — command parsing and clear-cycle rules (pure).
//
// Ported from Crash Utilities 9.0.4 (Darkere/CrashUtilities, MIT) for
// Minecraft 1.21.1 NeoForge. Upstream source read for behaviour only; this is a
// clean-room Rust rewrite.
//
// Slice 1: `/cu e remove items` (remove every item entity in the player's
// level) and `/cu help`.
//
// Slice 2: `/cu callItemClear` (the config-driven countdown that clears ground
// items), `/cu tp <dimension> <x> <y> <z>` and `/cu unstuck`. The game glue
// queues the teleports and runs them from `server_tick`, because the pin
// refuses `native_run_command` inside a player hook.
//
// Slice 3: `/cu e remove <type|hostile>`, `/cu e entityData <type>` and
// `/cu e find <name>`, over the entities the glue tracks from
// `entity_tick_pre` (API 146).

/// What a command line asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action<'a> {
    RemoveItems,
    /// `/cu e remove <type>`: every non-player entity of that kind (or
    /// `hostile`) in the acting player's level.
    RemoveEntities {
        filter: &'a str,
    },
    /// `/cu e entityData <type>`: each matching entity's saved NBT.
    EntityData {
        filter: &'a str,
    },
    /// `/cu e find <name>`: entities whose custom name holds `needle`.
    FindEntities {
        needle: &'a str,
    },
    Help,
    /// `callItemClear` (and the lowercase spelling the pin's command tree
    /// accepts as a literal).
    CallItemClear,
    /// A well-formed teleport. `dimension` is the canonical id.
    Teleport {
        dimension: &'static str,
        pos: [i32; 3],
    },
    Unstuck,
    /// `tp` named a dimension this pin does not have.
    BadDimension(&'a str),
    /// A recognised command with unusable arguments.
    Usage(&'static str),
    Unknown,
}

/// The roots upstream registers (`CommandUtils.java:21`, `CrashUtils.java:107`).
pub const ROOTS: [&str; 2] = ["cu", "crashutilities"];

/// The dimensions the pin knows (`native_server_block` resolves exactly these).
pub const DIMENSIONS: [(&str, &str); 3] = [
    ("overworld", "minecraft:overworld"),
    ("the_nether", "minecraft:the_nether"),
    ("the_end", "minecraft:the_end"),
];

/// Usage line for a malformed `/cu tp`.
pub const TP_USAGE: &str = "usage: /cu tp <overworld|the_nether|the_end> <x> <y> <z>";

/// Usage line for a malformed `/cu e …`.
pub const ENTITY_USAGE: &str =
    "usage: /cu e remove <type|hostile> | /cu e entityData <type> | /cu e find <name>";

/// The canonical id of a dimension word, with or without `minecraft:`.
pub fn dimension_id(word: &str) -> Option<&'static str> {
    let bare = word.strip_prefix("minecraft:").unwrap_or(word);
    DIMENSIONS
        .iter()
        .find(|(name, _)| *name == bare)
        .map(|(_, id)| *id)
}

/// Parse a command line (no leading slash, as the hook receives it).
pub fn parse(line: &str) -> Action<'_> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let Some((root, rest)) = words.split_first() else {
        return Action::Unknown;
    };
    if !ROOTS.contains(root) || rest.is_empty() {
        return Action::Unknown;
    }
    if rest == ["help"] {
        return Action::Help;
    }
    if rest == ["callItemClear"] || rest == ["callitemclear"] {
        return Action::CallItemClear;
    }
    if rest == ["unstuck"] {
        return Action::Unstuck;
    }
    let is_entities = matches!(rest[0], "e" | "entities" | "entity");
    if is_entities {
        if rest.len() == 3 && rest[1] == "remove" {
            if rest[2] == "items" {
                return Action::RemoveItems;
            }
            // A kind word (`zombie`, `minecraft:zombie`) or `hostile`.
            return match entity_word(rest[2]) {
                Some(filter) => Action::RemoveEntities { filter },
                None => Action::Usage(ENTITY_USAGE),
            };
        }
        // The tree's literal is lowercase (`entityData` is not a valid
        // literal); both spellings reach the handler.
        if rest.len() == 3 && matches!(rest[1], "entityData" | "entitydata") {
            return match entity_word(rest[2]) {
                Some(filter) => Action::EntityData { filter },
                None => Action::Usage(ENTITY_USAGE),
            };
        }
        if rest.len() == 3 && rest[1] == "find" {
            return match name_word(rest[2]) {
                Some(needle) => Action::FindEntities { needle },
                None => Action::Usage(ENTITY_USAGE),
            };
        }
    }
    if rest[0] == "tp" {
        return parse_tp(rest);
    }
    Action::Unknown
}

/// Whether `word` can name an entity kind (lowercase id characters, optionally
/// namespaced) or the `hostile` group.
fn entity_word(word: &str) -> Option<&str> {
    let ok = !word.is_empty()
        && word.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.:-".contains(&byte)
        });
    ok.then_some(word)
}

/// Whether `word` can name an entity to find: a player or custom name, one
/// word (`Alex`, `Flossy_2`).
fn name_word(word: &str) -> Option<&str> {
    let ok = !word.is_empty()
        && word
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte));
    ok.then_some(word)
}

/// Removes the optional `minecraft:` namespace from a kind.
fn bare(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

/// Whether an entity kind matches a `/cu e remove|entityData` filter: a bare
/// word matches the `minecraft:` kind of the same name, a namespaced one must
/// match exactly.
pub fn kind_matches(kind: &str, filter: &str) -> bool {
    bare(kind) == bare(filter)
}

fn parse_tp<'a>(rest: &[&'a str]) -> Action<'a> {
    if rest.len() != 5 {
        return Action::Usage(TP_USAGE);
    }
    let Some(dimension) = dimension_id(rest[1]) else {
        return Action::BadDimension(rest[1]);
    };
    let (Ok(x), Ok(y), Ok(z)) = (
        rest[2].parse::<i32>(),
        rest[3].parse::<i32>(),
        rest[4].parse::<i32>(),
    ) else {
        return Action::Usage(TP_USAGE);
    };
    // Vanilla's world border and height limits: refuse a teleport no level can
    // hold instead of queueing a command that cannot run.
    if !(-30_000_000..=30_000_000).contains(&x)
        || !(-2048..=2048).contains(&y)
        || !(-30_000_000..=30_000_000).contains(&z)
    {
        return Action::Usage(TP_USAGE);
    }
    Action::Teleport {
        dimension,
        pos: [x, y, z],
    }
}

/// Help lines for the commands this slice actually implements (upstream's own
/// list is stale and advertises commands that are not ported).
pub const HELP: [&str; 10] = [
    "Crash Utilities:",
    "  /cu e remove items - remove every item entity in your level",
    "  /cu e remove <type|hostile> - remove every entity of that kind",
    "  /cu e entityData <type> - print each matching entity's saved NBT",
    "  /cu e find <name> - list entities whose custom name holds <name>",
    "  /cu callItemClear - count items and clear them after the configured countdown",
    "  /cu tp <overworld|the_nether|the_end> <x> <y> <z> - teleport yourself",
    "  /cu unstuck - move yourself to the highest safe spot in your column",
    "  /cu help - this message",
    "  (aliases: /crashutilities, /cu entities, /cu entity)",
];

/// The message upstream prints after a manual item clear.
pub fn removed_message(count: usize) -> String {
    format!("Removed {count} Entities")
}

/// The line `/cu e find` prints for one entity.
pub fn entity_found_message(id: u32, kind: &str, pos: [f64; 3], name: Option<&str>) -> String {
    let at = format!("{:.1} {:.1} {:.1}", pos[0], pos[1], pos[2]);
    match name {
        Some(name) => format!("{id} {kind} at {at} name \"{name}\""),
        None => format!("{id} {kind} at {at}"),
    }
}

/// The line `/cu e entityData` prints for one entity: its id, kind and NBT as
/// SNBT (the glue caps the text so a long compound still fits in chat).
pub fn entity_data_message(id: u32, kind: &str, snbt: &str) -> String {
    format!("{id} {kind} {snbt}")
}

/// What `/cu e entityData` and `/cu e find` print when nothing matched.
pub fn no_entities_message(filter: &str) -> String {
    format!("No entities matching {filter} found")
}

/// `text` cut to at most `max_chars` characters (a trailing `...` counts),
/// never splitting a character: a chat line stays usable for a long compound.
pub fn short(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let keep = max_chars.saturating_sub(3);
    let mut out: String = text.chars().take(keep).collect();
    out.push_str("...");
    out
}

// ---- item-clear configuration (upstream `ServerConfig.java:30-39`) ----

/// The item-clear settings. Defaults are upstream 9.0.4's: disabled, a
/// 30-minute check, a 1000-item limit, warnings 20 and 5 seconds before the
/// clear, and the `ITEMCLEAR INCOMING!` title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub enabled: bool,
    pub timer_minutes: u32,
    pub maximum: usize,
    /// Seconds remaining at each warning, ascending.
    pub warnings: Vec<u32>,
    pub title: bool,
    pub title_text: String,
    pub warning_text: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: false,
            timer_minutes: 30,
            maximum: 1000,
            warnings: vec![5, 20],
            title: true,
            title_text: "ITEMCLEAR INCOMING!".to_owned(),
            warning_text: "ITEMCLEAR IN % SECONDS".to_owned(),
        }
    }
}

/// Parse the port's `key = value` config. Bad values keep their defaults and
/// are reported; unknown keys are reported too.
pub fn parse_config(input: &str) -> (Config, Vec<String>) {
    let mut config = Config::default();
    let mut warnings = Vec::new();
    for raw in input.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            warnings.push(format!("ignored line: {line}"));
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "enabled" => match parse_bool(value) {
                Some(parsed) => config.enabled = parsed,
                None => warnings.push(format!("enabled: not a boolean: {value}")),
            },
            "timer" => match value.parse::<u32>() {
                Ok(parsed) if parsed <= 1440 => config.timer_minutes = parsed,
                Ok(parsed) => {
                    config.timer_minutes = 1440;
                    warnings.push(format!("timer: clamped from {parsed}"));
                }
                Err(_) => warnings.push(format!("timer: not a number: {value}")),
            },
            "maximum" => match value.parse::<usize>() {
                Ok(parsed) => config.maximum = parsed,
                Err(_) => warnings.push(format!("maximum: not a number: {value}")),
            },
            "warnings" => {
                let text = strip_quotes(value).unwrap_or(value);
                let (parsed, problems) = parse_warnings(text);
                config.warnings = parsed;
                warnings.extend(problems);
            }
            "title" => match parse_bool(value) {
                Some(parsed) => config.title = parsed,
                None => warnings.push(format!("title: not a boolean: {value}")),
            },
            "titletext" => match strip_quotes(value) {
                Some(parsed) => config.title_text = parsed.to_owned(),
                None => warnings.push(format!("titletext: not a string: {value}")),
            },
            "warningtext" => match strip_quotes(value) {
                Some(parsed) => config.warning_text = parsed.to_owned(),
                None => warnings.push(format!("warningtext: not a string: {value}")),
            },
            _ => warnings.push(format!("unknown key: {key}")),
        }
    }
    (config, warnings)
}

/// Write the config back, with the upstream defaults as comments.
pub fn to_toml(config: &Config) -> String {
    let warnings = config
        .warnings
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let mut out = String::new();
    out.push_str("# Crash Utilities — item clear (MIT port of 9.0.4)\n");
    out.push_str("# enabled=false is upstream's default; set it to true to allow clears.\n");
    out.push_str(&format!("enabled = {}\n", config.enabled));
    out.push_str("# Minutes between the background checks (0 = every 10 seconds).\n");
    out.push_str(&format!("timer = {}\n", config.timer_minutes));
    out.push_str("# A clear starts when the level holds at least this many items.\n");
    out.push_str(&format!("maximum = {}\n", config.maximum));
    out.push_str("# Seconds remaining at each chat warning, comma separated.\n");
    out.push_str(&format!("warnings = \"{warnings}\"\n"));
    out.push_str("# Show the countdown title when a clear starts.\n");
    out.push_str(&format!("title = {}\n", config.title));
    out.push_str(&format!("titletext = \"{}\"\n", config.title_text));
    out.push_str("# The first % is replaced by the seconds remaining.\n");
    out.push_str(&format!("warningtext = \"{}\"\n", config.warning_text));
    out
}

/// Parse the `warnings` list: seconds remaining, ascending and deduplicated.
/// An empty or unusable list falls back to upstream's `5,20`.
pub fn parse_warnings(input: &str) -> (Vec<u32>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut problems = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.parse::<u32>() {
            Ok(seconds) => warnings.push(seconds),
            Err(_) => problems.push(format!("warnings: not a number: {part}")),
        }
    }
    warnings.sort_unstable();
    warnings.dedup();
    if warnings.is_empty() {
        problems.push("warnings: empty, using 5,20".to_owned());
        warnings = vec![5, 20];
    }
    (warnings, problems)
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn strip_quotes(value: &str) -> Option<&str> {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
}

// ---- the countdown (upstream `ClearItemTask.java:71-106`) ----

/// Minecraft's fixed tick rate: the countdown is measured in server ticks.
pub const TICKS_PER_SECOND: u64 = 20;

/// One chat warning: fire at `due_tick`, saying `seconds` remain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduledWarning {
    pub due_tick: u64,
    pub seconds: u32,
}

/// The warning/clear schedule for a list of ascending warning seconds. The
/// last warning is the clear deadline (upstream `ClearItemTask.java:72`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClearSchedule {
    pub total_ticks: u64,
    pub warnings: Vec<ScheduledWarning>,
}

pub fn clear_schedule(warnings: &[u32]) -> Option<ClearSchedule> {
    let last = *warnings.last()?;
    let total_ticks = u64::from(last) * TICKS_PER_SECOND;
    let mut warnings: Vec<ScheduledWarning> = warnings
        .iter()
        .map(|&seconds| ScheduledWarning {
            due_tick: u64::from(last.saturating_sub(seconds)) * TICKS_PER_SECOND,
            seconds,
        })
        .collect();
    warnings.sort_by_key(|warning| warning.due_tick);
    Some(ClearSchedule {
        total_ticks,
        warnings,
    })
}

/// Ticks between background checks: `timer` minutes, with upstream's
/// `timer = 0` meaning ten seconds (`ClearItemTask.java:51-54`).
pub fn period_ticks(minutes: u32) -> u64 {
    if minutes == 0 {
        10 * TICKS_PER_SECOND
    } else {
        u64::from(minutes) * 60 * TICKS_PER_SECOND
    }
}

// ---- messages ----

pub fn disabled_message() -> &'static str {
    "ItemClears are not enabled in the config"
}

pub fn count_message(count: usize, maximum: usize) -> String {
    format!("{count} Item Entities in World. Limit is set to {maximum}")
}

pub fn warning_message(template: &str, seconds: u32) -> String {
    template.replacen('%', &seconds.to_string(), 1)
}

pub fn cleared_message(count: usize) -> String {
    format!("{count} Items cleared")
}

pub fn prevented_message(count: usize) -> String {
    format!("Item Clear prevented. Only {count} items on the ground")
}

pub fn already_scheduled_message() -> &'static str {
    "Item clear already scheduled"
}

pub fn teleporting_message(name: &str, dimension: &str, pos: [i32; 3]) -> String {
    format!(
        "Teleporting {name} to {dimension} {} {} {}",
        pos[0], pos[1], pos[2]
    )
}

pub fn teleported_message(name: &str, dimension: &str, pos: [i32; 3]) -> String {
    format!(
        "Teleported {name} to {dimension} {} {} {}",
        pos[0], pos[1], pos[2]
    )
}

pub fn teleport_failed_message(name: &str) -> String {
    format!("Unable to teleport {name}")
}

pub fn unknown_dimension_message(word: &str) -> String {
    format!("Unknown dimension {word}")
}

pub fn unstuck_message(name: &str, pos: [i32; 3]) -> String {
    format!(
        "Sent Player {name} to a safe spot at {} {} {}",
        pos[0], pos[1], pos[2]
    )
}

pub fn no_safe_spot_message(name: &str) -> String {
    format!("No safe spot found for {name}")
}

pub fn unsticking_message(name: &str) -> String {
    format!("Unsticking {name}")
}

/// The playable range of the pin's three dimensions: `(min_y, max_y)` with an
/// exclusive top. The Nether's bedrock ceiling (y=127) is outside the logical
/// height, so a safe spot is never placed on top of it
/// (`LevelShape::OVERWORLD`/`NETHER`/`END`).
pub fn dimension_bounds(dimension: &str) -> (i32, i32) {
    match dimension {
        "minecraft:the_nether" => (0, 128),
        "minecraft:the_end" => (0, 256),
        _ => (-64, 320),
    }
}

/// The vanilla title command upstream runs when the countdown starts
/// (`ClearItemTask.java:109-115`).
pub fn title_command(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("title @a title {{\"text\":\"{escaped}\",\"color\":\"dark_red\"}}")
}

// ---- unstuck: the safe-spot rule ----

/// One sampled block of a column. `passable` is true for air and blocks that
/// do not collide and hold no fluid; `solid` is a full-collision floor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnBlock {
    pub y: i32,
    pub solid: bool,
    pub passable: bool,
}

/// The highest standing place in `column`: feet and head passable with a solid
/// block directly below. Samples must be contiguous in y; a gap (an unloaded
/// chunk) is not a safe spot.
pub fn safe_spot(column: &[ColumnBlock]) -> Option<i32> {
    let mut blocks = column.to_vec();
    blocks.sort_by_key(|block| std::cmp::Reverse(block.y));
    for window in blocks.windows(3) {
        let [head, feet, floor] = window else {
            continue;
        };
        if head.y == feet.y + 1
            && feet.y == floor.y + 1
            && head.passable
            && feet.passable
            && floor.solid
        {
            return Some(feet.y);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_roots_and_entity_aliases_are_accepted() {
        for root in ROOTS {
            for entities in ["e", "entities", "entity"] {
                assert_eq!(
                    parse(&format!("{root} {entities} remove items")),
                    Action::RemoveItems
                );
            }
            assert_eq!(parse(&format!("{root} help")), Action::Help);
        }
    }

    #[test]
    fn unrelated_commands_are_not_claimed() {
        for line in [
            "",
            "cu",
            "cu e",
            "cu e remove",
            "cu remove items",
            "cu e remove blocks",
            "time set noon",
            "cup e remove items",
        ] {
            // `cu e remove blocks` is a kind filter now (a valid word that
            // matches no entity), so it is claimed; keep it out below.
            if line == "cu e remove blocks" {
                assert_eq!(
                    parse(line),
                    Action::RemoveEntities { filter: "blocks" },
                    "{line}"
                );
                continue;
            }
            assert_eq!(parse(line), Action::Unknown, "{line}");
        }
    }

    #[test]
    fn extra_arguments_are_not_part_of_the_slice() {
        // Upstream's `items [force] [name]` quirks are not implemented; do not
        // claim lines we would silently misinterpret.
        assert_eq!(parse("cu e remove items force"), Action::Unknown);
        assert_eq!(parse("cu e remove items x Stone"), Action::Unknown);
        assert_eq!(parse("cu help extra"), Action::Unknown);
    }

    #[test]
    fn entity_commands_parse_a_kind_a_group_or_a_name() {
        for (line, expected) in [
            (
                "cu e remove zombie",
                Action::RemoveEntities { filter: "zombie" },
            ),
            (
                "crashutilities entities remove minecraft:zombie",
                Action::RemoveEntities {
                    filter: "minecraft:zombie",
                },
            ),
            (
                "cu entity remove hostile",
                Action::RemoveEntities { filter: "hostile" },
            ),
            (
                "cu e entityData sheep",
                Action::EntityData { filter: "sheep" },
            ),
            (
                "cu e entitydata minecraft:sheep",
                Action::EntityData {
                    filter: "minecraft:sheep",
                },
            ),
            (
                "cu e find Flossy",
                Action::FindEntities { needle: "Flossy" },
            ),
        ] {
            assert_eq!(parse(line), expected, "{line}");
        }
        // A kind word that cannot be an id, or no word at all: the command is
        // recognised, the argument is not.
        for line in [
            "cu e remove Zombie",
            "cu e remove zombie!",
            "cu e entityData Sheep",
            "cu e find tw$o",
        ] {
            assert_eq!(parse(line), Action::Usage(ENTITY_USAGE), "{line}");
        }
        // Incomplete or extra words stay unclaimed, as before.
        for line in [
            "cu e remove",
            "cu e entityData",
            "cu e find",
            "cu e find two words",
            "cu e remove items force",
        ] {
            assert_eq!(parse(line), Action::Unknown, "{line}");
        }
        // `remove items` keeps its own action.
        assert_eq!(parse("cu e remove items"), Action::RemoveItems);
    }

    #[test]
    fn entity_kind_matching_ignores_the_vanilla_namespace() {
        assert!(kind_matches("minecraft:sheep", "sheep"));
        assert!(kind_matches("sheep", "minecraft:sheep"));
        assert!(kind_matches("minecraft:sheep", "minecraft:sheep"));
        assert!(!kind_matches("minecraft:sheep", "cow"));
        assert!(!kind_matches("minecraft:sheep", "shee"));
    }

    #[test]
    fn entity_messages_name_the_entity() {
        assert_eq!(
            entity_found_message(42, "minecraft:sheep", [1.5, 64.0, -2.5], Some("Flossy")),
            "42 minecraft:sheep at 1.5 64.0 -2.5 name \"Flossy\""
        );
        assert_eq!(
            entity_found_message(7, "minecraft:pig", [0.0, 70.0, 0.0], None),
            "7 minecraft:pig at 0.0 70.0 0.0"
        );
        assert_eq!(
            entity_data_message(42, "minecraft:sheep", "{CustomName:\"Flossy\"}"),
            "42 minecraft:sheep {CustomName:\"Flossy\"}"
        );
        assert_eq!(
            no_entities_message("sheep"),
            "No entities matching sheep found"
        );
        assert_eq!(short("short", 10), "short");
        assert_eq!(short("abcdefghij", 10), "abcdefghij");
        assert_eq!(short("abcdefghijk", 10), "abcdefg...");
        // Characters, not bytes: a multi-byte name is never split.
        assert_eq!(short("αααααα", 5), "αα...");
    }

    #[test]
    fn the_message_matches_upstream_and_counts() {
        assert_eq!(removed_message(0), "Removed 0 Entities");
        assert_eq!(removed_message(5), "Removed 5 Entities");
    }

    #[test]
    fn call_item_clear_accepts_the_upstream_and_tree_spellings() {
        for root in ROOTS {
            assert_eq!(
                parse(&format!("{root} callItemClear")),
                Action::CallItemClear
            );
            assert_eq!(
                parse(&format!("{root} callitemclear")),
                Action::CallItemClear
            );
            assert_eq!(parse(&format!("{root} unstuck")), Action::Unstuck);
        }
    }

    #[test]
    fn tp_parses_a_dimension_and_a_block_position() {
        assert_eq!(
            parse("cu tp the_nether 0 70 0"),
            Action::Teleport {
                dimension: "minecraft:the_nether",
                pos: [0, 70, 0]
            }
        );
        assert_eq!(
            parse("crashutilities tp minecraft:the_end -3 64 12"),
            Action::Teleport {
                dimension: "minecraft:the_end",
                pos: [-3, 64, 12]
            }
        );
        assert_eq!(
            parse("cu tp overworld 1 2 3"),
            Action::Teleport {
                dimension: "minecraft:overworld",
                pos: [1, 2, 3]
            }
        );
    }

    #[test]
    fn tp_reports_an_unknown_dimension() {
        assert_eq!(
            parse("cu tp nowhere 0 70 0"),
            Action::BadDimension("nowhere")
        );
        assert_eq!(
            parse("cu tp minecraft:nowhere 0 70 0"),
            Action::BadDimension("minecraft:nowhere")
        );
    }

    #[test]
    fn tp_rejects_bad_arguments() {
        for line in [
            "cu tp",
            "cu tp the_nether",
            "cu tp the_nether 0 70",
            "cu tp the_nether 0 70 0 9",
            "cu tp the_nether x 70 0",
            "cu tp the_nether 0 70 0.5",
            "cu tp the_nether 40000000 70 0",
        ] {
            assert_eq!(parse(line), Action::Usage(TP_USAGE), "{line}");
        }
    }

    #[test]
    fn config_defaults_match_upstream_9_0_4() {
        let config = Config::default();
        assert!(!config.enabled);
        assert_eq!(config.timer_minutes, 30);
        assert_eq!(config.maximum, 1000);
        assert_eq!(config.warnings, vec![5, 20]);
        assert!(config.title);
        assert_eq!(config.title_text, "ITEMCLEAR INCOMING!");
        assert_eq!(config.warning_text, "ITEMCLEAR IN % SECONDS");
    }

    #[test]
    fn config_parses_and_round_trips() {
        let (config, warnings) = parse_config(
            "enabled = true\ntimer = 5\nmaximum = 12\nwarnings = \"20,5\"\ntitle = false\ntitletext = \"CLEAR\"\nwarningtext = \"% LEFT\"\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(config.enabled);
        assert_eq!(config.timer_minutes, 5);
        assert_eq!(config.maximum, 12);
        assert_eq!(config.warnings, vec![5, 20]);
        assert!(!config.title);
        assert_eq!(config.title_text, "CLEAR");
        assert_eq!(config.warning_text, "% LEFT");
        let (again, warnings) = parse_config(&to_toml(&config));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(again, config);
    }

    #[test]
    fn bad_config_values_keep_defaults_and_warn() {
        let (config, warnings) =
            parse_config("enabled = maybe\nmaximum = -1\nwarnings = \"x,5\"\n");
        assert!(!config.enabled);
        assert_eq!(config.maximum, 1000);
        assert_eq!(config.warnings, vec![5]);
        assert_eq!(warnings.len(), 3, "{warnings:?}");
    }

    #[test]
    fn warnings_sort_and_deduplicate() {
        let (warnings, problems) = parse_warnings("20, 5, 5, 12");
        assert_eq!(warnings, vec![5, 12, 20]);
        assert!(problems.is_empty());
        let (warnings, problems) = parse_warnings("");
        assert_eq!(warnings, vec![5, 20]);
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn the_clear_schedule_fires_warnings_then_clears() {
        let schedule = clear_schedule(&[5, 20]).expect("a schedule");
        assert_eq!(schedule.total_ticks, 400);
        assert_eq!(
            schedule.warnings,
            vec![
                ScheduledWarning {
                    due_tick: 0,
                    seconds: 20
                },
                ScheduledWarning {
                    due_tick: 300,
                    seconds: 5
                },
            ]
        );
        assert_eq!(clear_schedule(&[]), None);
    }

    #[test]
    fn messages_match_upstream_9_0_4() {
        assert_eq!(
            disabled_message(),
            "ItemClears are not enabled in the config"
        );
        assert_eq!(
            count_message(5, 1000),
            "5 Item Entities in World. Limit is set to 1000"
        );
        assert_eq!(
            warning_message("ITEMCLEAR IN % SECONDS", 20),
            "ITEMCLEAR IN 20 SECONDS"
        );
        assert_eq!(cleared_message(5), "5 Items cleared");
        assert_eq!(
            prevented_message(2),
            "Item Clear prevented. Only 2 items on the ground"
        );
        assert_eq!(
            teleported_message("Alex", "minecraft:the_nether", [0, 70, 0]),
            "Teleported Alex to minecraft:the_nether 0 70 0"
        );
        assert_eq!(
            unknown_dimension_message("nowhere"),
            "Unknown dimension nowhere"
        );
        assert_eq!(
            unstuck_message("Alex", [0, 64, 0]),
            "Sent Player Alex to a safe spot at 0 64 0"
        );
        assert_eq!(unsticking_message("Alex"), "Unsticking Alex");
    }

    #[test]
    fn dimension_bounds_match_the_pins_level_shapes() {
        assert_eq!(dimension_bounds("minecraft:overworld"), (-64, 320));
        assert_eq!(dimension_bounds("minecraft:the_nether"), (0, 128));
        assert_eq!(dimension_bounds("minecraft:the_end"), (0, 256));
    }

    #[test]
    fn safe_spot_picks_the_highest_standing_place() {
        let mut column = Vec::new();
        for y in -64..=100 {
            let solid = y <= 63 || (70..=71).contains(&y);
            column.push(ColumnBlock {
                y,
                solid,
                passable: !solid,
            });
        }
        assert_eq!(safe_spot(&column), Some(72));
    }

    #[test]
    fn safe_spot_rejects_fluid_and_missing_floor() {
        let lava = vec![
            ColumnBlock {
                y: 62,
                solid: false,
                passable: false,
            },
            ColumnBlock {
                y: 63,
                solid: true,
                passable: false,
            },
            ColumnBlock {
                y: 64,
                solid: false,
                passable: false,
            },
            ColumnBlock {
                y: 65,
                solid: false,
                passable: true,
            },
            ColumnBlock {
                y: 66,
                solid: false,
                passable: true,
            },
        ];
        assert_eq!(safe_spot(&lava), None);
        let void = vec![
            ColumnBlock {
                y: 63,
                solid: false,
                passable: true,
            },
            ColumnBlock {
                y: 64,
                solid: false,
                passable: true,
            },
            ColumnBlock {
                y: 65,
                solid: false,
                passable: true,
            },
        ];
        assert_eq!(safe_spot(&void), None);
    }

    #[test]
    fn the_periodic_check_matches_upstream() {
        assert_eq!(period_ticks(0), 200);
        assert_eq!(period_ticks(30), 36_000);
    }

    #[test]
    fn the_title_command_escapes_its_text() {
        assert_eq!(
            title_command("ITEMCLEAR INCOMING!"),
            "title @a title {\"text\":\"ITEMCLEAR INCOMING!\",\"color\":\"dark_red\"}"
        );
        assert_eq!(
            title_command("say \"hi\"\\"),
            "title @a title {\"text\":\"say \\\"hi\\\"\\\\\",\"color\":\"dark_red\"}"
        );
    }
}
