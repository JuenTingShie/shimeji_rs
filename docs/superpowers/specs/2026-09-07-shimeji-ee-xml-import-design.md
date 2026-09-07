# shimeji-ee XML Import Design

**Goal:** Let users import mascots straight from shimeji-ee's native distribution format (`conf/actions.xml` + `conf/behaviors.xml` + `img/<Name>/*.png`) — whether that's a bare community-shared character folder or a full Shimeji-ee Java app zip like `test_mascot.zip` — without first running them through the third-party PC exporter tool that produces `pc_import_v1` JSON.

## Background

shimeji_rs's importer (`shimeji::importer::import_zip`) currently only recognizes zips that already contain a `manifest.json` + `animation.json` pair in one of the two supported schemas (`legacy_default_v1`, `pc_import_v1`). Real-world shimeji-ee mascots are distributed as XML instead: `conf/actions.xml` declares named `Action`s (literal sprite-pose lists, composite `Sequence`s chaining other actions, or a handful of actions backed by embedded Java physics classes), and `conf/behaviors.xml` layers a weighted, conditional graph on top that drives idle wandering. Trying to import such a zip today fails outright — there's no `manifest.json` for `MascotBundle::load` to find.

shimeji-ee's format is materially richer than shimeji_rs's own (live EL-expression evaluation, IE-window integration, mascot-splitting, per-frame physics simulation). Full-fidelity support is out of scope; this spec targets a pragmatic subset: whatever maps cleanly onto shimeji_rs's existing `AnimationSchema`/`EventRule`/`auto` model imports and works, whatever doesn't degrades gracefully (a static pose, or is dropped) rather than blocking the import.

## Scope

