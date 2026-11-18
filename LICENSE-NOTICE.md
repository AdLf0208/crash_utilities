# Licence provenance

- Mod: `crashutilities` (Crash Utilities) by **Darkere** —
  <https://github.com/Darkere/CrashUtilities>
- Licence: **MIT**, read from the mod's own jar,
  `ports/_shared/licence-cache/crashutilities-9.0.4.jar`, whose
  `META-INF/neoforge.mods.toml` declares `license = "MIT"` and `license = "MIT LICENSE"`
  (the campaign's `licence-audit.csv` records the same row: `crashutilities`,
  rank 0, permissive). Rank 0: publishable with this source, and **local-only**
  until the maintainer approves publishing (ARCHITECTURE.md §8).
- Upstream version analysed: **9.0.4**, repository commit
  `8ac8a96341dc06285af501ee83cc7f254a00dbfc` (Minecraft Java 1.21.1, NeoForge
  21.1.43; the repo has no tags and `gradle.properties` reads `mod_version=9.0.4`).
- This port is a **clean-room Rust rewrite** written from the mod's observable
  behaviour, its data files and its public documentation. No Java source was
  copied or translated, and no jar was decompiled.
- Nothing is carried: the mod ships no blocks, items, models or recipes, so the
  package has no `assets/` or `data/` directory. Its only files are the source,
  the manifest, this notice and the two documents the registry packs.
- The complete corresponding source of this package is this directory: `src/`,
  `horizon-mod.toml`, `Cargo.toml` and the `LICENSE` text beside this notice.
