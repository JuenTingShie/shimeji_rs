# Windows Native Shimeji Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a single portable native Windows `.exe` (Rust, no installed runtime) that lets users drag-and-drop or pick a mascot `.zip` (in the `legacy_default_v1` schema, e.g. `8ge8jqm7.zip`), spawns it as a transparent, always-on-top desktop mascot, and animates it per the imported `animation.json` state machine — including walking/climbing along the edges of the screen **and of other running applications' windows**, with multiple mascots running at once.

**Architecture:** A single Rust process runs one Win32 message loop hosting N per-mascot `WS_EX_LAYERED` windows painted via `UpdateLayeredWindow`. A generic, data-driven interpreter executes the `animation.json` schema (no per-mascot code). A throttled `Environment` tracker supplies each mascot's state machine with what surface (screen edge or another window's edge) it's on, each tick.

**Tech Stack:** Rust (stable, MSVC target), `windows` crate for all Win32 interop, `image` (webp feature, pure-Rust `image-webp` decoder — no bundled/system libwebp), `zip`, `serde`/`serde_json`, `rand`, `thiserror` (library errors), `anyhow` (app-glue errors), `tempfile` (test/import scratch dirs).

## Global Constraints

- Windows-only native desktop app; **no installed runtime** (no JVM/.NET/etc. required on the target machine) — the build output is a statically-linked native `.exe`.
- Distribution is a **single portable `.exe`**, no installer.
- WebP sprite decoding uses a **pure-Rust decoder** (`image` crate's `webp` feature, backed by `image-webp`) — no bundled or system `libwebp.dll`.
- **Multiple simultaneous mascot instances** must be supported, same or different imported characters.
- **Full window climbing**: mascots interact with the edges of other running applications' top-level windows, not just the screen bounds.
- Import works via **both** drag-and-drop and a file picker.
- Animation tick rate: **60 Hz**.
- A mascot's "level" (gates `minLevel`-tagged animations/transitions) is **fixed at `manifest.levels`** for every spawned instance — there is no progression/affection system in this version.
- The engine's `IDLE` event fires after **3600 ticks (~60s at 60Hz)** with no drag/tap interaction on that mascot instance.
- Importing a mascot whose slug already exists in the library is **rejected**, not overwritten — the user must remove the existing library entry first.
- **No automated visual/UI testing.** Tasks that touch real Win32 windows, GDI painting, or the tray icon are verified manually against an explicit checklist instead of an automated test — call this out per-task, it's a deliberate scope limit from the spec, not an oversight.

---

## Reference: the `legacy_default_v1` schema (from `tests/fixtures/8ge8jqm7.zip`)

`manifest.json` top-level fields: `schemaVersion`, `name`, `nameSlug`, `category`, `categorySlug`, `description`, `bundleVersion`, `minAppVersion`, `levels`, `origin`, `animationSchema: {path, schemaId, version}`, `sprites: {type, basePath, filePattern, spriteCount, size: [w, h]}`, `preview: {thumbnail}`, `author: {name}`, `license: {type, text, attribution}`.

`animation.json` top-level: `schema_id`, `version`, `default_animation`, `initial_candidates`, `animations[]`, `events[]`.

Each `animations[]` entry: `key`, `type` (`GROUND`/`WALL`/`CEILING`/`AIR`/`USER`), `subtype`, `level`, `loop` (`ONESHOT`/`LOOP`), `direction` (`LEFT`/`RIGHT`/`ANY`), `frames[]` (`sprite`, `dx`, `dy`, `durationTicks`), optional `auto: {onFinish: [ChoiceItem], onTimer: [TimerRule], maxDurationTicks}`, optional `borderTransitions: [{when, facing?, choices: [ChoiceItem]}]`.

`ChoiceItem`: `to`, `weight`, optional `setFacing`, optional `minLevel`.

`TimerRule`: `choices: [ChoiceItem]`, `minTicks`, `maxTicks`, `chance` (0.0-1.0).

`maxDurationTicks` is **either** a plain integer (e.g. `480`) **or** an object `{minTicks, maxTicks}` (e.g. `climb_left`'s `{200, 699}`) — this is a real quirk in the sample fixture, not hypothetical.

`events[]` entries vary by `event` kind (`DRAG_START`/`DRAG_END`/`FLING_START`/`FLING_END`/`JUMP`/`IDLE`/`TAP`): most have `from` (an animation key or `"*"`), optional `when` (edge), optional `facing`, and either a single `to` (+ optional `setFacing`) or a `choices: [ChoiceItem]` list. `TAP` rules instead have `minLevel`, `maxLevel`, `allowedTypes: [SurfaceType]` in place of `from`.

The fixture has **37** `animations[]` entries and **27** `events[]` entries — used as concrete assertions in Task 3's test.

---

### Task 1: Bootstrap Rust project & extract test fixtures

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `src/main.rs`
- Create: `tests/fixtures/8ge8jqm7.zip` (moved from repo root)
- Create: `tests/fixtures/sample_manifest.json`
- Create: `tests/fixtures/sample_animation.json`
- Create: `tests/fixtures/sample_bundle/` (full unzipped bundle: `manifest.json`, `animation.json`, `sprites/*.webp`, `thumbnail.webp`)

**Interfaces:**
- Produces: a buildable `shimeji` binary crate at the repo root, and the three fixture forms every later test task reads from.

- [ ] **Step 1: Confirm/install the Rust toolchain**

Run: `rustc --version && cargo --version`

If either command is not found, install via `winget install --id Rustlang.Rustup -e` (or https://rustup.rs), then open a new shell and re-run the check. Target the default `stable-x86_64-pc-windows-msvc` toolchain — confirm with `rustup show`.

- [ ] **Step 2: Initialize the crate**

```bash
cd "e:/dev/shimeji"
cargo init --name shimeji --vcs none
```

- [ ] **Step 3: Ignore build output**

Create `.gitignore`:

```
/target
```

- [ ] **Step 4: Add dependencies**

```bash
cargo add serde --features derive
cargo add serde_json
cargo add zip
cargo add image --no-default-features --features webp,png
cargo add rand
cargo add thiserror
cargo add anyhow
cargo add tempfile --dev
```

- [ ] **Step 5: Move the test zip into fixtures and extract it two ways**

```bash
mkdir -p tests/fixtures/sample_bundle
git mv 8ge8jqm7.zip tests/fixtures/8ge8jqm7.zip
unzip -p tests/fixtures/8ge8jqm7.zip manifest.json > tests/fixtures/sample_manifest.json
unzip -p tests/fixtures/8ge8jqm7.zip animation.json > tests/fixtures/sample_animation.json
unzip -o tests/fixtures/8ge8jqm7.zip -d tests/fixtures/sample_bundle > /dev/null
```

Verify: `tests/fixtures/sample_bundle/sprites/` contains 70 `.webp` files, and `tests/fixtures/sample_bundle/manifest.json` / `animation.json` exist.

- [ ] **Step 6: Verify the project builds**

Run: `cargo build`
Expected: builds successfully, producing `target/debug/shimeji.exe`.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore src/main.rs tests/fixtures
git commit -m "chore: bootstrap Rust project and extract test fixtures"
```

---

### Task 2: Parse `manifest.json`

**Files:**
- Create: `src/format/mod.rs`
- Create: `src/format/manifest.rs`
- Modify: `src/main.rs:1` (add `mod format;`)

**Interfaces:**
- Consumes: nothing (leaf module).
- Produces: `format::manifest::Manifest` (and nested `AnimationSchemaRef`, `SpriteSheetInfo`, `PreviewInfo`, `AuthorInfo`, `LicenseInfo`), all `#[derive(Debug, Clone, serde::Deserialize)]`, parseable via `serde_json::from_str::<Manifest>(json)`.

- [ ] **Step 1: Write the failing test**

```rust
// src/format/manifest.rs (bottom of file)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sample_manifest() {
        let json = std::fs::read_to_string("tests/fixtures/sample_manifest.json").unwrap();
        let manifest: Manifest = serde_json::from_str(&json).unwrap();

        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.name, "usagi");
        assert_eq!(manifest.name_slug, "usagi");
        assert_eq!(manifest.levels, 4);
        assert_eq!(manifest.animation_schema.schema_id, "legacy_default_v1");
        assert_eq!(manifest.sprites.sprite_count, 70);
        assert_eq!(manifest.sprites.size, [512, 512]);
        assert_eq!(manifest.sprites.base_path, "sprites/");
        assert_eq!(manifest.sprites.file_pattern, "%04d.webp");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test parses_sample_manifest`
Expected: FAIL to compile — `Manifest` is not defined yet.

- [ ] **Step 3: Write the implementation**

```rust
// src/format/manifest.rs (top of file, above the tests module)
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u32,
    pub name: String,
    pub name_slug: String,
    pub category: String,
    pub category_slug: String,
    #[serde(default)]
    pub description: String,
    pub bundle_version: u32,
    pub min_app_version: u32,
    pub levels: u8,
    pub origin: String,
    pub animation_schema: AnimationSchemaRef,
    pub sprites: SpriteSheetInfo,
    pub preview: PreviewInfo,
    pub author: AuthorInfo,
    pub license: LicenseInfo,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationSchemaRef {
    pub path: String,
    pub schema_id: String,
    pub version: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpriteSheetInfo {
    #[serde(rename = "type")]
    pub kind: String,
    pub base_path: String,
    pub file_pattern: String,
    pub sprite_count: u32,
    pub size: [u32; 2],
}

#[derive(Debug, Clone, Deserialize)]
pub struct PreviewInfo {
    pub thumbnail: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthorInfo {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseInfo {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
    pub attribution: String,
}
```

```rust
// src/format/mod.rs
pub mod manifest;
```

```rust
// src/main.rs — add at the top
mod format;

fn main() {
    println!("shimeji");
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test parses_sample_manifest`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/format/mod.rs src/format/manifest.rs src/main.rs
git commit -m "feat: parse manifest.json"
```

---

### Task 3: Parse `animation.json`

**Files:**
- Create: `src/format/animation.rs`
- Modify: `src/format/mod.rs` (add `pub mod animation;`)

**Interfaces:**
- Consumes: nothing (leaf module, independent of `manifest.rs`).
- Produces: `format::animation::{AnimationSchema, Animation, Frame, AutoBehavior, ChoiceItem, TimerRule, MaxDurationTicks, BorderTransition, EventRule, Edge, Direction, SurfaceType, LoopMode, EngineEventKind}`, all parseable via `serde_json::from_str::<AnimationSchema>(json)`. `AnimationSchema.animations: Vec<Animation>` and `.events: Vec<EventRule>` are consumed directly by Tasks 5, 9, and 10.

- [ ] **Step 1: Write the failing test**

```rust
// src/format/animation.rs (bottom of file)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sample_animation_schema() {
        let json = std::fs::read_to_string("tests/fixtures/sample_animation.json").unwrap();
        let schema: AnimationSchema = serde_json::from_str(&json).unwrap();

        assert_eq!(schema.schema_id, "legacy_default_v1");
        assert_eq!(schema.default_animation, "fall");
        assert_eq!(schema.initial_candidates, vec!["fall".to_string()]);
        assert_eq!(schema.animations.len(), 37);
        assert_eq!(schema.events.len(), 27);

        let walk_left = schema
            .animations
            .iter()
            .find(|a| a.key == "walk_left")
            .expect("walk_left animation must exist");
        assert_eq!(walk_left.kind, SurfaceType::Ground);
        assert_eq!(walk_left.loop_mode, LoopMode::Loop);
        assert_eq!(walk_left.direction, Direction::Left);
        assert_eq!(walk_left.frames.len(), 4);
        assert_eq!(walk_left.frames[0].sprite, 33);
        assert_eq!(walk_left.frames[0].dx, -2);
        assert_eq!(walk_left.frames[0].duration_ticks, 6);

        let auto = walk_left.auto.as_ref().expect("walk_left has auto behavior");
        assert_eq!(auto.on_timer.len(), 2);
        assert_eq!(auto.on_timer[0].chance, 0.6);
        assert_eq!(auto.on_timer[0].min_ticks, 96);
        assert_eq!(auto.on_timer[0].max_ticks, 96);
        assert!(matches!(auto.max_duration_ticks, Some(MaxDurationTicks::Fixed(480))));

        let climb_left = schema
            .animations
            .iter()
            .find(|a| a.key == "climb_left")
            .unwrap();
        let climb_auto = climb_left.auto.as_ref().unwrap();
        assert!(matches!(
            climb_auto.max_duration_ticks,
            Some(MaxDurationTicks::Range { min_ticks: 200, max_ticks: 699 })
        ));

        assert_eq!(walk_left.border_transitions.len(), 1);
        assert_eq!(walk_left.border_transitions[0].when, Edge::Left);
        assert_eq!(walk_left.border_transitions[0].choices.len(), 2);

        let drag_start = schema
            .events
            .iter()
            .find(|e| e.event == EngineEventKind::DragStart)
            .unwrap();
        assert_eq!(drag_start.from.as_deref(), Some("*"));
        assert_eq!(drag_start.to.as_deref(), Some("drag"));
        assert_eq!(drag_start.set_facing.as_deref(), Some("RANDOM"));

        let tap_level4_ground = schema
            .events
            .iter()
            .find(|e| {
                e.event == EngineEventKind::Tap
                    && e.min_level == Some(4)
                    && e.allowed_types.as_deref() == Some(&[SurfaceType::Ground])
            })
            .unwrap();
        assert_eq!(tap_level4_ground.choices.as_ref().unwrap().len(), 6);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test parses_sample_animation_schema`
Expected: FAIL to compile — none of these types exist yet.

- [ ] **Step 3: Write the implementation**

```rust
// src/format/animation.rs (top of file, above the tests module)
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AnimationSchema {
    pub schema_id: String,
    pub version: u32,
    pub default_animation: String,
    pub initial_candidates: Vec<String>,
    pub animations: Vec<Animation>,
    pub events: Vec<EventRule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Animation {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: SurfaceType,
    pub subtype: String,
    pub level: u8,
    #[serde(rename = "loop")]
    pub loop_mode: LoopMode,
    pub direction: Direction,
    pub frames: Vec<Frame>,
    #[serde(default)]
    pub auto: Option<AutoBehavior>,
    #[serde(default)]
    pub border_transitions: Vec<BorderTransition>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Frame {
    pub sprite: u32,
    pub dx: i32,
    pub dy: i32,
    #[serde(rename = "durationTicks")]
    pub duration_ticks: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoBehavior {
    #[serde(default)]
    pub on_finish: Vec<ChoiceItem>,
    #[serde(default)]
    pub on_timer: Vec<TimerRule>,
    #[serde(default)]
    pub max_duration_ticks: Option<MaxDurationTicks>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceItem {
    pub to: String,
    pub weight: f64,
    #[serde(default)]
    pub set_facing: Option<String>,
    #[serde(default)]
    pub min_level: Option<u8>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerRule {
    pub choices: Vec<ChoiceItem>,
    pub min_ticks: u32,
    pub max_ticks: u32,
    #[serde(default = "default_chance")]
    pub chance: f64,
}

fn default_chance() -> f64 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum MaxDurationTicks {
    Fixed(u32),
    Range {
        #[serde(rename = "minTicks")]
        min_ticks: u32,
        #[serde(rename = "maxTicks")]
        max_ticks: u32,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BorderTransition {
    pub when: Edge,
    #[serde(default)]
    pub facing: Option<Direction>,
    pub choices: Vec<ChoiceItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRule {
    pub event: EngineEventKind,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub when: Option<Edge>,
    #[serde(default)]
    pub facing: Option<Direction>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub set_facing: Option<String>,
    #[serde(default)]
    pub choices: Option<Vec<ChoiceItem>>,
    #[serde(default)]
    pub min_level: Option<u8>,
    #[serde(default)]
    pub max_level: Option<u8>,
    #[serde(default)]
    pub allowed_types: Option<Vec<SurfaceType>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Direction {
    Left,
    Right,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SurfaceType {
    Ground,
    Wall,
    Ceiling,
    Air,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LoopMode {
    Oneshot,
    Loop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EngineEventKind {
    DragStart,
    DragEnd,
    FlingStart,
    FlingEnd,
    Jump,
    Idle,
    Tap,
}
```

```rust
// src/format/mod.rs
pub mod animation;
pub mod manifest;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test parses_sample_animation_schema`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/format/animation.rs src/format/mod.rs
git commit -m "feat: parse animation.json state machine schema"
```

---

### Task 4: Decode WebP sprites

**Files:**
- Create: `src/format/sprites.rs`
- Modify: `src/format/mod.rs` (add `pub mod sprites;`)

**Interfaces:**
- Consumes: `manifest::SpriteSheetInfo` (`base_path`, `file_pattern`, `sprite_count`).
- Produces: `format::sprites::sprite_filename(pattern: &str, index: u32) -> String` and `format::sprites::decode_sprite(path: &std::path::Path) -> Result<image::RgbaImage, image::ImageError>`. Both consumed by Task 5 (existence checks use `sprite_filename`) and by Task 18 (actual pixel decode at spawn time).

- [ ] **Step 1: Write the failing test**

```rust
// src/format/sprites.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn formats_sprite_filenames() {
        assert_eq!(sprite_filename("%04d.webp", 0), "0000.webp");
        assert_eq!(sprite_filename("%04d.webp", 7), "0007.webp");
        assert_eq!(sprite_filename("%04d.webp", 69), "0069.webp");
    }

    #[test]
    fn decodes_a_sample_sprite() {
        let path = Path::new("tests/fixtures/sample_bundle/sprites/0000.webp");
        let img = decode_sprite(path).unwrap();
        assert_eq!(img.width(), 512);
        assert_eq!(img.height(), 512);
        assert_eq!(img.as_raw().len(), 512 * 512 * 4);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib format::sprites`
Expected: FAIL to compile — `sprite_filename`/`decode_sprite` undefined.

- [ ] **Step 3: Write the implementation**

```rust
// src/format/sprites.rs (above the tests module)
use std::path::Path;

pub fn sprite_filename(pattern: &str, index: u32) -> String {
    let pct = pattern.find('%').expect("sprite file pattern must contain '%'");
    let d = pattern[pct..]
        .find('d')
        .map(|i| pct + i)
        .expect("sprite file pattern must contain 'd' after '%'");
    let width: usize = pattern[pct + 1..d].parse().unwrap_or(0);
    format!("{}{:0width$}{}", &pattern[..pct], index, &pattern[d + 1..], width = width)
}

pub fn decode_sprite(path: &Path) -> Result<image::RgbaImage, image::ImageError> {
    Ok(image::open(path)?.into_rgba8())
}
```

```rust
// src/format/mod.rs
pub mod animation;
pub mod manifest;
pub mod sprites;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib format::sprites`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/format/sprites.rs src/format/mod.rs
git commit -m "feat: decode WebP sprite frames"
```

---

### Task 5: `MascotBundle::load` — cross-validation

**Files:**
- Create: `src/format/bundle.rs`
- Modify: `src/format/mod.rs` (add `pub mod bundle;`)
- Test: `tests/bundle_validation.rs`

**Interfaces:**
- Consumes: `manifest::Manifest`, `animation::AnimationSchema`, `sprites::sprite_filename`.
- Produces: `format::bundle::{MascotBundle, BundleError}`, `MascotBundle::load(dir: &Path) -> Result<MascotBundle, BundleError>`. `MascotBundle { manifest, animation, base_path }` is consumed by Task 7 (importer) and Task 18 (spawn).

`MascotBundle::load` validates, but deliberately does **not** decode sprite pixels (that's `sprites::decode_sprite`, called lazily later) — it only checks structure so import validation stays fast even for large sprite sheets.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/bundle_validation.rs
use shimeji::format::bundle::{BundleError, MascotBundle};
use std::fs;
use std::path::{Path, PathBuf};

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path);
        } else {
            fs::copy(entry.path(), &dst_path).unwrap();
        }
    }
}

fn sample_copy() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("bundle");
    copy_dir_recursive(Path::new("tests/fixtures/sample_bundle"), &dir);
    (tmp, dir)
}

#[test]
fn loads_the_valid_sample_bundle() {
    let (_tmp, dir) = sample_copy();
    let bundle = MascotBundle::load(&dir).unwrap();
    assert_eq!(bundle.manifest.name, "usagi");
    assert_eq!(bundle.animation.animations.len(), 37);
}

#[test]
fn rejects_unsupported_manifest_schema_version() {
    let (_tmp, dir) = sample_copy();
    let manifest_path = dir.join("manifest.json");
    let text = fs::read_to_string(&manifest_path).unwrap();
    let bumped = text.replacen("\"schemaVersion\": 1", "\"schemaVersion\": 2", 1);
    fs::write(&manifest_path, bumped).unwrap();

    let err = MascotBundle::load(&dir).unwrap_err();
    assert!(matches!(err, BundleError::UnsupportedSchemaVersion { found: 2 }));
}

#[test]
fn rejects_missing_sprite_file() {
    let (_tmp, dir) = sample_copy();
    fs::remove_file(dir.join("sprites/0000.webp")).unwrap();

    let err = MascotBundle::load(&dir).unwrap_err();
    assert!(matches!(err, BundleError::SpriteCountMismatch { declared: 70, found: 69, .. }));
}

#[test]
fn rejects_dangling_transition_target() {
    let (_tmp, dir) = sample_copy();
    let animation_path = dir.join("animation.json");
    let text = fs::read_to_string(&animation_path).unwrap();
    let broken = text.replacen("\"to\": \"walk_left\"", "\"to\": \"not_a_real_animation\"", 1);
    fs::write(&animation_path, broken).unwrap();

    let err = MascotBundle::load(&dir).unwrap_err();
    assert!(matches!(err, BundleError::UnknownAnimationKey { .. }));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test bundle_validation`
Expected: FAIL to compile — `MascotBundle`/`BundleError` undefined, and `shimeji::format` isn't public from a library target yet.

Note: this is the first test that needs the crate to expose a library target. Add to `Cargo.toml`:

```toml
[lib]
name = "shimeji"
path = "src/lib.rs"
```

And create `src/lib.rs`:

```rust
pub mod format;
```

And change `src/main.rs` to use the library instead of its own `mod format;`:

```rust
fn main() {
    println!("shimeji");
}
```

- [ ] **Step 3: Write the implementation**

```rust
// src/format/bundle.rs
use super::animation::AnimationSchema;
use super::manifest::Manifest;
use super::sprites::sprite_filename;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("could not read {path}: {source}")]
    Io { path: PathBuf, #[source] source: std::io::Error },
    #[error("invalid manifest.json: {0}")]
    ManifestParse(#[source] serde_json::Error),
    #[error("invalid animation.json: {0}")]
    AnimationParse(#[source] serde_json::Error),
    #[error("unsupported manifest schemaVersion {found} (expected 1)")]
    UnsupportedSchemaVersion { found: u32 },
    #[error("unsupported animation schemaId '{found}' (expected legacy_default_v1)")]
    UnsupportedAnimationSchema { found: String },
    #[error("manifest declares {declared} sprites but {found} were found under {base_path}")]
    SpriteCountMismatch { declared: u32, found: usize, base_path: PathBuf },
    #[error("animation '{animation}' frame references sprite index {index}, but only {count} sprites exist")]
    SpriteIndexOutOfRange { animation: String, index: u32, count: u32 },
    #[error("{context} references unknown animation key '{key}'")]
    UnknownAnimationKey { context: String, key: String },
}

pub struct MascotBundle {
    pub manifest: Manifest,
    pub animation: AnimationSchema,
    pub base_path: PathBuf,
}

impl MascotBundle {
    pub fn load(dir: &Path) -> Result<MascotBundle, BundleError> {
        let manifest_path = dir.join("manifest.json");
        let manifest_text = std::fs::read_to_string(&manifest_path)
            .map_err(|source| BundleError::Io { path: manifest_path.clone(), source })?;
        let manifest: Manifest =
            serde_json::from_str(&manifest_text).map_err(BundleError::ManifestParse)?;

        if manifest.schema_version != 1 {
            return Err(BundleError::UnsupportedSchemaVersion { found: manifest.schema_version });
        }

        let animation_path = dir.join(&manifest.animation_schema.path);
        let animation_text = std::fs::read_to_string(&animation_path)
            .map_err(|source| BundleError::Io { path: animation_path.clone(), source })?;
        let animation: AnimationSchema =
            serde_json::from_str(&animation_text).map_err(BundleError::AnimationParse)?;

        if animation.schema_id != "legacy_default_v1" {
            return Err(BundleError::UnsupportedAnimationSchema { found: animation.schema_id.clone() });
        }

        let sprites_dir = dir.join(&manifest.sprites.base_path);
        let found = (0..manifest.sprites.sprite_count)
            .filter(|i| sprites_dir.join(sprite_filename(&manifest.sprites.file_pattern, *i)).is_file())
            .count();
        if found != manifest.sprites.sprite_count as usize {
            return Err(BundleError::SpriteCountMismatch {
                declared: manifest.sprites.sprite_count,
                found,
                base_path: sprites_dir,
            });
        }

        for anim in &animation.animations {
            for frame in &anim.frames {
                if frame.sprite >= manifest.sprites.sprite_count {
                    return Err(BundleError::SpriteIndexOutOfRange {
                        animation: anim.key.clone(),
                        index: frame.sprite,
                        count: manifest.sprites.sprite_count,
                    });
                }
            }
        }

        let known_keys: HashSet<&str> = animation.animations.iter().map(|a| a.key.as_str()).collect();
        let check = |context: &str, key: &str| -> Result<(), BundleError> {
            if known_keys.contains(key) {
                Ok(())
            } else {
                Err(BundleError::UnknownAnimationKey { context: context.to_string(), key: key.to_string() })
            }
        };

        check("default_animation", &animation.default_animation)?;
        for key in &animation.initial_candidates {
            check("initial_candidates", key)?;
        }
        for anim in &animation.animations {
            if let Some(auto) = &anim.auto {
                for choice in &auto.on_finish {
                    check(&format!("{}.auto.onFinish", anim.key), &choice.to)?;
                }
                for rule in &auto.on_timer {
                    for choice in &rule.choices {
                        check(&format!("{}.auto.onTimer", anim.key), &choice.to)?;
                    }
                }
            }
            for bt in &anim.border_transitions {
                for choice in &bt.choices {
                    check(&format!("{}.borderTransitions", anim.key), &choice.to)?;
                }
            }
        }
        for event in &animation.events {
            if let Some(to) = &event.to {
                check(&format!("events[{:?}]", event.event), to)?;
            }
            if let Some(choices) = &event.choices {
                for choice in choices {
                    check(&format!("events[{:?}]", event.event), &choice.to)?;
                }
            }
        }

        Ok(MascotBundle { manifest, animation, base_path: dir.to_path_buf() })
    }
}
```

```rust
// src/format/mod.rs
pub mod animation;
pub mod bundle;
pub mod manifest;
pub mod sprites;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test bundle_validation`
Expected: PASS (all 4 tests)

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/lib.rs src/main.rs src/format/bundle.rs src/format/mod.rs tests/bundle_validation.rs
git commit -m "feat: validate mascot bundles end to end"
```

---

### Task 6: Catalog persistence (`catalog.json`)

**Files:**
- Create: `src/importer/mod.rs`
- Create: `src/importer/catalog.rs`
- Modify: `src/lib.rs` (add `pub mod importer;`)

**Interfaces:**
- Consumes: nothing beyond `std::path::Path`.
- Produces: `importer::catalog::{CatalogEntry, CatalogError, load_catalog, save_catalog, add_entry}`. `CatalogEntry { slug: String, name: String, dir: PathBuf }` is consumed by Task 7 (importer writes entries) and Task 17 (tray reads entries to build the Spawn menu).

- [ ] **Step 1: Write the failing test**

```rust
// src/importer/catalog.rs (bottom of file)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_catalog_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        assert!(load_catalog(root).unwrap().is_empty());

        add_entry(root, CatalogEntry {
            slug: "usagi".into(),
            name: "usagi".into(),
            dir: root.join("usagi"),
        }).unwrap();

        let entries = load_catalog(root).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].slug, "usagi");
        assert_eq!(entries[0].name, "usagi");
    }

    #[test]
    fn rejects_duplicate_slug() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        add_entry(root, CatalogEntry { slug: "usagi".into(), name: "usagi".into(), dir: root.join("usagi") }).unwrap();
        let err = add_entry(root, CatalogEntry { slug: "usagi".into(), name: "usagi again".into(), dir: root.join("usagi") })
            .unwrap_err();
        assert!(matches!(err, CatalogError::DuplicateSlug { .. }));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib importer::catalog`
Expected: FAIL to compile — nothing defined yet.

- [ ] **Step 3: Write the implementation**

```rust
// src/importer/catalog.rs (above the tests module)
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub slug: String,
    pub name: String,
    pub dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("could not read catalog.json: {0}")]
    Read(#[source] std::io::Error),
    #[error("could not write catalog.json: {0}")]
    Write(#[source] std::io::Error),
    #[error("catalog.json is corrupt: {0}")]
    Parse(#[source] serde_json::Error),
    #[error("a mascot with slug '{slug}' is already in the library")]
    DuplicateSlug { slug: String },
}

fn catalog_path(library_root: &Path) -> PathBuf {
    library_root.join("catalog.json")
}

pub fn load_catalog(library_root: &Path) -> Result<Vec<CatalogEntry>, CatalogError> {
    let path = catalog_path(library_root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).map_err(CatalogError::Read)?;
    serde_json::from_str(&text).map_err(CatalogError::Parse)
}

pub fn save_catalog(library_root: &Path, entries: &[CatalogEntry]) -> Result<(), CatalogError> {
    std::fs::create_dir_all(library_root).map_err(CatalogError::Write)?;
    let text = serde_json::to_string_pretty(entries).expect("CatalogEntry always serializes");
    std::fs::write(catalog_path(library_root), text).map_err(CatalogError::Write)
}

pub fn add_entry(library_root: &Path, entry: CatalogEntry) -> Result<(), CatalogError> {
    let mut entries = load_catalog(library_root)?;
    if entries.iter().any(|e| e.slug == entry.slug) {
        return Err(CatalogError::DuplicateSlug { slug: entry.slug });
    }
    entries.push(entry);
    save_catalog(library_root, &entries)
}
```

```rust
// src/importer/mod.rs
pub mod catalog;
```

```rust
// src/lib.rs
pub mod format;
pub mod importer;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib importer::catalog`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/importer src/lib.rs
git commit -m "feat: persist the imported-mascot catalog"
```

---

### Task 7: Importer — validate & unpack a `.zip`

**Files:**
- Modify: `src/importer/mod.rs`
- Test: `tests/importer.rs`

**Interfaces:**
- Consumes: `format::bundle::MascotBundle::load`, `importer::catalog::{CatalogEntry, add_entry}`.
- Produces: `importer::{import_zip, ImportError}`, `import_zip(zip_bytes: &[u8], library_root: &Path) -> Result<CatalogEntry, ImportError>`. Consumed by Task 17 (tray's "Import Mascot..." action and drag-drop handler).

- [ ] **Step 1: Write the failing tests**

```rust
// tests/importer.rs
use shimeji::importer::{import_zip, ImportError};
use shimeji::importer::catalog::load_catalog;
use std::fs;

#[test]
fn imports_the_sample_zip() {
    let tmp = tempfile::tempdir().unwrap();
    let library_root = tmp.path();
    let zip_bytes = fs::read("tests/fixtures/8ge8jqm7.zip").unwrap();

    let entry = import_zip(&zip_bytes, library_root).unwrap();
    assert_eq!(entry.slug, "usagi");
    assert_eq!(entry.name, "usagi");
    assert!(entry.dir.join("manifest.json").is_file());
    assert!(entry.dir.join("sprites/0000.webp").is_file());

    let catalog = load_catalog(library_root).unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].slug, "usagi");
}

#[test]
fn rejects_a_corrupt_zip_without_writing_anything() {
    let tmp = tempfile::tempdir().unwrap();
    let library_root = tmp.path();

    let err = import_zip(b"not a zip file", library_root).unwrap_err();
    assert!(matches!(err, ImportError::Zip(_)));
    assert!(load_catalog(library_root).unwrap().is_empty());
    assert!(!library_root.join("usagi").exists());
}

#[test]
fn rejects_importing_the_same_slug_twice() {
    let tmp = tempfile::tempdir().unwrap();
    let library_root = tmp.path();
    let zip_bytes = fs::read("tests/fixtures/8ge8jqm7.zip").unwrap();

    import_zip(&zip_bytes, library_root).unwrap();
    let err = import_zip(&zip_bytes, library_root).unwrap_err();
    assert!(matches!(err, ImportError::Catalog(_)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test importer`
Expected: FAIL to compile — `import_zip`/`ImportError` undefined.

- [ ] **Step 3: Write the implementation**

```rust
// src/importer/mod.rs
pub mod catalog;

use crate::format::bundle::{BundleError, MascotBundle};
use catalog::{add_entry, CatalogEntry, CatalogError};
use std::io::Cursor;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("not a valid zip file: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("could not extract the zip: {0}")]
    Io(#[from] std::io::Error),
    #[error("this doesn't look like a mascot bundle: {0}")]
    InvalidBundle(#[from] BundleError),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

pub fn import_zip(zip_bytes: &[u8], library_root: &Path) -> Result<CatalogEntry, ImportError> {
    let scratch = tempfile::tempdir()?;
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    archive.extract(scratch.path())?;

    let bundle = MascotBundle::load(scratch.path())?;

    let slug = bundle.manifest.name_slug.clone();
    let target_dir = library_root.join(&slug);
    if target_dir.exists() {
        return Err(ImportError::Catalog(CatalogError::DuplicateSlug { slug }));
    }

    std::fs::create_dir_all(library_root)?;
    copy_dir_recursive(scratch.path(), &target_dir)?;

    let entry = CatalogEntry { slug: slug.clone(), name: bundle.manifest.name.clone(), dir: target_dir };
    add_entry(library_root, entry.clone())?;
    Ok(entry)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}
```

Add `#[derive(Debug, Clone, ...)]` already present on `CatalogEntry` from Task 6 — confirm it derives `Clone` (it does, per Task 6's implementation).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test importer`
Expected: PASS (all 3 tests)

- [ ] **Step 5: Commit**

```bash
git add src/importer/mod.rs tests/importer.rs
git commit -m "feat: import and validate mascot zip bundles"
```

---

### Task 8: Weighted transition picker

**Files:**
- Create: `src/state_machine/mod.rs`
- Create: `src/state_machine/weighted.rs`
- Modify: `src/lib.rs` (add `pub mod state_machine;`)

**Interfaces:**
- Consumes: `format::animation::ChoiceItem`.
- Produces: `state_machine::weighted::pick_weighted<R: rand::RngExt>(choices: &[ChoiceItem], level: u8, rng: &mut R) -> Option<&ChoiceItem>`. Consumed by Tasks 9 and 10.

- [ ] **Step 1: Write the failing test**

```rust
// src/state_machine/weighted.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::animation::ChoiceItem;
    use rand::prelude::*;

    fn choice(to: &str, weight: f64, min_level: Option<u8>) -> ChoiceItem {
        ChoiceItem { to: to.to_string(), weight, set_facing: None, min_level }
    }

    #[test]
    fn filters_out_choices_below_the_current_level() {
        let choices = vec![choice("a", 1.0, Some(3)), choice("b", 1.0, None)];
        let mut rng = StdRng::seed_from_u64(1);
        for _ in 0..20 {
            let picked = pick_weighted(&choices, 1, &mut rng).unwrap();
            assert_eq!(picked.to, "b");
        }
    }

    #[test]
    fn returns_none_when_no_choice_is_eligible() {
        let choices = vec![choice("a", 1.0, Some(3))];
        let mut rng = StdRng::seed_from_u64(1);
        assert!(pick_weighted(&choices, 1, &mut rng).is_none());
    }

    #[test]
    fn respects_relative_weights_over_many_draws() {
        let choices = vec![choice("common", 9.0, None), choice("rare", 1.0, None)];
        let mut rng = StdRng::seed_from_u64(42);
        let mut common_count = 0;
        for _ in 0..1000 {
            if pick_weighted(&choices, 1, &mut rng).unwrap().to == "common" {
                common_count += 1;
            }
        }
        assert!(common_count > 800 && common_count < 980, "got {common_count}/1000");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib state_machine::weighted`
Expected: FAIL to compile — `pick_weighted` undefined.

- [ ] **Step 3: Write the implementation**

```rust
// src/state_machine/weighted.rs (above the tests module)
use crate::format::animation::ChoiceItem;
use rand::RngExt;

pub fn pick_weighted<'a, R: RngExt>(
    choices: &'a [ChoiceItem],
    level: u8,
    rng: &mut R,
) -> Option<&'a ChoiceItem> {
    let eligible: Vec<&ChoiceItem> = choices
        .iter()
        .filter(|c| c.min_level.map_or(true, |m| level >= m))
        .collect();
    if eligible.is_empty() {
        return None;
    }
    let total: f64 = eligible.iter().map(|c| c.weight).sum();
    let mut roll = rng.random_range(0.0..total);
    for c in &eligible {
        if roll < c.weight {
            return Some(c);
        }
        roll -= c.weight;
    }
    eligible.last().copied()
}
```

```rust
// src/state_machine/mod.rs
pub mod weighted;
```

```rust
// src/lib.rs
pub mod format;
pub mod importer;
pub mod state_machine;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib state_machine::weighted`
Expected: PASS (all 3 tests)

- [ ] **Step 5: Commit**

```bash
git add src/state_machine src/lib.rs
git commit -m "feat: add weighted transition picker"
```

---

### Task 9: State machine core stepping

**Files:**
- Modify: `src/state_machine/mod.rs`

**Interfaces:**
- Consumes: `format::animation::{AnimationSchema, Animation, Direction, Edge}`, `state_machine::weighted::pick_weighted`.
- Produces: `state_machine::{StateMachine, SurfaceKind, SurfaceContext, StepOutput}`. `StateMachine::new(schema: &AnimationSchema) -> StateMachine`, `StateMachine::step<R: rand::RngExt>(&mut self, surface: SurfaceContext, level: u8, rng: &mut R) -> StepOutput`, `StateMachine::current_key(&self) -> &str`, `StateMachine::facing(&self) -> Direction`. Consumed by Task 10 (adds `apply_event`) and Task 18 (drives the per-tick loop).

Design notes baked into this task:
- `onTimer` rules pick a random target tick **once**, when the rule's animation is entered (`rng.random_range(rule.min_ticks..=rule.max_ticks)`), then roll `chance` exactly once when `ticks_in_animation` reaches that target. This matches the fixture: a `{minTicks:0, maxTicks:720, chance:1}` rule does **not** fire on tick 0 — it fires once, at one randomly-chosen tick within the window.
- `maxDurationTicks` (resolved the same way if it's a `Range`) is a hard fallback: if reached with no `onTimer` rule having fired, the state machine force-transitions to `schema.default_animation`.

- [ ] **Step 1: Write the failing tests**

```rust
// src/state_machine/mod.rs (bottom of file, new tests module)
#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::animation::AnimationSchema;
    use rand::prelude::*;

    fn schema() -> AnimationSchema {
        let json = std::fs::read_to_string("tests/fixtures/sample_animation.json").unwrap();
        serde_json::from_str(&json).unwrap()
    }

    fn no_edge() -> SurfaceContext {
        SurfaceContext { kind: SurfaceKind::Ground, edge_hit: None }
    }

    #[test]
    fn starts_on_the_default_animation() {
        let schema = schema();
        let sm = StateMachine::new(&schema);
        assert_eq!(sm.current_key(), "fall");
    }

    #[test]
    fn oneshot_animation_advances_frames_by_duration_ticks() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        // Force onto stand_left (ONESHOT, single 90-tick frame) for this test via apply_event in Task 10;
        // for Task 9 we instead exercise walk_left directly since it's LOOP and reachable by stepping fall's border.
        // Use bounce_left instead: ONESHOT with two 8-tick frames, ends via onFinish.
        sm.force_animation("bounce_left");
        let mut rng = StdRng::seed_from_u64(7);

        let out1 = sm.step(no_edge(), 4, &mut rng);
        assert_eq!(out1.sprite_index, 17);
        for _ in 0..7 {
            sm.step(no_edge(), 4, &mut rng);
        }
        // after 8 ticks total, frame 2 (sprite 18) should be current
        let out_after_frame1 = sm.step(no_edge(), 4, &mut rng);
        assert_eq!(out_after_frame1.sprite_index, 18);
    }

    #[test]
    fn oneshot_animation_transitions_on_finish() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("bounce_left");
        let mut rng = StdRng::seed_from_u64(7);

        // bounce_left = 8 + 8 = 16 ticks total before onFinish fires.
        for _ in 0..16 {
            sm.step(no_edge(), 4, &mut rng);
        }
        assert!(sm.current_key() == "walk_left" || sm.current_key() == "walk_right");
    }

    #[test]
    fn border_transition_fires_immediately_on_edge_hit() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("walk_left");
        let mut rng = StdRng::seed_from_u64(3);

        let hit_left = SurfaceContext { kind: SurfaceKind::Ground, edge_hit: Some(crate::format::animation::Edge::Left) };
        sm.step(hit_left, 4, &mut rng);
        assert!(sm.current_key() == "climb_left" || sm.current_key() == "walk_right");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib state_machine::tests`
Expected: FAIL to compile — `StateMachine`, `SurfaceKind`, `SurfaceContext`, `force_animation` undefined.

- [ ] **Step 3: Write the implementation**

```rust
// src/state_machine/mod.rs (above the tests module)
pub mod weighted;

use crate::format::animation::{Animation, AnimationSchema, Direction, Edge};
use rand::RngExt;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    Ground,
    Wall,
    Ceiling,
    Air,
}

#[derive(Debug, Clone, Copy)]
pub struct SurfaceContext {
    pub kind: SurfaceKind,
    pub edge_hit: Option<Edge>,
}

#[derive(Debug, Clone, Copy)]
pub struct StepOutput {
    pub sprite_index: u32,
    pub dx: i32,
    pub dy: i32,
    pub changed_animation: bool,
}

struct TimerTarget {
    tick: u32,
    resolved: bool,
}

pub struct StateMachine<'a> {
    schema: &'a AnimationSchema,
    by_key: HashMap<&'a str, &'a Animation>,
    current: &'a Animation,
    frame_index: usize,
    frame_ticks_remaining: u32,
    ticks_in_animation: u32,
    facing: Direction,
    timer_targets: Vec<TimerTarget>,
    max_duration_target: Option<u32>,
}

/// `new()` and the internal `on_finish()` fallback below need a source of randomness but aren't
/// reachable from a test-controlled `rng` argument (the public `step`/`apply_event` API takes the
/// caller's `rng` explicitly for testability). They use this instead. This only affects unweighted
/// single-choice cases in the fixture's `onFinish` lists, so it doesn't affect the determinism of
/// any test above, which all supply their own seeded `rng` directly to `step`/`apply_event`.
fn fresh_rng() -> rand::rngs::StdRng {
    use rand::SeedableRng;
    rand::rngs::StdRng::seed_from_u64(rand::random())
}

impl<'a> StateMachine<'a> {
    pub fn new(schema: &'a AnimationSchema) -> Self {
        let by_key: HashMap<&str, &Animation> =
            schema.animations.iter().map(|a| (a.key.as_str(), a)).collect();
        let current = *by_key
            .get(schema.default_animation.as_str())
            .expect("bundle validation guarantees default_animation exists");
        let mut sm = StateMachine {
            schema,
            by_key,
            current,
            frame_index: 0,
            frame_ticks_remaining: current.frames[0].duration_ticks,
            ticks_in_animation: 0,
            facing: current.direction,
            timer_targets: Vec::new(),
            max_duration_target: None,
        };
        sm.resolve_timers_for_current(&mut fresh_rng());
        sm
    }

    pub fn force_animation(&mut self, key: &str) {
        self.enter_animation(key, self.facing, &mut fresh_rng());
    }

    pub fn current_key(&self) -> &str {
        &self.current.key
    }

    pub fn facing(&self) -> Direction {
        self.facing
    }

    fn enter_animation<R: RngExt>(&mut self, key: &str, facing: Direction, rng: &mut R) {
        self.current = self.by_key.get(key).expect("caller guarantees key exists");
        self.frame_index = 0;
        self.frame_ticks_remaining = self.current.frames[0].duration_ticks;
        self.ticks_in_animation = 0;
        self.facing = facing;
        self.resolve_timers_for_current(rng);
    }

    fn resolve_timers_for_current<R: RngExt>(&mut self, rng: &mut R) {
        self.timer_targets.clear();
        self.max_duration_target = None;
        let Some(auto) = &self.current.auto else { return };
        for rule in &auto.on_timer {
            let tick = if rule.min_ticks >= rule.max_ticks {
                rule.min_ticks
            } else {
                rng.random_range(rule.min_ticks..=rule.max_ticks)
            };
            self.timer_targets.push(TimerTarget { tick, resolved: false });
        }
        if let Some(max) = &auto.max_duration_ticks {
            self.max_duration_target = Some(match max {
                crate::format::animation::MaxDurationTicks::Fixed(t) => *t,
                crate::format::animation::MaxDurationTicks::Range { min_ticks, max_ticks } => {
                    if min_ticks >= max_ticks {
                        *min_ticks
                    } else {
                        rng.random_range(*min_ticks..=*max_ticks)
                    }
                }
            });
        }
    }

    pub fn step<R: RngExt>(&mut self, surface: SurfaceContext, level: u8, rng: &mut R) -> StepOutput {
        // Border transitions take priority: they fire immediately on edge contact.
        if let Some(edge) = surface.edge_hit {
            for bt in &self.current.border_transitions {
                if bt.when == edge && bt.facing.map_or(true, |f| f == self.facing) {
                    if let Some(choice) = weighted::pick_weighted(&bt.choices, level, rng) {
                        let facing = resolve_facing(choice.set_facing.as_deref(), self.facing, rng);
                        let to = choice.to.clone();
                        self.enter_animation(&to, facing, rng);
                        return self.frame_output(true);
                    }
                }
            }
        }

        let frame = &self.current.frames[self.frame_index];
        let dx = frame.dx;
        let dy = frame.dy;

        self.ticks_in_animation += 1;
        if self.frame_ticks_remaining > 1 {
            self.frame_ticks_remaining -= 1;
        } else {
            self.advance_frame();
        }

        // Cloning each due rule (rather than holding a borrow of `self.current.auto` across the
        // `self.enter_animation(...)` calls below, which also mutate `self`) keeps the borrow checker happy.
        if self.current.auto.is_some() {
            for i in 0..self.timer_targets.len() {
                if self.timer_targets[i].resolved || self.ticks_in_animation < self.timer_targets[i].tick {
                    continue;
                }
                self.timer_targets[i].resolved = true;
                let rule = self.current.auto.as_ref().unwrap().on_timer[i].clone();
                if rng.random_bool(rule.chance) {
                    if let Some(choice) = weighted::pick_weighted(&rule.choices, level, rng) {
                        let facing = resolve_facing(choice.set_facing.as_deref(), self.facing, rng);
                        let to = choice.to.clone();
                        self.enter_animation(&to, facing, rng);
                        return StepOutput { sprite_index: self.current.frames[0].sprite, dx, dy, changed_animation: true };
                    }
                }
            }
            if let Some(max) = self.max_duration_target {
                if self.ticks_in_animation >= max {
                    let default = self.schema.default_animation.clone();
                    self.enter_animation(&default, self.facing, rng);
                    return StepOutput { sprite_index: self.current.frames[0].sprite, dx, dy, changed_animation: true };
                }
            }
        }

        StepOutput { sprite_index: frame.sprite, dx, dy, changed_animation: false }
    }

    fn advance_frame(&mut self) {
        if self.frame_index + 1 < self.current.frames.len() {
            self.frame_index += 1;
            self.frame_ticks_remaining = self.current.frames[self.frame_index].duration_ticks;
        } else if matches!(self.current.loop_mode, crate::format::animation::LoopMode::Loop) {
            self.frame_index = 0;
            self.frame_ticks_remaining = self.current.frames[0].duration_ticks;
        } else {
            self.on_finish();
        }
    }

    fn on_finish(&mut self) {
        let Some(auto) = &self.current.auto else { return };
        if auto.on_finish.is_empty() {
            return;
        }
        let mut rng = fresh_rng();
        if let Some(choice) = weighted::pick_weighted(&auto.on_finish, u8::MAX, &mut rng) {
            let facing = resolve_facing(choice.set_facing.as_deref(), self.facing, &mut rng);
            let to = choice.to.clone();
            self.enter_animation(&to, facing, &mut rng);
        }
    }

    fn frame_output(&self, changed_animation: bool) -> StepOutput {
        let frame = &self.current.frames[self.frame_index];
        StepOutput { sprite_index: frame.sprite, dx: frame.dx, dy: frame.dy, changed_animation }
    }
}

fn resolve_facing<R: RngExt>(set_facing: Option<&str>, current: Direction, rng: &mut R) -> Direction {
    match set_facing {
        Some("LEFT") => Direction::Left,
        Some("RIGHT") => Direction::Right,
        Some("RANDOM") => if rng.random_bool(0.5) { Direction::Left } else { Direction::Right },
        _ => current,
    }
}
```

`TimerRule` needs `Clone` for the `.clone()` call above — add `Clone` to its `#[derive(...)]` list in `src/format/animation.rs` from Task 3 (it currently derives `Debug, Deserialize`; change to `Debug, Clone, Deserialize`) as part of this task.

`resolve_facing` takes `rng` because `"RANDOM"` (used by the `DRAG_START` event in Task 10) needs to flip a coin — every call site above already threads its local `rng` through.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib state_machine`
Expected: PASS (all tests in this module)

- [ ] **Step 5: Commit**

```bash
git add src/state_machine/mod.rs src/format/animation.rs
git commit -m "feat: interpret animation.json frame/timer/border transitions"
```

---

### Task 10: State machine event handling

**Files:**
- Modify: `src/state_machine/mod.rs`

**Interfaces:**
- Consumes: `format::animation::EngineEventKind`, `state_machine::{StateMachine, SurfaceContext, weighted::pick_weighted}`.
- Produces: `StateMachine::apply_event<R: rand::RngExt>(&mut self, event: EngineEventKind, surface: SurfaceContext, level: u8, rng: &mut R) -> bool` (returns whether a transition occurred). Consumed by Task 16 (mouse input → `DRAG_START`/`DRAG_END`/`FLING_START`/`FLING_END`/`TAP`/`JUMP`) and Task 18 (idle timeout → `IDLE`).

- [ ] **Step 1: Write the failing tests**

```rust
// src/state_machine/mod.rs — extend the existing #[cfg(test)] mod tests block
    use crate::format::animation::{EngineEventKind, SurfaceType};

    #[test]
    fn drag_start_switches_to_drag_from_any_animation() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("walk_left");
        let mut rng = StdRng::seed_from_u64(1);

        let changed = sm.apply_event(EngineEventKind::DragStart, no_edge(), 4, &mut rng);
        assert!(changed);
        assert_eq!(sm.current_key(), "drag");
    }

    #[test]
    fn fling_end_on_left_edge_climbs() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("fling");
        let mut rng = StdRng::seed_from_u64(1);

        let left_edge = SurfaceContext { kind: SurfaceKind::Wall, edge_hit: Some(crate::format::animation::Edge::Left) };
        let changed = sm.apply_event(EngineEventKind::FlingEnd, left_edge, 4, &mut rng);
        assert!(changed);
        assert_eq!(sm.current_key(), "climb_left");
    }

    #[test]
    fn tap_at_level_one_has_no_matching_rule() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("walk_left");
        let mut rng = StdRng::seed_from_u64(1);

        let changed = sm.apply_event(EngineEventKind::Tap, no_edge(), 1, &mut rng);
        assert!(!changed);
        assert_eq!(sm.current_key(), "walk_left");
    }

    #[test]
    fn tap_at_level_four_on_ground_picks_a_response() {
        let schema = schema();
        let mut sm = StateMachine::new(&schema);
        sm.force_animation("walk_left");
        assert_eq!(sm.current.kind, SurfaceType::Ground);
        let mut rng = StdRng::seed_from_u64(1);

        let changed = sm.apply_event(EngineEventKind::Tap, no_edge(), 4, &mut rng);
        assert!(changed);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib state_machine`
Expected: the four new tests FAIL to compile — `apply_event` undefined, `sm.current` is private (make the test's direct field access via a helper instead — see Step 3).

- [ ] **Step 3: Write the implementation**

Add a `pub fn current_kind(&self) -> SurfaceType` helper (used by the level-4-tap test above in place of touching the private `current` field directly) and `apply_event`:

```rust
// src/state_machine/mod.rs — add to the `impl<'a> StateMachine<'a>` block
    pub fn current_kind(&self) -> crate::format::animation::SurfaceType {
        self.current.kind
    }

    pub fn apply_event<R: RngExt>(
        &mut self,
        event: crate::format::animation::EngineEventKind,
        surface: SurfaceContext,
        level: u8,
        rng: &mut R,
    ) -> bool {
        for rule in &self.schema.events {
            if rule.event != event {
                continue;
            }
            let from_matches = match &rule.from {
                Some(f) if f == "*" => true,
                Some(f) => f == self.current.key.as_str(),
                None => {
                    let level_ok = rule.min_level.map_or(true, |m| level >= m)
                        && rule.max_level.map_or(true, |m| level <= m);
                    let type_ok = rule
                        .allowed_types
                        .as_ref()
                        .map_or(true, |types| types.contains(&self.current.kind));
                    level_ok && type_ok
                }
            };
            if !from_matches {
                continue;
            }
            if let Some(when) = rule.when {
                if surface.edge_hit != Some(when) {
                    continue;
                }
            }
            if let Some(facing) = rule.facing {
                if facing != self.facing {
                    continue;
                }
            }

            if let Some(choices) = &rule.choices {
                if let Some(choice) = weighted::pick_weighted(choices, level, rng) {
                    let facing = resolve_facing(choice.set_facing.as_deref(), self.facing, rng);
                    let to = choice.to.clone();
                    self.enter_animation(&to, facing, rng);
                    return true;
                }
            } else if let Some(to) = &rule.to {
                let facing = resolve_facing(rule.set_facing.as_deref(), self.facing, rng);
                let to = to.clone();
                self.enter_animation(&to, facing, rng);
                return true;
            }
        }
        false
    }
```

(`resolve_facing` already takes `rng` and handles `"RANDOM"` — used by `DRAG_START` — from Task 9.)

Fix the test's field-access line (`sm.current.kind`) to use the new accessor: change `assert_eq!(sm.current.kind, SurfaceType::Ground);` to `assert_eq!(sm.current_kind(), SurfaceType::Ground);` in the test written in Step 1.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib state_machine`
Expected: PASS (every test in the module, old and new)

- [ ] **Step 5: Commit**

```bash
git add src/state_machine/mod.rs
git commit -m "feat: handle drag/fling/jump/idle/tap engine events"
```

---

### Task 11: Environment tracking abstraction

**Files:**
- Create: `src/environment/mod.rs`
- Create: `src/environment/tracker.rs`
- Modify: `src/lib.rs` (add `pub mod environment;`)

**Interfaces:**
- Consumes: nothing beyond `std::time::Duration`.
- Produces: `environment::{Rect, MonitorSource, WindowSource, EnvironmentTracker}`. `Rect { left: i32, top: i32, right: i32, bottom: i32 }`; `MonitorSource`/`WindowSource` are traits (`fn monitors(&self) -> Vec<Rect>`, `fn windows(&self) -> Vec<Rect>`) so real Win32 sources (Task 13) and fake sources (this task's tests) are interchangeable. `EnvironmentTracker::new(monitor_source, window_source, refresh_interval: Duration) -> Self`, `EnvironmentTracker::poll(&mut self, now: Instant) -> (&Rect, &[Rect])` returning the current virtual-screen rect and window rects, only re-querying the sources when `refresh_interval` has elapsed since the last query. Consumed by Task 12 (surface queries read the tracker's cached rects) and Task 18 (drives the tracker once per tick).

- [ ] **Step 1: Write the failing tests**

```rust
// src/environment/tracker.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::time::{Duration, Instant};

    struct CountingSource {
        rects: Vec<Rect>,
        calls: Cell<u32>,
    }
    impl MonitorSource for CountingSource {
        fn monitors(&self) -> Vec<Rect> {
            self.calls.set(self.calls.get() + 1);
            self.rects.clone()
        }
    }
    impl WindowSource for CountingSource {
        fn windows(&self) -> Vec<Rect> {
            self.calls.set(self.calls.get() + 1);
            self.rects.clone()
        }
    }

    #[test]
    fn combines_monitor_rects_into_a_virtual_screen_rect() {
        let monitors = CountingSource {
            rects: vec![
                Rect { left: 0, top: 0, right: 1920, bottom: 1080 },
                Rect { left: 1920, top: 0, right: 3840, bottom: 1080 },
            ],
            calls: Cell::new(0),
        };
        let windows = CountingSource { rects: vec![], calls: Cell::new(0) };
        let mut tracker = EnvironmentTracker::new(monitors, windows, Duration::from_millis(150));

        let (screen, _) = tracker.poll(Instant::now());
        assert_eq!(*screen, Rect { left: 0, top: 0, right: 3840, bottom: 1080 });
    }

    #[test]
    fn does_not_requery_before_the_refresh_interval_elapses() {
        let monitors = CountingSource { rects: vec![Rect { left: 0, top: 0, right: 100, bottom: 100 }], calls: Cell::new(0) };
        let windows = CountingSource { rects: vec![], calls: Cell::new(0) };
        let mut tracker = EnvironmentTracker::new(monitors, windows, Duration::from_millis(150));

        let t0 = Instant::now();
        tracker.poll(t0);
        tracker.poll(t0 + Duration::from_millis(50));
        assert_eq!(tracker.monitor_source.calls.get(), 1);

        tracker.poll(t0 + Duration::from_millis(200));
        assert_eq!(tracker.monitor_source.calls.get(), 2);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib environment::tracker`
Expected: FAIL to compile — nothing defined yet.

- [ ] **Step 3: Write the implementation**

```rust
// src/environment/tracker.rs (above the tests module)
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

pub trait MonitorSource {
    fn monitors(&self) -> Vec<Rect>;
}

pub trait WindowSource {
    fn windows(&self) -> Vec<Rect>;
}

pub struct EnvironmentTracker<M: MonitorSource, W: WindowSource> {
    pub monitor_source: M,
    pub window_source: W,
    refresh_interval: Duration,
    last_refresh: Option<Instant>,
    screen_rect: Rect,
    window_rects: Vec<Rect>,
}

impl<M: MonitorSource, W: WindowSource> EnvironmentTracker<M, W> {
    pub fn new(monitor_source: M, window_source: W, refresh_interval: Duration) -> Self {
        EnvironmentTracker {
            monitor_source,
            window_source,
            refresh_interval,
            last_refresh: None,
            screen_rect: Rect { left: 0, top: 0, right: 0, bottom: 0 },
            window_rects: Vec::new(),
        }
    }

    pub fn poll(&mut self, now: Instant) -> (&Rect, &[Rect]) {
        let due = match self.last_refresh {
            None => true,
            Some(last) => now.duration_since(last) >= self.refresh_interval,
        };
        if due {
            self.screen_rect = combine_rects(&self.monitor_source.monitors());
            self.window_rects = self.window_source.windows();
            self.last_refresh = Some(now);
        }
        (&self.screen_rect, &self.window_rects)
    }
}

fn combine_rects(rects: &[Rect]) -> Rect {
    rects.iter().fold(
        Rect { left: i32::MAX, top: i32::MAX, right: i32::MIN, bottom: i32::MIN },
        |acc, r| Rect {
            left: acc.left.min(r.left),
            top: acc.top.min(r.top),
            right: acc.right.max(r.right),
            bottom: acc.bottom.max(r.bottom),
        },
    )
}
```

```rust
// src/environment/mod.rs
pub mod tracker;
pub use tracker::{EnvironmentTracker, MonitorSource, Rect, WindowSource};
```

```rust
// src/lib.rs
pub mod environment;
pub mod format;
pub mod importer;
pub mod state_machine;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib environment::tracker`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/environment src/lib.rs
git commit -m "feat: add throttled environment (monitor/window) tracker"
```

---

### Task 12: Environment surface-query geometry

**Files:**
- Create: `src/environment/surface.rs`
- Modify: `src/environment/mod.rs` (add `pub mod surface;`)

**Interfaces:**
- Consumes: `environment::Rect`, `state_machine::{SurfaceContext, SurfaceKind}`, `format::animation::Edge`.
- Produces: `environment::surface::query_surface(mascot_x: i32, mascot_y: i32, mascot_width: i32, mascot_height: i32, dx: i32, dy: i32, screen: &Rect, windows: &[Rect]) -> SurfaceContext`. Consumed by Task 18 (called once per mascot per tick, feeding `StateMachine::step`).

Geometry model: the mascot's bounding box is `(mascot_x, mascot_y)` to `(mascot_x + mascot_width, mascot_y + mascot_height)`. "Standing" means the box's bottom edge is resting on some rect's top edge (screen or a window) directly beneath it. `dx`/`dy` (this tick's proposed movement, from the state machine's current frame) is used to detect an edge about to be crossed: if moving left would put the box's left edge past the nearest surface's left bound (or the screen's left bound when not on any window), that's `Edge::Left`, etc. Prefer the **narrowest applicable surface**: if the mascot's feet are over another window's top edge, that window is the floor, not the screen.

- [ ] **Step 1: Write the failing tests**

```rust
// src/environment/surface.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::animation::Edge;
    use crate::state_machine::SurfaceKind;

    const SCREEN: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };

    #[test]
    fn ground_with_no_windows_is_the_screen_floor() {
        let ctx = query_surface(500, 1080 - 64, 64, 64, 0, 0, &SCREEN, &[]);
        assert_eq!(ctx.kind, SurfaceKind::Ground);
        assert_eq!(ctx.edge_hit, None);
    }

    #[test]
    fn walking_left_off_the_screen_hits_left_edge() {
        let ctx = query_surface(0, 1080 - 64, 64, 64, -2, 0, &SCREEN, &[]);
        assert_eq!(ctx.edge_hit, Some(Edge::Left));
    }

    #[test]
    fn walking_right_off_the_screen_hits_right_edge() {
        let ctx = query_surface(1920 - 64, 1080 - 64, 64, 64, 2, 0, &SCREEN, &[]);
        assert_eq!(ctx.edge_hit, Some(Edge::Right));
    }

    #[test]
    fn standing_on_top_of_another_window_uses_that_window_as_ground() {
        let browser = Rect { left: 200, top: 400, right: 900, bottom: 900 };
        let ctx = query_surface(500, 400 - 64, 64, 64, 0, 0, &SCREEN, &[browser]);
        assert_eq!(ctx.kind, SurfaceKind::Ground);
        assert_eq!(ctx.edge_hit, None);
    }

    #[test]
    fn walking_off_the_left_edge_of_a_hosting_window() {
        let browser = Rect { left: 200, top: 400, right: 900, bottom: 900 };
        let ctx = query_surface(200, 400 - 64, 64, 64, -2, 0, &SCREEN, &[browser]);
        assert_eq!(ctx.edge_hit, Some(Edge::Left));
    }

    #[test]
    fn falling_in_open_air_is_air_with_no_edge() {
        let ctx = query_surface(500, 300, 64, 64, 0, 15, &SCREEN, &[]);
        assert_eq!(ctx.kind, SurfaceKind::Air);
        assert_eq!(ctx.edge_hit, None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib environment::surface`
Expected: FAIL to compile — `query_surface` undefined.

- [ ] **Step 3: Write the implementation**

```rust
// src/environment/surface.rs (above the tests module)
use super::Rect;
use crate::format::animation::Edge;
use crate::state_machine::{SurfaceContext, SurfaceKind};

const GROUND_TOLERANCE: i32 = 2;

pub fn query_surface(
    mascot_x: i32,
    mascot_y: i32,
    mascot_width: i32,
    mascot_height: i32,
    dx: i32,
    dy: i32,
    screen: &Rect,
    windows: &[Rect],
) -> SurfaceContext {
    let feet_y = mascot_y + mascot_height;
    let center_x = mascot_x + mascot_width / 2;

    let hosting = windows
        .iter()
        .filter(|w| w.left <= center_x && center_x < w.right && (w.top - feet_y).abs() <= GROUND_TOLERANCE)
        .min_by_key(|w| w.right - w.left)
        .copied();

    let floor = hosting.unwrap_or(*screen);
    let on_ground = (floor.top - feet_y).abs() <= GROUND_TOLERANCE;

    if !on_ground {
        return SurfaceContext { kind: SurfaceKind::Air, edge_hit: None };
    }

    let mut edge_hit = None;
    if dx < 0 && mascot_x + dx <= floor.left {
        edge_hit = Some(Edge::Left);
    } else if dx > 0 && mascot_x + mascot_width + dx >= floor.right {
        edge_hit = Some(Edge::Right);
    }

    SurfaceContext { kind: SurfaceKind::Ground, edge_hit }
}
```

```rust
// src/environment/mod.rs
pub mod surface;
pub mod tracker;
pub use tracker::{EnvironmentTracker, MonitorSource, Rect, WindowSource};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib environment::surface`
Expected: PASS (all 6 tests)

- [ ] **Step 5: Commit**

```bash
git add src/environment/surface.rs src/environment/mod.rs
git commit -m "feat: resolve ground/wall/edge surface context from screen and window rects"
```

---

### Task 13: Win32 monitor & window enumeration

**Files:**
- Create: `src/environment/win32.rs`
- Modify: `src/environment/mod.rs` (add `pub mod win32;`)
- Modify: `Cargo.toml`
- Create: `examples/dump_environment.rs`

**Interfaces:**
- Consumes: `environment::{MonitorSource, WindowSource, Rect}`.
- Produces: `environment::win32::{Win32MonitorSource, Win32WindowSource}`, both implementing the Task 11 traits. Consumed by Task 18's real (non-test) wiring.

No automated test — this is a thin, OS-dependent wrapper. Verified manually via the `dump_environment` example.

- [ ] **Step 1: Add the `windows` crate with the features this task needs**

```bash
cargo add windows --features "Win32_Foundation,Win32_UI_WindowsAndMessaging,Win32_Graphics_Gdi,Win32_Graphics_Dwm,Win32_System_LibraryLoader"
```

(If a later task's `cargo build` reports a missing item from another `windows::Win32::...` module, add that module's feature the same way — the crate's own compiler errors name the exact feature string required.)

- [ ] **Step 2: Write the implementation**

```rust
// src/environment/win32.rs
use super::{MonitorSource, Rect, WindowSource};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowTextLengthW, IsWindowVisible, IsIconic,
};

pub struct Win32MonitorSource;
pub struct Win32WindowSource {
    pub exclude: Vec<HWND>,
}

impl MonitorSource for Win32MonitorSource {
    fn monitors(&self) -> Vec<Rect> {
        let mut rects: Vec<Rect> = Vec::new();
        unsafe {
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(monitor_enum_proc),
                LPARAM(&mut rects as *mut Vec<Rect> as isize),
            );
        }
        rects
    }
}

unsafe extern "system" fn monitor_enum_proc(
    _hmonitor: HMONITOR,
    _hdc: HDC,
    rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let rects = &mut *(lparam.0 as *mut Vec<Rect>);
    let r = *rect;
    rects.push(Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom });
    BOOL(1)
}

impl WindowSource for Win32WindowSource {
    fn windows(&self) -> Vec<Rect> {
        let mut collected: Vec<(HWND, Rect)> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(window_enum_proc), LPARAM(&mut collected as *mut Vec<(HWND, Rect)> as isize));
        }
        collected
            .into_iter()
            .filter(|(hwnd, _)| !self.exclude.contains(hwnd))
            .map(|(_, rect)| rect)
            .collect()
    }
}

unsafe extern "system" fn window_enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let collected = &mut *(lparam.0 as *mut Vec<(HWND, Rect)>);

    if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
        return BOOL(1);
    }
    if GetWindowTextLengthW(hwnd) == 0 {
        return BOOL(1); // skip windows with no title (tool/helper windows)
    }

    let mut cloaked: u32 = 0;
    let _ = DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        &mut cloaked as *mut u32 as *mut _,
        std::mem::size_of::<u32>() as u32,
    );
    if cloaked != 0 {
        return BOOL(1); // skip cloaked windows (e.g. UWP apps on another virtual desktop)
    }

    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_ok() {
        collected.push((hwnd, Rect { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom }));
    }
    BOOL(1)
}
```

```rust
// src/environment/mod.rs
pub mod surface;
pub mod tracker;
pub mod win32;
pub use tracker::{EnvironmentTracker, MonitorSource, Rect, WindowSource};
```

- [ ] **Step 3: Write a manual-verification example**

```rust
// examples/dump_environment.rs
use shimeji::environment::win32::{Win32MonitorSource, Win32WindowSource};
use shimeji::environment::{MonitorSource, WindowSource};

fn main() {
    let monitors = Win32MonitorSource.monitors();
    println!("Monitors ({}):", monitors.len());
    for r in &monitors {
        println!("  {:?}", r);
    }

    let windows = Win32WindowSource { exclude: vec![] }.windows();
    println!("Visible top-level windows ({}):", windows.len());
    for r in &windows {
        println!("  {:?}", r);
    }
}
```

- [ ] **Step 4: Build and manually verify**

Run: `cargo run --example dump_environment`

Manually confirm: the monitor list matches your actual display layout (check against Windows Settings > Display), and the window list includes visibly open windows (e.g. this terminal, a browser if open) with plausible on-screen coordinates, and does **not** include minimized windows.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/environment/win32.rs src/environment/mod.rs examples/dump_environment.rs
git commit -m "feat: enumerate monitors and top-level windows via Win32"
```

---

### Task 14: Alpha premultiplication + layered window creation/painting

**Files:**
- Create: `src/window/mod.rs`
- Create: `src/window/alpha.rs`
- Create: `src/window/mascot_window.rs`
- Modify: `src/lib.rs` (add `pub mod window;`)
- Modify: `Cargo.toml`
- Create: `examples/show_static_sprite.rs`

**Interfaces:**
- Consumes: `image::RgbaImage` (Task 4).
- Produces: `window::alpha::premultiply(rgba: &image::RgbaImage) -> Vec<u8>` (BGRA, premultiplied, the exact byte layout `UpdateLayeredWindow` expects), and `window::mascot_window::MascotWindow` with `MascotWindow::create(initial_frame: &image::RgbaImage, x: i32, y: i32) -> windows::core::Result<MascotWindow>` and `MascotWindow::update_frame(&self, frame: &image::RgbaImage)` and `MascotWindow::move_to(&self, x: i32, y: i32)`. Consumed by Task 18.

Only `premultiply` is unit-tested — window creation/painting is OS-integration, verified manually via the example.

- [ ] **Step 1: Write the failing test for premultiplication**

```rust
// src/window/alpha.rs
#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn fully_opaque_pixel_becomes_bgra_unchanged() {
        let mut img = RgbaImage::new(1, 1);
        img.put_pixel(0, 0, Rgba([10, 20, 30, 255]));
        let out = premultiply(&img);
        assert_eq!(out, vec![30, 20, 10, 255]); // B, G, R, A
    }

    #[test]
    fn half_alpha_pixel_scales_color_channels() {
        let mut img = RgbaImage::new(1, 1);
        img.put_pixel(0, 0, Rgba([200, 100, 40, 128]));
        let out = premultiply(&img);
        // 200*128/255 ≈ 100, 100*128/255 ≈ 50, 40*128/255 ≈ 20
        assert_eq!(out, vec![20, 50, 100, 128]);
    }

    #[test]
    fn fully_transparent_pixel_becomes_all_zero() {
        let mut img = RgbaImage::new(1, 1);
        img.put_pixel(0, 0, Rgba([255, 255, 255, 0]));
        let out = premultiply(&img);
        assert_eq!(out, vec![0, 0, 0, 0]);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib window::alpha`
Expected: FAIL to compile — `premultiply` undefined.

- [ ] **Step 3: Write the implementation**

```rust
// src/window/alpha.rs (above the tests module)
use image::RgbaImage;

/// Converts RGBA to premultiplied BGRA, the byte layout `UpdateLayeredWindow`
/// requires for a 32bpp top-down DIB with per-pixel alpha.
pub fn premultiply(img: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::with_capacity(img.as_raw().len());
    for px in img.pixels() {
        let [r, g, b, a] = px.0;
        let scale = a as u32;
        out.push(((b as u32 * scale) / 255) as u8);
        out.push(((g as u32 * scale) / 255) as u8);
        out.push(((r as u32 * scale) / 255) as u8);
        out.push(a);
    }
    out
}
```

```rust
// src/window/mod.rs
pub mod alpha;
pub mod mascot_window;
```

```rust
// src/lib.rs
pub mod environment;
pub mod format;
pub mod importer;
pub mod state_machine;
pub mod window;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib window::alpha`
Expected: PASS

- [ ] **Step 5: Add the `windows` crate features this task needs and write the layered window wrapper**

```bash
cargo add windows --features "Win32_UI_WindowsAndMessaging,Win32_Graphics_Gdi,Win32_System_LibraryLoader,Win32_Foundation"
```

```rust
// src/window/mascot_window.rs
use super::alpha::premultiply;
use image::RgbaImage;
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW, SetWindowPos, UpdateLayeredWindow,
    CW_USEDEFAULT, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, ULW_ALPHA,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    BLENDFUNCTION, AC_SRC_ALPHA, AC_SRC_OVER,
};

pub struct MascotWindow {
    pub hwnd: HWND,
}

impl MascotWindow {
    pub fn create(initial_frame: &RgbaImage, x: i32, y: i32) -> Result<MascotWindow> {
        unsafe {
            let hinstance = GetModuleHandleW(None)?;
            let class_name = w!("ShimejiMascotWindow");

            let wc = WNDCLASSW {
                lpfnWndProc: Some(DefWindowProcW),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                ..Default::default()
            };
            // RegisterClassW fails harmlessly if already registered by an earlier instance; ignore that case.
            let _ = RegisterClassW(&wc);

            let width = initial_frame.width() as i32;
            let height = initial_frame.height() as i32;

            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                class_name,
                w!("Shimeji"),
                WS_POPUP,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(hinstance.into()),
                None,
            )?;

            let window = MascotWindow { hwnd };
            window.update_frame(initial_frame);
            Ok(window)
        }
    }

    pub fn update_frame(&self, frame: &RgbaImage) {
        unsafe {
            let width = frame.width() as i32;
            let height = frame.height() as i32;
            let screen_dc = windows::Win32::Graphics::Gdi::GetDC(None);
            let mem_dc = CreateCompatibleDC(Some(screen_dc));

            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // negative = top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };

            let mut bits_ptr: *mut core::ffi::c_void = std::ptr::null_mut();
            let dib = CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0)
                .expect("CreateDIBSection failed");
            let old_bitmap = SelectObject(mem_dc, dib.into());

            let bgra = premultiply(frame);
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits_ptr as *mut u8, bgra.len());

            let size = SIZE { cx: width, cy: height };
            let src_pos = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };

            let _ = UpdateLayeredWindow(
                self.hwnd,
                Some(screen_dc),
                None,
                Some(&size),
                Some(mem_dc),
                Some(&src_pos),
                windows::Win32::Foundation::COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );

            SelectObject(mem_dc, old_bitmap);
            let _ = DeleteObject(dib.into());
            let _ = DeleteDC(mem_dc);
            windows::Win32::Graphics::Gdi::ReleaseDC(None, screen_dc);
        }
    }

    pub fn move_to(&self, x: i32, y: i32) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
}
```

- [ ] **Step 6: Write a manual-verification example**

```rust
// examples/show_static_sprite.rs
use shimeji::format::sprites::decode_sprite;
use shimeji::window::mascot_window::MascotWindow;
use std::path::Path;
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, TranslateMessage, MSG};

fn main() {
    let frame = decode_sprite(Path::new("tests/fixtures/sample_bundle/sprites/0000.webp")).unwrap();
    let _window = MascotWindow::create(&frame, 400, 300).unwrap();

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
```

- [ ] **Step 7: Build and manually verify**

Run: `cargo run --example show_static_sprite`

Manually confirm: a small borderless window appears at (400, 300) showing sprite `0000.webp` with correct transparency (no black/white box around the character), stays on top of other windows, and does not appear in the taskbar. Close it via Task Manager or Ctrl+C in the terminal (it has no close affordance yet — that's fine, this task only proves painting works).

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock src/window src/lib.rs examples/show_static_sprite.rs
git commit -m "feat: create and paint a per-pixel-alpha layered mascot window"
```

---

### Task 15: Per-pixel hit testing

**Files:**
- Create: `src/window/hit_test.rs`
- Modify: `src/window/mod.rs` (add `pub mod hit_test;`)
- Modify: `src/window/mascot_window.rs` (wire `WM_NCHITTEST`)

**Interfaces:**
- Consumes: `image::RgbaImage`.
- Produces: `window::hit_test::is_opaque_at(frame: &image::RgbaImage, local_x: i32, local_y: i32) -> bool`. Consumed by `MascotWindow`'s window procedure (this task) and, conceptually, by Task 16's click classification (a click that lands on a transparent pixel should pass through rather than start a drag).

- [ ] **Step 1: Write the failing test**

```rust
// src/window/hit_test.rs
#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn frame_with_one_opaque_pixel() -> RgbaImage {
        let mut img = RgbaImage::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
        img.put_pixel(2, 2, Rgba([255, 0, 0, 255]));
        img
    }

    #[test]
    fn opaque_pixel_hits() {
        let img = frame_with_one_opaque_pixel();
        assert!(is_opaque_at(&img, 2, 2));
    }

    #[test]
    fn transparent_pixel_passes_through() {
        let img = frame_with_one_opaque_pixel();
        assert!(!is_opaque_at(&img, 0, 0));
    }

    #[test]
    fn out_of_bounds_passes_through() {
        let img = frame_with_one_opaque_pixel();
        assert!(!is_opaque_at(&img, -1, 0));
        assert!(!is_opaque_at(&img, 100, 0));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib window::hit_test`
Expected: FAIL to compile — `is_opaque_at` undefined.

- [ ] **Step 3: Write the implementation**

```rust
// src/window/hit_test.rs (above the tests module)
use image::RgbaImage;

const OPAQUE_THRESHOLD: u8 = 8;

pub fn is_opaque_at(frame: &RgbaImage, local_x: i32, local_y: i32) -> bool {
    if local_x < 0 || local_y < 0 || local_x as u32 >= frame.width() || local_y as u32 >= frame.height() {
        return false;
    }
    frame.get_pixel(local_x as u32, local_y as u32).0[3] > OPAQUE_THRESHOLD
}
```

```rust
// src/window/mod.rs
pub mod alpha;
pub mod hit_test;
pub mod mascot_window;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib window::hit_test`
Expected: PASS

- [ ] **Step 5: No automated test for the `WM_NCHITTEST` wiring itself — defer it to Task 16**, since hit-testing is only observable once mouse input is wired up. Commit this task's pure logic now:

```bash
git add src/window/hit_test.rs src/window/mod.rs
git commit -m "feat: add per-pixel opacity hit testing"
```

---

### Task 16: Mouse drag/fling/tap classification + window input wiring

**Files:**
- Create: `src/window/input.rs`
- Modify: `src/window/mascot_window.rs` (custom `WndProc`, `WM_NCHITTEST`, mouse messages, right-click menu)
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: `state_machine::StateMachine` (via `apply_event`), `window::hit_test::is_opaque_at`, `format::animation::EngineEventKind`.
- Produces: `window::input::{PointerSample, classify_release}`, a pure function `classify_release(down: PointerSample, up: PointerSample) -> ReleaseKind` (`ReleaseKind::Tap` or `ReleaseKind::Fling { vx: f64, vy: f64 }`), used by the (manually-verified) `WndProc` this task adds. Also produces a per-mascot right-click context menu offering "Jump" (fires `EngineEventKind::Jump`) and "Close" (destroys that one `MascotWindow`), which is how the `JUMP` event — otherwise unreachable from drag/tap/fling — gets triggered.

`classify_release` is unit tested. The `WndProc` wiring is manually verified (mouse/window integration, out of scope for automated testing per the Global Constraints).

- [ ] **Step 1: Write the failing test**

```rust
// src/window/input.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn sample_at(t: Instant, x: i32, y: i32) -> PointerSample {
        PointerSample { time: t, x, y }
    }

    #[test]
    fn a_quick_small_movement_is_a_tap() {
        let t0 = Instant::now();
        let down = sample_at(t0, 100, 100);
        let up = sample_at(t0 + Duration::from_millis(120), 103, 98);
        assert!(matches!(classify_release(down, up), ReleaseKind::Tap));
    }

    #[test]
    fn a_fast_large_movement_is_a_fling_with_velocity() {
        let t0 = Instant::now();
        let down = sample_at(t0, 100, 100);
        let up = sample_at(t0 + Duration::from_millis(100), 300, 50);
        match classify_release(down, up) {
            ReleaseKind::Fling { vx, vy } => {
                assert!(vx > 0.0, "expected rightward velocity, got {vx}");
                assert!(vy < 0.0, "expected upward velocity, got {vy}");
            }
            ReleaseKind::Tap => panic!("expected a fling"),
        }
    }

    #[test]
    fn a_slow_large_movement_is_still_a_drag_release_not_a_tap() {
        let t0 = Instant::now();
        let down = sample_at(t0, 100, 100);
        let up = sample_at(t0 + Duration::from_secs(2), 400, 100);
        assert!(matches!(classify_release(down, up), ReleaseKind::Fling { .. }));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib window::input`
Expected: FAIL to compile — `PointerSample`, `ReleaseKind`, `classify_release` undefined.

- [ ] **Step 3: Write the implementation**

```rust
// src/window/input.rs (above the tests module)
use std::time::Instant;

const TAP_MAX_DISTANCE_PX: f64 = 6.0;
const TAP_MAX_DURATION_SECS: f64 = 0.25;

#[derive(Debug, Clone, Copy)]
pub struct PointerSample {
    pub time: Instant,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy)]
pub enum ReleaseKind {
    Tap,
    Fling { vx: f64, vy: f64 },
}

pub fn classify_release(down: PointerSample, up: PointerSample) -> ReleaseKind {
    let dx = (up.x - down.x) as f64;
    let dy = (up.y - down.y) as f64;
    let distance = (dx * dx + dy * dy).sqrt();
    let duration = up.time.duration_since(down.time).as_secs_f64().max(1.0 / 1000.0);

    if distance <= TAP_MAX_DISTANCE_PX && duration <= TAP_MAX_DURATION_SECS {
        ReleaseKind::Tap
    } else {
        ReleaseKind::Fling { vx: dx / duration, vy: dy / duration }
    }
}
```

```rust
// src/window/mod.rs
pub mod alpha;
pub mod hit_test;
pub mod input;
pub mod mascot_window;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib window::input`
Expected: PASS (all 3 tests)

- [ ] **Step 5: Wire mouse/menu handling into `MascotWindow` (manual verification, no automated test)**

Add the required feature:

```bash
cargo add windows --features "Win32_UI_Input_KeyboardAndMouse"
```

This step changes `MascotWindow::create`'s signature from Task 14 (`create(initial_frame, x, y)`) to `create(initial_frame, x, y, owner: HWND, instance_id: u32) -> Result<MascotWindow>` — Task 18 is the only other caller and is written against this new signature.

Every mascot posts its classified input back to a single **owner window** (created once in Task 18's `main.rs`) rather than handling `StateMachine`/`Environment` itself — that keeps exactly one `App` owning all mascot state. `WM_APP + 10..13` are defined here since this is where they're first produced; Task 18 is where they're consumed.

```rust
// src/window/mascot_window.rs — add above `impl MascotWindow`, replacing the Task 14 import list's
// `DefWindowProcW`-only usage with these additions:
use crate::window::input::{classify_release, PointerSample, ReleaseKind};
use std::time::Instant;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, GetCursorPos, GetParent, GetWindowLongPtrW, PostMessageW,
    ScreenToClient, SetCapture, SetWindowLongPtrW, TrackPopupMenu, GWLP_USERDATA, HTCLIENT,
    HTTRANSPARENT, MF_STRING, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_DESTROY, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCHITTEST, WM_RBUTTONUP,
};
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;

pub const WM_MASCOT_TAP: u32 = WM_APP + 10;
pub const WM_MASCOT_FLING: u32 = WM_APP + 11;
pub const WM_MASCOT_JUMP: u32 = WM_APP + 12;
pub const WM_MASCOT_CLOSE: u32 = WM_APP + 13;

struct WindowState {
    frame: RgbaImage,
    mouse_down: Option<PointerSample>,
    instance_id: u32,
    owner: HWND,
}

unsafe extern "system" fn mascot_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let state = &mut *ptr;

    match msg {
        WM_NCHITTEST => {
            let mut pt = POINT { x: (lparam.0 & 0xFFFF) as i16 as i32, y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 };
            let _ = ScreenToClient(hwnd, &mut pt);
            if crate::window::hit_test::is_opaque_at(&state.frame, pt.x, pt.y) {
                LRESULT(HTCLIENT as isize)
            } else {
                LRESULT(HTTRANSPARENT as isize)
            }
        }
        WM_LBUTTONDOWN => {
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            state.mouse_down = Some(PointerSample { time: Instant::now(), x: cursor.x, y: cursor.y });
            SetCapture(hwnd);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if state.mouse_down.is_some() {
                let mut cursor = POINT::default();
                let _ = GetCursorPos(&mut cursor);
                let half = (state.frame.width() / 2) as i32;
                let _ = SetWindowPos(hwnd, None, cursor.x - half, cursor.y - half, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = ReleaseCapture();
            if let Some(down) = state.mouse_down.take() {
                let mut cursor = POINT::default();
                let _ = GetCursorPos(&mut cursor);
                let up = PointerSample { time: Instant::now(), x: cursor.x, y: cursor.y };
                match classify_release(down, up) {
                    ReleaseKind::Tap => {
                        let _ = PostMessageW(Some(state.owner), WM_MASCOT_TAP, WPARAM(state.instance_id as usize), LPARAM(0));
                    }
                    ReleaseKind::Fling { vx, vy } => {
                        let packed = ((vy.clamp(-32768.0, 32767.0) as i16 as u32) << 16)
                            | (vx.clamp(-32768.0, 32767.0) as i16 as u16 as u32);
                        let _ = PostMessageW(Some(state.owner), WM_MASCOT_FLING, WPARAM(state.instance_id as usize), LPARAM(packed as isize));
                    }
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let menu = CreatePopupMenu().unwrap();
            let _ = AppendMenuW(menu, MF_STRING, 1, w!("Jump"));
            let _ = AppendMenuW(menu, MF_STRING, 2, w!("Close"));
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let choice = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, cursor.x, cursor.y, Some(0), hwnd, None);
            if choice.0 == 1 {
                let _ = PostMessageW(Some(state.owner), WM_MASCOT_JUMP, WPARAM(state.instance_id as usize), LPARAM(0));
            } else if choice.0 == 2 {
                let _ = PostMessageW(Some(state.owner), WM_MASCOT_CLOSE, WPARAM(state.instance_id as usize), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            drop(Box::from_raw(ptr));
            let _ = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
```

Update `MascotWindow::create` (Task 14) to register `mascot_wnd_proc` instead of `DefWindowProcW`, accept `owner: HWND` and `instance_id: u32`, pass `Some(owner)` as `CreateWindowExW`'s `hwndParent` (this makes `owner` the window's *owner*, not a true parent — the standard Win32 technique for a borderless popup that still targets messages at another window), and box a `WindowState` into `GWLP_USERDATA` right after creation:

```rust
// src/window/mascot_window.rs — inside `MascotWindow::create`, after the existing `CreateWindowExW(...)` call succeeds:
            let state = Box::new(WindowState {
                frame: initial_frame.clone(),
                mouse_down: None,
                instance_id,
                owner,
            });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
```

and change the earlier `CreateWindowExW` call's `hwndParent` argument from `None` to `Some(owner)`, and its `lpfnWndProc` field (on the `WNDCLASSW` used to register the class) from `Some(DefWindowProcW)` to `Some(mascot_wnd_proc)`.

Finally, keep `state.frame` in sync so hit-testing uses the currently-displayed sprite: at the end of `update_frame` (Task 14), after painting, also refresh the stored state:

```rust
// src/window/mascot_window.rs — appended to the end of `update_frame`, before the closing brace
            let ptr = GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                (&mut *ptr).frame = frame.clone();
            }
```

- [ ] **Step 6: Build and manually verify**

Run: `cargo run --example show_static_sprite` (after adding a temporary `println!` in the input handlers, or a debugger breakpoint, since Task 18 hasn't wired the state machine yet)

Manually confirm: clicking on the opaque part of the sprite starts a drag that follows the mouse; clicking on a fully transparent corner of the window passes the click through to the window/desktop behind it; a quick click without movement is distinguishable from a drag in your temporary logging; right-clicking shows a "Jump"/"Close" menu and "Close" closes the window.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/window/input.rs src/window/mod.rs src/window/mascot_window.rs
git commit -m "feat: classify mouse drag/tap/fling and wire mascot window input"
```

---

### Task 17: Tray icon, spawn/import menu, drag-drop

**Files:**
- Create: `src/tray/mod.rs`
- Create: `src/tray/menu.rs`
- Modify: `src/lib.rs` (add `pub mod tray;`)
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: `importer::catalog::CatalogEntry`, `importer::import_zip`.
- Produces: `tray::menu::{filter_zip_paths, build_spawn_items}` (pure, tested) and `tray::TrayIcon` (Win32 `Shell_NotifyIconW` wrapper — manually verified). `filter_zip_paths(paths: &[PathBuf]) -> Vec<PathBuf>` keeps only `.zip` paths (case-insensitive) from a `WM_DROPFILES` payload. `build_spawn_items(catalog: &[CatalogEntry]) -> Vec<(u16, String)>` maps catalog entries to `(menu command id, label)` pairs, ids starting at a fixed base so they don't collide with the static "Import Mascot...", "Close All", "Exit" items. Consumed by Task 18.

- [ ] **Step 1: Write the failing tests**

```rust
// src/tray/menu.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::importer::catalog::CatalogEntry;
    use std::path::PathBuf;

    #[test]
    fn keeps_only_zip_paths_case_insensitively() {
        let paths = vec![
            PathBuf::from("C:/drop/usagi.zip"),
            PathBuf::from("C:/drop/readme.txt"),
            PathBuf::from("C:/drop/OTHER.ZIP"),
        ];
        let kept = filter_zip_paths(&paths);
        assert_eq!(kept, vec![PathBuf::from("C:/drop/usagi.zip"), PathBuf::from("C:/drop/OTHER.ZIP")]);
    }

    #[test]
    fn builds_stable_menu_ids_starting_at_the_spawn_base() {
        let catalog = vec![
            CatalogEntry { slug: "usagi".into(), name: "usagi".into(), dir: PathBuf::from("usagi") },
            CatalogEntry { slug: "neko".into(), name: "neko".into(), dir: PathBuf::from("neko") },
        ];
        let items = build_spawn_items(&catalog);
        assert_eq!(items, vec![(SPAWN_MENU_ID_BASE, "usagi".to_string()), (SPAWN_MENU_ID_BASE + 1, "neko".to_string())]);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tray::menu`
Expected: FAIL to compile — nothing defined yet.

- [ ] **Step 3: Write the implementation**

```rust
// src/tray/menu.rs (above the tests module)
use crate::importer::catalog::CatalogEntry;
use std::path::PathBuf;

pub const SPAWN_MENU_ID_BASE: u16 = 1000;
pub const IMPORT_MENU_ID: u16 = 1;
pub const CLOSE_ALL_MENU_ID: u16 = 2;
pub const EXIT_MENU_ID: u16 = 3;

pub fn filter_zip_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|p| p.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("zip")))
        .cloned()
        .collect()
}

pub fn build_spawn_items(catalog: &[CatalogEntry]) -> Vec<(u16, String)> {
    catalog
        .iter()
        .enumerate()
        .map(|(i, entry)| (SPAWN_MENU_ID_BASE + i as u16, entry.name.clone()))
        .collect()
}
```

```rust
// src/tray/mod.rs
pub mod menu;
```

```rust
// src/lib.rs
pub mod environment;
pub mod format;
pub mod importer;
pub mod state_machine;
pub mod tray;
pub mod window;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib tray::menu`
Expected: PASS

- [ ] **Step 5: Add the `windows` crate features this task needs and write the tray icon wrapper (manual verification, no automated test)**

```bash
cargo add windows --features "Win32_UI_Shell"
```

```rust
// src/tray/mod.rs — append below `pub mod menu;`
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
};
use windows::Win32::UI::WindowsAndMessaging::{LoadIconW, IDI_APPLICATION, WM_APP};

pub const WM_TRAY_CALLBACK: u32 = WM_APP + 1;

pub struct TrayIcon {
    data: NOTIFYICONDATAW,
}

impl TrayIcon {
    pub fn create(owner: HWND) -> windows::core::Result<TrayIcon> {
        let mut data = NOTIFYICONDATAW::default();
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = owner;
        data.uID = 1;
        data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        data.uCallbackMessage = WM_TRAY_CALLBACK;
        data.hIcon = unsafe { LoadIconW(None, IDI_APPLICATION)? };
        let tip = "Shimeji\0".encode_utf16().collect::<Vec<u16>>();
        data.szTip[..tip.len()].copy_from_slice(&tip);

        unsafe {
            Shell_NotifyIconW(NIM_ADD, &data).ok()?;
        }
        Ok(TrayIcon { data })
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.data);
        }
    }
}

impl TrayIcon {
    /// Shows a Windows balloon notification from the tray icon — used for both import failures
    /// ("this doesn't look like a mascot bundle: ...") and successes, per the spec's requirement
    /// that a rejected import explains why.
    pub fn notify(&self, title: &str, message: &str) {
        use windows::Win32::UI::Shell::NIF_INFO;
        let mut data = self.data;
        data.uFlags |= NIF_INFO;
        let title_u16 = title.encode_utf16().collect::<Vec<u16>>();
        let len = title_u16.len().min(data.szInfoTitle.len() - 1);
        data.szInfoTitle[..len].copy_from_slice(&title_u16[..len]);
        let msg_u16 = message.encode_utf16().collect::<Vec<u16>>();
        let len = msg_u16.len().min(data.szInfo.len() - 1);
        data.szInfo[..len].copy_from_slice(&msg_u16[..len]);
        unsafe {
            let _ = Shell_NotifyIconW(windows::Win32::UI::Shell::NIM_MODIFY, &data);
        }
    }
}
```

Drag-and-drop onto the tray icon's owner window (and any mascot window) is enabled the same way in both places: call `DragAcceptFiles(hwnd, true)` on window creation, handle `WM_DROPFILES` by calling `DragQueryFileW` for each dropped path, then `DragFinish`, then pass the resulting paths through `filter_zip_paths` and each surviving path's bytes to `importer::import_zip`. Task 18 owns exactly where this handler lives (the app's hidden owner window), so it's specified precisely there rather than duplicated here.

- [ ] **Step 6: Build and manually verify**

This task alone has no runnable entry point (it needs an owner `HWND`, provided by Task 18) — defer manual verification of the tray icon and drag-drop to Task 18's checklist.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/tray src/lib.rs
git commit -m "feat: add system tray icon and spawn/import menu building blocks"
```

---

### Task 18: App wiring — main loop, tick, spawn/close mascots

**Files:**
- Create: `src/app.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: everything from Tasks 1-17: `format::bundle::MascotBundle`, `format::sprites::decode_sprite`, `importer::{import_zip, ImportError, catalog::{load_catalog, CatalogEntry}}`, `state_machine::{StateMachine, SurfaceContext}`, `environment::{EnvironmentTracker, surface::query_surface, win32::{Win32MonitorSource, Win32WindowSource}}`, `window::mascot_window::{MascotWindow, WM_MASCOT_TAP, WM_MASCOT_FLING, WM_MASCOT_JUMP, WM_MASCOT_CLOSE}`, `tray::{TrayIcon, WM_TRAY_CALLBACK, menu::*}`.
- Produces: the finished `shimeji.exe`. No further tasks consume this one — it's the integration root.

Fling physics this task adds (the `fling` animation's own frame has `dx: 0, dy: 0` in the fixture — the thrown velocity is engine state layered on top, not part of the schema): on `FLING_START`, the mascot's release velocity (converted from px/sec to px/tick at 60 ticks/sec) is stored on the `MascotInstance` and added to its position every tick, with a small constant downward acceleration (gravity) applied to the vertical component, until `query_surface` reports an `edge_hit`, which fires `FLING_END` and clears the stored velocity.

- [ ] **Step 1: Write `App` state, spawn/import/event handling, and the tick function**

```rust
// src/app.rs
use crate::environment::surface::query_surface;
use crate::environment::win32::{Win32MonitorSource, Win32WindowSource};
use crate::environment::{EnvironmentTracker, Rect};
use crate::format::animation::{Direction, EngineEventKind};
use crate::format::bundle::MascotBundle;
use crate::format::sprites::{decode_sprite, sprite_filename};
use crate::importer::catalog::CatalogEntry;
use crate::state_machine::StateMachine;
use crate::tray::TrayIcon;
use crate::window::mascot_window::MascotWindow;
use rand::rngs::StdRng;
use rand::SeedableRng;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;
use std::time::{Duration, Instant};

const IDLE_THRESHOLD_TICKS: u32 = 3600;
const SPRITE_SIZE: i32 = 512;
const TICKS_PER_SECOND: f64 = 60.0;
const FLING_GRAVITY_PER_TICK: f64 = 0.6;

pub struct MascotInstance {
    pub id: u32,
    pub bundle: MascotBundle,
    pub current_key: String,
    pub facing: Direction,
    pub window: MascotWindow,
    pub x: i32,
    pub y: i32,
    pub level: u8,
    pub ticks_since_interaction: u32,
    pub fling_velocity: Option<(f64, f64)>,
    pub rng: StdRng,
}

pub struct App {
    pub library_root: std::path::PathBuf,
    pub catalog: Vec<CatalogEntry>,
    pub mascots: Vec<MascotInstance>,
    pub environment: EnvironmentTracker<Win32MonitorSource, Win32WindowSource>,
    pub tray: TrayIcon,
    pub owner: HWND,
    next_instance_id: u32,
}

impl App {
    pub fn new(library_root: std::path::PathBuf, owner: HWND, tray: TrayIcon) -> Self {
        let catalog = crate::importer::catalog::load_catalog(&library_root).unwrap_or_default();
        let environment = EnvironmentTracker::new(
            Win32MonitorSource,
            Win32WindowSource { exclude: Vec::new() },
            Duration::from_millis(150),
        );
        App { library_root, catalog, mascots: Vec::new(), environment, tray, owner, next_instance_id: 1 }
    }

    pub fn import_from_bytes(&mut self, zip_bytes: &[u8]) {
        match crate::importer::import_zip(zip_bytes, &self.library_root) {
            Ok(entry) => {
                self.tray.notify("Mascot imported", &format!("\"{}\" is ready to spawn.", entry.name));
                self.catalog = crate::importer::catalog::load_catalog(&self.library_root).unwrap_or_default();
            }
            Err(err) => {
                self.tray.notify("Import failed", &err.to_string());
            }
        }
    }

    pub fn spawn(&mut self, slug: &str) -> windows::core::Result<()> {
        let Some(entry) = self.catalog.iter().find(|e| e.slug == slug) else { return Ok(()) };
        let bundle = match MascotBundle::load(&entry.dir) {
            Ok(b) => b,
            Err(err) => {
                self.tray.notify("Could not spawn mascot", &err.to_string());
                return Ok(());
            }
        };

        let default_key = bundle.animation.default_animation.clone();
        let default_anim = bundle.animation.animations.iter().find(|a| a.key == default_key).unwrap();
        let sprite_path = bundle
            .base_path
            .join(&bundle.manifest.sprites.base_path)
            .join(sprite_filename(&bundle.manifest.sprites.file_pattern, default_anim.frames[0].sprite));
        let frame = decode_sprite(&sprite_path).map_err(|_| windows::core::Error::from_win32())?;

        let id = self.next_instance_id;
        self.next_instance_id += 1;
        let level = bundle.manifest.levels;
        let facing = default_anim.direction;
        let window = MascotWindow::create(&frame, 100, 100, self.owner, id)?;

        self.mascots.push(MascotInstance {
            id,
            bundle,
            current_key: default_key,
            facing,
            window,
            x: 100,
            y: 100,
            level,
            ticks_since_interaction: 0,
            fling_velocity: None,
            rng: StdRng::seed_from_u64(rand::random()),
        });
        Ok(())
    }

    pub fn close(&mut self, instance_id: u32) {
        if let Some(pos) = self.mascots.iter().position(|m| m.id == instance_id) {
            let mascot = self.mascots.remove(pos);
            unsafe {
                let _ = DestroyWindow(mascot.window.hwnd);
            }
        }
    }

    pub fn close_all(&mut self) {
        for mascot in self.mascots.drain(..) {
            unsafe {
                let _ = DestroyWindow(mascot.window.hwnd);
            }
        }
    }

    pub fn handle_engine_event(&mut self, instance_id: u32, event: EngineEventKind, fling_velocity: Option<(f64, f64)>) {
        let (screen, windows) = self.environment.poll(Instant::now());
        let screen = *screen;
        let windows = windows.to_vec();
        let Some(mascot) = self.mascots.iter_mut().find(|m| m.id == instance_id) else { return };

        let mut sm = StateMachine::new(&mascot.bundle.animation);
        sm.force_animation(&mascot.current_key);
        let surface = query_surface(mascot.x, mascot.y, SPRITE_SIZE, SPRITE_SIZE, 0, 0, &screen, &windows);
        if sm.apply_event(event, surface, mascot.level, &mut mascot.rng) {
            mascot.current_key = sm.current_key().to_string();
            mascot.facing = sm.facing();
            mascot.ticks_since_interaction = 0;
            if event == EngineEventKind::FlingStart {
                if let Some((vx, vy)) = fling_velocity {
                    mascot.fling_velocity = Some((vx / TICKS_PER_SECOND, vy / TICKS_PER_SECOND));
                }
            }
        }
    }

    pub fn tick(&mut self, now: Instant) {
        let (screen, windows) = self.environment.poll(now);
        let screen = *screen;
        let windows = windows.to_vec();

        for mascot in &mut self.mascots {
            step_one_mascot(mascot, &screen, &windows);
        }
    }
}

fn step_one_mascot(mascot: &mut MascotInstance, screen: &Rect, windows: &[Rect]) {
    let mut sm = StateMachine::new(&mascot.bundle.animation);
    sm.force_animation(&mascot.current_key);

    let surface = query_surface(mascot.x, mascot.y, SPRITE_SIZE, SPRITE_SIZE, 0, 0, screen, windows);
    let out = sm.step(surface, mascot.level, &mut mascot.rng);

    let mut dx = out.dx;
    let mut dy = out.dy;
    if let Some((vx, vy)) = mascot.fling_velocity {
        dx += vx.round() as i32;
        dy += vy.round() as i32;
        mascot.fling_velocity = Some((vx, vy + FLING_GRAVITY_PER_TICK));
    }

    mascot.x += dx;
    mascot.y += dy;
    mascot.current_key = sm.current_key().to_string();
    mascot.facing = sm.facing();
    mascot.window.move_to(mascot.x, mascot.y);

    if mascot.current_key == "fling" && surface.edge_hit.is_some() {
        sm.apply_event(EngineEventKind::FlingEnd, surface, mascot.level, &mut mascot.rng);
        mascot.current_key = sm.current_key().to_string();
        mascot.fling_velocity = None;
    }

    let sprite_path = mascot
        .bundle
        .base_path
        .join(&mascot.bundle.manifest.sprites.base_path)
        .join(sprite_filename(&mascot.bundle.manifest.sprites.file_pattern, out.sprite_index));
    if let Ok(frame) = decode_sprite(&sprite_path) {
        mascot.window.update_frame(&frame);
    }

    mascot.ticks_since_interaction += 1;
    if mascot.ticks_since_interaction == IDLE_THRESHOLD_TICKS {
        sm.apply_event(EngineEventKind::Idle, surface, mascot.level, &mut mascot.rng);
        mascot.current_key = sm.current_key().to_string();
    }
}
```

`EngineEventKind` needs `PartialEq` for the `event == EngineEventKind::FlingStart` comparison above — it already derives `PartialEq, Eq` in Task 3, so no change needed there.

- [ ] **Step 2: Wire the Win32 message loop, hidden owner window, tray, and drag-drop**

```rust
// src/main.rs
mod app;

use app::App;
use shimeji::importer::catalog::CatalogEntry;
use shimeji::tray::menu::{build_spawn_items, filter_zip_paths, CLOSE_ALL_MENU_ID, EXIT_MENU_ID, IMPORT_MENU_ID};
use shimeji::tray::{TrayIcon, WM_TRAY_CALLBACK};
use shimeji::window::mascot_window::{WM_MASCOT_CLOSE, WM_MASCOT_FLING, WM_MASCOT_JUMP, WM_MASCOT_TAP};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, DragQueryPoint, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetCursorPos,
    GetMessageW, PeekMessageW, PostQuitMessage, RegisterClassW, SetForegroundWindow, SetTimer,
    TrackPopupMenu, TranslateMessage, MF_STRING, MSG, PM_REMOVE, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    WM_DESTROY, WM_DROPFILES, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::core::w;

const TICK_TIMER_ID: usize = 1;
const TICK_INTERVAL_MS: u32 = 1000 / 60;

fn main() -> windows::core::Result<()> {
    let library_root = dirs_next::data_dir()
        .expect("APPDATA must be resolvable on Windows")
        .join("ShimejiRust")
        .join("mascots");

    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let class_name = w!("ShimejiOwnerWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
        let owner = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_name,
            w!("Shimeji"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance.into()),
            None,
        )?;
        DragAcceptFiles(owner, true);

        let tray = TrayIcon::create(owner)?;
        let mut app = App::new(library_root, owner, tray);
        SetTimer(Some(owner), TICK_TIMER_ID, TICK_INTERVAL_MS, None);

        let mut msg = MSG::default();
        let mut last_tick = Instant::now();
        loop {
            if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                match msg.message {
                    WM_DESTROY => {
                        PostQuitMessage(0);
                        break;
                    }
                    WM_TIMER => app.tick(Instant::now()),
                    WM_MASCOT_TAP => app.handle_engine_event(msg.wParam.0 as u32, shimeji::format::animation::EngineEventKind::DragEnd, None),
                    WM_MASCOT_FLING => {
                        let packed = msg.lParam.0 as u32;
                        let vx = (packed & 0xFFFF) as i16 as f64;
                        let vy = ((packed >> 16) & 0xFFFF) as i16 as f64;
                        app.handle_engine_event(msg.wParam.0 as u32, shimeji::format::animation::EngineEventKind::FlingStart, Some((vx, vy)));
                    }
                    WM_MASCOT_JUMP => app.handle_engine_event(msg.wParam.0 as u32, shimeji::format::animation::EngineEventKind::Jump, None),
                    WM_MASCOT_CLOSE => app.close(msg.wParam.0 as u32),
                    WM_TRAY_CALLBACK => {
                        if (msg.lParam.0 as u32) == WM_RBUTTONUP {
                            show_tray_menu(owner, &mut app);
                        }
                    }
                    WM_DROPFILES => handle_drop(&mut app, HDROP(msg.wParam.0 as *mut _)),
                    _ => {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            } else if last_tick.elapsed() >= Duration::from_millis(TICK_INTERVAL_MS as u64) {
                app.tick(Instant::now());
                last_tick = Instant::now();
            }
        }
    }
    Ok(())
}

unsafe fn handle_drop(app: &mut App, hdrop: HDROP) {
    let count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
    let mut paths = Vec::new();
    for i in 0..count {
        let mut buf = [0u16; 260];
        let len = DragQueryFileW(hdrop, i, Some(&mut buf));
        paths.push(std::path::PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])));
    }
    DragFinish(hdrop);
    for path in filter_zip_paths(&paths) {
        if let Ok(bytes) = std::fs::read(&path) {
            app.import_from_bytes(&bytes);
        }
    }
}

unsafe fn show_tray_menu(owner: HWND, app: &mut App) {
    let menu = CreatePopupMenu().unwrap();
    let mut labels: Vec<(u16, CatalogEntry)> = Vec::new();
    for (id, label) in build_spawn_items(&app.catalog) {
        let entry = app.catalog[(id - shimeji::tray::menu::SPAWN_MENU_ID_BASE) as usize].clone();
        let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = AppendMenuW(menu, MF_STRING, id as usize, windows::core::PCWSTR(wide.as_ptr()));
        labels.push((id, entry));
    }
    let _ = AppendMenuW(menu, MF_STRING, IMPORT_MENU_ID as usize, w!("Import Mascot..."));
    let _ = AppendMenuW(menu, MF_STRING, CLOSE_ALL_MENU_ID as usize, w!("Close All"));
    let _ = AppendMenuW(menu, MF_STRING, EXIT_MENU_ID as usize, w!("Exit"));

    let mut cursor = POINT::default();
    let _ = GetCursorPos(&mut cursor);
    let _ = SetForegroundWindow(owner);
    let choice = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, cursor.x, cursor.y, Some(0), owner, None).0 as u16;

    if choice == IMPORT_MENU_ID {
        if let Some(path) = rfd::FileDialog::new().add_filter("Mascot bundle", &["zip"]).pick_file() {
            if let Ok(bytes) = std::fs::read(&path) {
                app.import_from_bytes(&bytes);
            }
        }
    } else if choice == CLOSE_ALL_MENU_ID {
        app.close_all();
    } else if choice == EXIT_MENU_ID {
        app.close_all();
        PostQuitMessage(0);
    } else if let Some((_, entry)) = labels.into_iter().find(|(id, _)| *id == choice) {
        let _ = app.spawn(&entry.slug);
    }
}
```

```bash
cargo add dirs-next
cargo add rfd
```

(`rfd` provides the native "Open File" dialog for the "Import Mascot..." menu item — it's a thin wrapper over the same Win32 common file dialog APIs, ships no runtime dependency, and keeps `main.rs` from hand-rolling `IFileOpenDialog` COM plumbing.)

- [ ] **Step 3: Build**

Run: `cargo build --release`
Expected: builds successfully, producing `target/release/shimeji.exe` with no other files needed alongside it.

- [ ] **Step 4: Manually verify end-to-end (this is the integration checkpoint — no automated test covers this)**

Run `target/release/shimeji.exe` and confirm, via the tray icon:
1. "Import Mascot..." opens a file picker; selecting `tests/fixtures/8ge8jqm7.zip` succeeds and "usagi" appears under "Spawn".
2. Dragging `tests/fixtures/8ge8jqm7.zip` onto the tray icon also imports it (second import of the same file should show a friendly rejection, per the duplicate-slug behavior from Task 7).
3. Spawning "usagi" shows the mascot falling, then walking on the screen floor.
4. Dragging the mascot with the mouse follows the cursor; releasing with a fast flick sends it flying and it lands/climbs appropriately at a wall.
5. Left-clicking without dragging triggers a tap response.
6. Right-clicking shows "Jump"/"Close"; "Jump" makes it jump; "Close" removes just that instance.
7. Spawning "usagi" a second time runs two independent mascots simultaneously.
8. Open a normal window (e.g. Notepad) and walk the mascot toward it — it should be able to climb onto and walk along the top edge of that window, not just the screen.
9. "Close All" removes every mascot; "Exit" quits the app and removes the tray icon.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/app.rs src/main.rs
git commit -m "feat: wire main loop, tick, and mascot spawn/close (integration)"
```

---

### Task 19: Manual verification pass + README

**Files:**
- Create: `README.md`

**Interfaces:**
- Consumes: the finished `target/release/shimeji.exe` from Task 18.
- Produces: nothing further tasks consume — this is the final task.

- [ ] **Step 1: Write the README**

```markdown
# Shimeji (native Windows, no runtime install)

A desktop mascot app. Import a mascot from a `.zip` (the `legacy_default_v1`
manifest.json + animation.json + sprites/ schema) and it walks, falls, climbs,
and hangs around your desktop — including on top of your other open windows.

## Build

    cargo build --release

Output: `target/release/shimeji.exe`. That single file is the whole app — no
installer, no runtime to install alongside it.

## Run

Double-click `shimeji.exe`, or `cargo run --release`. Use the system tray icon
to import a mascot (drag a `.zip` onto the tray icon, or use "Import
Mascot...") and spawn it.

## Test

    cargo test

Runs the unit and integration tests covering format parsing, bundle
validation, the animation state machine, and the environment surface
geometry. Win32 window/tray/input code is not covered by automated tests —
see the manual verification checklist below.
```

- [ ] **Step 2: Run the full automated test suite**

Run: `cargo test`
Expected: PASS — every test from Tasks 2-12, 14, 15, 16 (all format/state-machine/environment/window-alpha/hit-test/input logic).

- [ ] **Step 3: Re-run the Task 18 manual checklist against the release build**

Run through all 9 checklist items from Task 18, Step 5 again using `target/release/shimeji.exe`, and additionally:
10. If a second monitor is available, drag the mascot across the monitor boundary and confirm it continues to walk correctly rather than falling or getting stuck at the boundary.
11. Copy `shimeji.exe` alone (no other files) to a different folder and run it from there — confirm it still launches and behaves identically, proving there's no hidden dependency on the build directory.

Record the outcome of each item directly in this plan file by checking it off, or note any failures for follow-up.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: add build/run instructions and final verification pass"
```