- **In scope:** detecting a shimeji-ee XML mascot inside an extracted zip (bare character folder or full app distribution), translating its actions/behaviors into a synthesized `manifest.json`/`animation.json` (`pc_import_v1`/`legacy_default_v1`-shaped) that the existing `MascotBundle::load` accepts unmodified, baking real fall/jump physics from the two Java classes that need it, best-effort-translating `behaviors.xml`'s idle graph, and reporting which actions/behaviors couldn't be translated.
- **Not in scope:** EL-expression evaluation, IE-window-specific actions/behaviors (`FallWithIE`, `WalkWithIE`, `ThrowIE`, any `activeIE`-conditioned behavior), mascot-splitting/multi-mascot behaviors (`SplitIntoTwo`, `Divided`, `PullUp`), live-cursor-position-dependent pose switching (`Pinched`'s `FootX`-relative poses collapse to one static pose), multi-character zips (only the first character found is imported), and any UI for surfacing skipped/degraded names (the importer produces the data; wiring it into a dialog is a follow-up).

## Architecture

A new `src/importer/shimeji_ee/` module. `import_zip` gains a fallback branch:

```rust
pub fn import_zip(zip_bytes: &[u8], library_root: &Path) -> Result<ImportResult, ImportError> {
    let scratch = tempfile::tempdir()?;
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    archive.extract(scratch.path())?;

    let skipped = if !scratch.path().join("manifest.json").exists() {
        match shimeji_ee::try_import(scratch.path())? {
            Some(synthesized) => {
                synthesized.write_into(scratch.path())?;
                synthesized.skipped
            }
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let bundle = MascotBundle::load(scratch.path())?;
    // ... existing slug/copy/catalog logic, unchanged ...
    Ok(ImportResult { entry, skipped })
}
```

`ImportResult { pub entry: CatalogEntry, pub skipped: Vec<String> }` replaces `CatalogEntry` as `import_zip`'s success type — `skipped` is always empty for the two existing JSON-based schemas. `src/app.rs`'s `import_from_bytes` updates its `Ok(_entry)` match arm to `Ok(result)`, and logs each skipped name via the existing `crate::logging::log_error("import", ...)` path when the list is non-empty (satisfies "drop but warn" without a new UI surface — matches how every other degraded/failed operation in `App` is already reported).

`try_import` returns `Ok(None)` when no `actions.xml` exists anywhere in the tree (this zip isn't shimeji-ee's format — `MascotBundle::load` will go on to fail with its own `Io`/`ManifestParse` error, unchanged), `Ok(Some(_))` on a successful translation, and `Err(_)` when `actions.xml` was found but couldn't be parsed or mapped (e.g. no matching `img/` folder).

`MascotBundle::load` itself, `StateMachine`, rendering, and the catalog are **untouched** — the synthesized bundle is written to the same scratch directory and re-validated through the existing pipeline exactly like a native JSON bundle.

**New dependency:** `roxmltree` (a small, read-only, allocation-light XML tree parser) for walking `actions.xml`/`behaviors.xml`. shimeji-ee's XML mixes attributes with irregular, order-dependent child elements (`Animation` blocks, `ActionReference` chains) in a shape serde-style struct-per-element deserialization doesn't fit well; direct tree-walking does.

**Supporting change:** `Manifest`, `AnimationSchema`, `Animation`, and related types in `src/format/` currently only derive `Deserialize`. They gain `Serialize` too (purely additive) so the synthesizer builds real `Manifest`/`AnimationSchema` values and writes them with `serde_json::to_string_pretty`, guaranteeing the synthesized bundle matches exactly what the parser expects — no hand-built JSON strings duplicating the schema shape.

### Detection

`try_import` walks the extracted tree for any `actions.xml` (bounded depth, e.g. 6 levels, to handle `test_mascot.zip`'s `shimejiee/conf/actions.xml` nesting). For the first one found:

1. Walk upward from `actions.xml`'s parent until a directory containing an `img/` child is found — call it `app_root`.
2. If `actions.xml`'s parent is `app_root/conf` (the shared/default conf), the character is the first subdirectory of `app_root/img/` (alphabetical order), and `behaviors.xml` is expected alongside `actions.xml` at `app_root/conf/behaviors.xml`.
3. If `actions.xml`'s parent is `app_root/conf/<Name>` (a per-character override), the character is `<Name>`, its sprites live at `app_root/img/<Name>/`, and `behaviors.xml` is expected at `app_root/conf/<Name>/behaviors.xml`.
4. If no `img/<character>/` directory can be resolved, or it contains no `.png` files, return `Err(ShimejiEeError::NoSprites)`.
5. `behaviors.xml` is optional — if missing, idle wandering is simply empty (the mascot only reacts to drag/fling; no autonomous idle switching) rather than failing the import.

Only the first character found is imported; multi-character zips are out of scope (per Scope above).

### XML parsing (`src/importer/shimeji_ee/xml.rs`)

Intermediate structs mirroring shimeji-ee's own vocabulary, populated by walking the `roxmltree::Document`:

```rust
pub struct RawAction {
    pub name: String,
    pub kind: String,              // "Stay" | "Move" | "Animate" | "Sequence" | "Embedded"
    pub border_type: Option<String>, // "Floor" | "Wall" | "Ceiling" (absent => no border)
    pub class: Option<String>,     // Java class, only for kind == "Embedded"
    pub loop_flag: bool,           // Sequence's Loop="true"/"false"
    pub params: HashMap<String, f64>, // Gravity, RegistanceX/Y, VelocityParam, InitialVX/VY -- numeric Embedded attrs
    pub animations: Vec<RawAnimationBlock>, // one per <Animation> child (conditioned or not)
    pub refs: Vec<RawActionRef>,   // one per <ActionReference> child (Sequence actions only)
}

pub struct RawAnimationBlock {
    pub condition: Option<String>, // raw EL string, unevaluated -- only used to know "this Action has >1 pose set"
    pub poses: Vec<RawPose>,
}

pub struct RawPose {
    pub image: String,   // "/shime4.png"
    pub velocity: (i32, i32),
    pub duration: u32,
}

pub struct RawActionRef {
    pub name: String,
    pub duration: Option<String>, // may be a literal int or an EL expression -- see mapping rules
}

pub struct RawBehavior {
    pub name: String,
    pub frequency: u32,
    pub hidden: bool,
    pub condition: Option<String>,
    pub next: Vec<RawBehaviorRef>,
}

pub struct RawBehaviorRef {
    pub name: String,
    pub frequency: u32,
    pub condition: Option<String>,
}

pub fn parse_actions(xml: &str) -> Result<Vec<RawAction>, ShimejiEeError>;
pub fn parse_behaviors(xml: &str) -> Result<Vec<RawBehavior>, ShimejiEeError>;
```

Any attribute string containing `${` or `#{` (an EL expression, e.g. `TargetX="${...}"`) is recorded as `None`/dropped at the point it's consumed by the mapper (see below) rather than parsed further — this module has no expression evaluator.

### Mapping (`src/importer/shimeji_ee/mapping.rs`)

```rust
pub fn map_to_schema(
    actions: &[RawAction],
    behaviors: &[RawBehavior],
    character_name: &str,
) -> (AnimationSchema, Vec<SpriteFile>, Vec<String> /* skipped names */);

pub struct SpriteFile {
    pub source: PathBuf, // e.g. .../img/Shimeji/shime4.png
    pub index: u32,
}
```

Rules, applied per `RawAction`:

- **Literal pose-list actions** (`kind` is `Stay`/`Move`/`Animate`, no `class`): become one `Animation`. `SurfaceType` from `border_type` (`Floor`→`Ground`, `Wall`→`Wall`, `Ceiling`→`Ceiling`, `None`→`Air`). `loop_mode` is `Loop` for `Stay`/`Move` (they run until something else triggers a transition) and `Oneshot` for `Animate`. If the action has more than one `RawAnimationBlock` (conditioned poses, e.g. `Pinched`), only the **last** block (the fallback/no-condition case in every observed shimeji-ee mascot) is used — the rest are dropped and the action's name is added to `skipped`. Each `RawPose` becomes a `Frame`: `dx`/`dy` from `velocity`, `duration_ticks` from `duration`, `sprite` from the sprite-index table built while collecting `SpriteFile`s (see below).
- **`class == "com.group_finity.mascot.action.Fall"`**: baked physics (see Physics baking below) replaces the single literal pose with a real multi-frame falling sequence. `SurfaceType::Air`.
- **`class == "com.group_finity.mascot.action.Jump"`**: same treatment, baked jump arc. `SurfaceType::Air`.
- **`class == "com.group_finity.mascot.action.Dragged"`**: treated as a literal pose-list action per the first rule (it already has multiple `RawAnimationBlock`s from `FootX`-conditioned poses — the "collapse to last block" rule naturally picks one static pose). `SurfaceType::User`, becomes the `drag` animation.
- **Any other `class`** (`FallWithIE`, `WalkWithIE`, `ThrowIE`, `Regist`): treated as a literal pose-list action using only its first `RawAnimationBlock`'s first `Pose`, forced to a single `dx=0,dy=0` frame — degraded, never moving, never broken. Name added to `skipped`.
- **`kind == "Sequence"`**: not turned into its own `Animation`. Instead, each consecutive pair of `refs` becomes a chain link from the first ref's target `Animation` to the second's. shimeji_rs's engine has two distinct autonomous-transition mechanisms and they aren't interchangeable: `on_finish` only fires when a `Oneshot`-loop-mode animation's frames run out naturally, while `on_timer` fires after a tick count elapses regardless of loop mode and can target any animation (`max_duration_ticks` was considered and rejected here — it always transitions to the schema's single global `default_animation`, not an arbitrary step target, so it can't express a chain). The mapper picks per step: if the first ref's target `Animation` has `loop_mode: Oneshot` (an `Animate`-kind action, or a baked `Fall`/`Jump`), the link is an `auto.on_finish` `ChoiceItem` (weight `1.0`) — it ends on its own. If the target is `loop_mode: Loop` (a `Stay`/`Move`-kind action, which otherwise runs forever), the link is an `auto.on_timer` `TimerRule` (`min_ticks == max_ticks`, `chance: 1.0`, one `ChoiceItem` of weight `1.0`) using that ref's literal-integer `duration` attribute as the tick count when present, or a fixed fallback constant (e.g. 60 ticks) when the duration is an EL expression this mapper can't evaluate (shimeji-ee's own duration there is usually "run until some `TargetX`/`TargetY` is reached", which has no literal tick equivalent) -- an approximation, not a drop, so it is not added to `skipped`. If any `ref.name` doesn't resolve to an `Animation` produced by this mapping (an IE-offset `Offset` reference, a reference to an unmapped embedded action, etc.), the **whole Sequence** is dropped and its name added to `skipped` — partial chains aren't synthesized.

**Resolving a named Action to an animation.** A literal (non-Sequence) Action resolves directly to the `Animation` it was mapped to. A Sequence's *entry anchor* is its first surviving step's resolved animation (recursively, if that step is itself a Sequence); its *exit anchor* is its last surviving step's resolved animation. Entry anchors are used below to wire externally-triggered events (drag/fling — the mascot needs to know where a named behavior *starts*); exit anchors are used by the `behaviors.xml` translation (it needs to know where a named behavior *ends*, to attach the "what happens next" choice).

Two specific well-known Sequence names get wired into the top-level `events` the same way `legacy_default_v1` already does, so shimeji_rs's own drag/fling handling picks them up with no new state-machine concept:

- `Fall`'s entry anchor becomes `default_animation` and the sole `initial_candidates` entry, and is targeted by synthesized `DragEnd`/`FlingEnd` events with `from: "*"`.
- `Dragged`'s entry anchor is targeted by a synthesized `DragStart` event with `from: "*"`.

If either `Fall` or `Dragged` can't be resolved (missing from `actions.xml`, or dropped for referencing something unsupported), `map_to_schema` returns `Err(ShimejiEeError::MissingRequiredAction(name))` from `try_import` — these two are load-bearing for the mascot to work at all after being clicked, so a mascot missing them fails the import rather than silently producing a bundle that freezes on release (the exact bug class this project just fixed for `pc_import_v1`).

**Sprite collection:** built from the final mapped output, not the raw XML — every `image` a produced `Frame` actually ends up pointing at (across all kept, non-dropped `Animation`s, including baked-physics and degraded-static ones) is collected into a `Vec<SpriteFile>`, deduplicated, and assigned a stable `0..N` index in first-seen order, which becomes that `Frame`'s `sprite` value. Sprites referenced only by collapsed-away `RawAnimationBlock`s or dropped Actions/Sequences are never collected.

### Physics baking (`src/importer/shimeji_ee/physics.rs`)

Pure functions, no I/O, directly unit-testable:

```rust
/// Replicates com.group_finity.mascot.action.Fall's per-tick integration:
/// vy += gravity each tick; vx *= (1 - resistance_x); vy *= (1 - resistance_y);
/// terminates (returns to Stay) once the mascot would be on the floor -- but since
/// this bakes a fixed frame list with no floor-position feedback, it runs for a fixed
/// `ticks` budget long enough to cover any realistic screen height at max velocity,
/// and the state machine's existing border-transition handling (already unchanged)
/// takes over once the mascot's window actually reaches the floor.
pub fn bake_fall(gravity: f64, resistance_x: f64, resistance_y: f64, sprite: u32, ticks: u32) -> Vec<Frame>;

/// Replicates com.group_finity.mascot.action.Jump's initial upward velocity decaying
/// under the same per-tick gravity used by Fall (shimeji-ee shares one gravity constant
/// across both), producing a rise-then-fall arc from a single upward VelocityParam.
pub fn bake_jump(velocity_param: f64, sprite: u32, ticks: u32) -> Vec<Frame>;
```

Each produces one `Frame` per simulated tick with `dx`/`dy` rounded to the nearest integer (shimeji_rs's `Frame::dx`/`dy` are `i32`) and `duration_ticks: 1`, reusing the Action's single authored sprite index for every frame (shimeji-ee's own `Fall`/`Jump` classes don't switch sprites based on velocity in the mascots inspected — `test_mascot.zip`'s `Falling` uses `shime4.png` throughout). `ticks` is a fixed constant (e.g. 240 — four seconds at 60 ticks/sec, comfortably more than any realistic screen-height fall) chosen once in `mapping.rs`, not per-mascot.

### behaviors.xml → idle wandering

For each `RawBehavior` with at least one `RawBehaviorRef` in `next`, whose own name resolves (via its **exit anchor**, defined above) to a mapped Animation: the surviving refs (see dropping rules below) become `ChoiceItem`s (`weight` = `frequency as f64`, `to` = the ref's entry anchor). Where they attach depends on the exit anchor's `loop_mode`, using the same rule as Sequence-step chaining: `auto.on_finish` if it's `Oneshot` (fires when its frames naturally run out), or an `auto.on_timer` `TimerRule` (`min_ticks == max_ticks` = a fixed fallback constant, e.g. 300 ticks/5s — `behaviors.xml` gives no duration at all, unlike a Sequence's `ActionReference`, so there's no literal value to prefer here — `chance: 1.0`) if it's `Loop`.

A ref is dropped (and the *ref's* name — not the whole behavior — added to `skipped`) when its `condition` is present (any `Condition` attribute at all — none of them are evaluable without an EL engine) or when its `name` doesn't resolve to a mapped Animation. A `RawBehavior` whose own name doesn't resolve to any mapped Animation (e.g. it's an alias for a dropped Sequence) is skipped entirely, name added to `skipped`. If a Behavior's exit anchor already received an `on_finish`/`on_timer` rule from Sequence-step chaining (mapping runs actions first, then behaviors), and the mechanism matches (both `on_finish`, or both `on_timer`), the behaviors.xml choices are appended to that existing rule's `choices` rather than overwriting it. On the rare mismatch (chaining wants `on_finish`, `behaviors.xml` wants `on_timer`, or vice versa — both apply to the same anchor), the actions.xml-derived rule wins since it may be load-bearing for `Fall`/`Dragged`, and the whole Behavior is skipped, name added to `skipped`.

## Error handling

New `ShimejiEeError` (`thiserror`), covering: `Xml(roxmltree::Error)` (malformed XML), `NoSprites` (resolved character folder has no `.png` files), `MissingRequiredAction(String)` (`Fall` or `Dragged` unresolvable — see Mapping above), `Io(std::io::Error)`. `ImportError` gains `ShimejiEe(#[from] ShimejiEeError)`.

## Testing

A hand-authored synthetic fixture, `tests/fixtures/fixture_shimeji_ee/`, mirroring the existing `fixture_bundle` convention (no real character's art or authored animation data):

- `conf/actions.xml`: a `Stand` (Stay), `Walk` (Move, two poses), `Falling` (Embedded, `class="com.group_finity.mascot.action.Fall"`, made-up `Gravity`/`RegistanceX`/`RegistanceY`), `Fall` (Sequence wrapping `Falling`), `Pinched` (Embedded `Dragged`, two conditioned `Animation` blocks to exercise the collapse-to-last rule), `Dragged` (Sequence wrapping `Pinched`), one IE-specific embedded action (e.g. a made-up `WalkWithIe`) to exercise the degraded-static-pose path, and one Sequence referencing that IE action to exercise whole-Sequence dropping.
- `conf/behaviors.xml`: `Stand`→{`Walk` weight 100, `SitAndSpin` weight 1 (name deliberately unresolvable, to exercise ref-dropping)}, and one `Behavior` gated by a made-up `Condition` attribute to exercise condition-based dropping.
- `img/TestMascot/`: a handful of tiny placeholder `.png` files (same pattern as the existing `sprites/0000.webp` fixture, generated the same way).

Unit tests:
- `xml.rs`: parses the fixture's `actions.xml`/`behaviors.xml` into the expected `RawAction`/`RawBehavior` shapes.
- `physics.rs`: `bake_fall`/`bake_jump` produce monotonically-increasing `dy` (resp. rise-then-fall) sequences of the expected length; a zero-gravity input produces all-zero frames.
- `mapping.rs`: the fixture's `RawAction`/`RawBehavior` lists map to an `AnimationSchema` with the expected `default_animation`, `drag`/`fall` `events`, the IE action's dropped Sequence and dropped ref both appearing in `skipped`, and the condition-gated behavior ref also in `skipped`.

Integration test (`tests/importer.rs`, alongside the existing zip-import tests): zip up `fixture_shimeji_ee/` (both as a bare character folder and nested under a `shimejiee/` app-root prefix, to cover both detection shapes from the Detection section) and confirm `import_zip` succeeds, the resulting `CatalogEntry`'s bundle loads via `MascotBundle::load` with the expected animation keys, and `ImportResult::skipped` contains the expected dropped names.
