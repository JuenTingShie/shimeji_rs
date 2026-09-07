# shimeji-ee XML Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let `shimeji::importer::import_zip` accept shimeji-ee's native `conf/actions.xml` + `conf/behaviors.xml` + `img/<Name>/*.png` distribution format, translating it into a synthesized `manifest.json`/`animation.json` bundle that the existing loader/state-machine/rendering pipeline consumes unmodified.

**Architecture:** A new `src/importer/shimeji_ee/` module (`xml.rs` parses the XML into raw structs, `physics.rs` bakes real fall/jump frame sequences from shimeji-ee's known physics formulas, `mapping.rs` translates raw actions/behaviors into shimeji_rs's `AnimationSchema`, `detect.rs` locates the XML inside an extracted zip, `mod.rs` orchestrates all of it into a `try_import`/`write_into` pair). `import_zip` gains a fallback branch that runs this translation before handing off to the existing, unmodified `MascotBundle::load`.

**Tech Stack:** Rust, `roxmltree` (new dependency, for read-only XML tree walking), existing `serde`/`serde_json`/`zip`/`tempfile`/`image` dependencies.

**Spec:** `docs/superpowers/specs/2026-09-07-shimeji-ee-xml-import-design.md`

## Global Constraints

- No EL-expression evaluation anywhere (`${...}`/`#{...}` attribute values are either used as literal strings when they parse as plain numbers/integers, or otherwise treated as "unavailable").
- IE-window actions/behaviors, mascot-splitting, and multi-cursor-pose dragging poses degrade gracefully (single static frame, or dropped) rather than erroring.
- Only the first character folder found is imported; multi-character zips are out of scope.
- Real/licensed mascot content is never checked into the repo — all test fixtures are hand-authored/synthetic (project convention, see commit `98c821c`).
- `MascotBundle::load`, `StateMachine`, rendering, and the catalog module are never modified by this feature — the synthesized bundle must satisfy their existing, unchanged contracts.

---

### Task 1: Add `roxmltree` dependency and `Serialize` to the format types

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/format/manifest.rs`
- Modify: `src/format/animation.rs`
- Test: inline `#[cfg(test)]` round-trip tests in both files above

**Interfaces:**
- Produces: every public type in `shimeji::format::manifest` and `shimeji::format::animation` now implements both `serde::Serialize` and `serde::Deserialize` — later tasks build `Manifest`/`AnimationSchema` values in memory and write them out with `serde_json::to_string_pretty`.

- [ ] **Step 1: Write the failing round-trip tests**

Add to `src/format/manifest.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn manifest_round_trips_through_json() {
        let json = std::fs::read_to_string("tests/fixtures/fixture_manifest.json").unwrap();
        let manifest: Manifest = serde_json::from_str(&json).unwrap();
        let re_encoded = serde_json::to_string(&manifest).unwrap();
        let round_tripped: Manifest = serde_json::from_str(&re_encoded).unwrap();
        assert_eq!(round_tripped.name, manifest.name);
        assert_eq!(round_tripped.sprites.sprite_count, manifest.sprites.sprite_count);
    }
```

Add to `src/format/animation.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn animation_schema_round_trips_through_json() {
        let json = std::fs::read_to_string("tests/fixtures/fixture_animation.json").unwrap();
        let schema: AnimationSchema = serde_json::from_str(&json).unwrap();
        let re_encoded = serde_json::to_string(&schema).unwrap();
        let round_tripped: AnimationSchema = serde_json::from_str(&re_encoded).unwrap();
        assert_eq!(round_tripped.animations.len(), schema.animations.len());
        assert_eq!(round_tripped.events.len(), schema.events.len());
    }
```

- [ ] **Step 2: Run the tests to verify they fail to compile**

Run: `cargo test --lib manifest_round_trips_through_json animation_schema_round_trips_through_json`
Expected: compile error, `the trait bound Manifest: Serialize is not satisfied` (and likewise for `AnimationSchema`) — neither type derives `Serialize` yet.

- [ ] **Step 3: Add `roxmltree` to Cargo.toml**

Run: `cargo add roxmltree`
Expected: adds a `roxmltree = "<resolved-version>"` line under `[dependencies]` in `Cargo.toml` and updates `Cargo.lock`.

- [ ] **Step 4: Add `Serialize` to every type in `src/format/manifest.rs`**

Change every derive line in the file from `#[derive(Debug, Clone, Deserialize)]` to `#[derive(Debug, Clone, Serialize, Deserialize)]`, and the file's import line from `use serde::Deserialize;` to `use serde::{Deserialize, Serialize};`. This touches: `Manifest`, `AnimationSchemaRef`, `SpriteSheetInfo`, `PreviewInfo`, `AuthorInfo`, `LicenseInfo` (six structs total — every `#[derive(Debug, Clone, Deserialize)]` occurrence in the file).

- [ ] **Step 5: Add `Serialize` to every type in `src/format/animation.rs`**

Change the file's import line from `use serde::Deserialize;` to `use serde::{Deserialize, Serialize};`. Then update every derive line:

- `AnimationSchema`, `Frame`, `ChoiceItem`, `TimerRule`, `MaxDurationTicks`, `BorderTransition`, `EventRule`: `#[derive(Debug, Clone, Deserialize)]` → `#[derive(Debug, Clone, Serialize, Deserialize)]`.
- `Animation`: `#[derive(Debug, Clone, Deserialize)]` → `#[derive(Debug, Clone, Serialize, Deserialize)]` (the `#[serde(rename_all = "camelCase")]` line above it is unchanged — it governs both directions once both derives are present).
- `AutoBehavior`: `#[derive(Debug, Clone, Default, Deserialize)]` → `#[derive(Debug, Clone, Default, Serialize, Deserialize)]`.
- `Edge`, `Direction`, `SurfaceType`, `LoopMode`: `#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]` → `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]`.
- `EngineEventKind`: `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]` → `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]`.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test --lib manifest_round_trips_through_json animation_schema_round_trips_through_json`
Expected: both PASS.

- [ ] **Step 7: Run the full existing test suite to confirm no regression**

Run: `cargo test`
Expected: all existing tests still pass (adding `Serialize` is purely additive and doesn't change any `Deserialize` behavior).

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock src/format/manifest.rs src/format/animation.rs
git commit -m "feat: add Serialize to format types and roxmltree dependency

Needed by the upcoming shimeji-ee XML importer, which builds Manifest
and AnimationSchema values in memory and writes them out as JSON
rather than parsing them from a file."
```

---

### Task 2: Physics baking (`src/importer/shimeji_ee/physics.rs`)

**Files:**
- Create: `src/importer/shimeji_ee/physics.rs`
- Modify: `src/importer/mod.rs` (add `pub mod shimeji_ee;`)
- Create: `src/importer/shimeji_ee/mod.rs` (minimal skeleton, just enough to host `physics`)

**Interfaces:**
- Consumes: `crate::format::animation::Frame { sprite: u32, dx: i32, dy: i32, duration_ticks: u32 }` (existing type from Task 1).
- Produces: `pub fn bake_fall(gravity: f64, resistance_x: f64, resistance_y: f64, sprite: u32, ticks: u32) -> Vec<Frame>` and `pub fn bake_jump(velocity_param: f64, sprite: u32, ticks: u32) -> Vec<Frame>`, both in `shimeji::importer::shimeji_ee::physics`. Later tasks (4, 7) call these by these exact names and signatures.

- [ ] **Step 1: Create the module skeleton**

Create `src/importer/shimeji_ee/mod.rs`:

```rust
pub mod physics;
```

Add to `src/importer/mod.rs`, right after `pub mod catalog;`:

```rust
pub mod shimeji_ee;
```

- [ ] **Step 2: Write the failing tests**

Create `src/importer/shimeji_ee/physics.rs`:

```rust
use crate::format::animation::Frame;

pub fn bake_fall(gravity: f64, resistance_x: f64, resistance_y: f64, sprite: u32, ticks: u32) -> Vec<Frame> {
    unimplemented!()
}

pub fn bake_jump(velocity_param: f64, sprite: u32, ticks: u32) -> Vec<Frame> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bake_fall_accelerates_downward_under_gravity() {
        let frames = bake_fall(2.0, 0.0, 0.0, 4, 5);
        assert_eq!(frames.len(), 5);
        assert_eq!(frames[0].dy, 2);
        assert_eq!(frames[1].dy, 4);
        assert_eq!(frames[4].dy, 10);
        assert!(frames.iter().all(|f| f.sprite == 4 && f.duration_ticks == 1 && f.dx == 0));
    }

    #[test]
    fn bake_fall_with_zero_gravity_never_moves() {
        let frames = bake_fall(0.0, 0.0, 0.0, 4, 3);
        assert!(frames.iter().all(|f| f.dx == 0 && f.dy == 0));
    }

    #[test]
    fn bake_fall_resistance_decays_velocity() {
        // 50% resistance each tick roughly halves the accumulated velocity every step,
        // so it must grow much slower than the zero-resistance case.
        let with_resistance = bake_fall(4.0, 0.0, 0.5, 0, 4);
        let without_resistance = bake_fall(4.0, 0.0, 0.0, 0, 4);
        assert!(with_resistance[3].dy < without_resistance[3].dy);
    }

    #[test]
    fn bake_jump_rises_before_falling() {
        let frames = bake_jump(20.0, 7, 60);
        assert!(frames[0].dy < 0, "jump must start moving upward, got {}", frames[0].dy);
        let (min_index, _) = frames.iter().enumerate().min_by_key(|(_, f)| f.dy).unwrap();
        assert!(min_index > 0, "should keep rising for at least one tick before falling");
        assert!(
            frames.last().unwrap().dy > frames[min_index].dy,
            "should be falling again by the last simulated tick"
        );
        assert!(frames.iter().all(|f| f.sprite == 7 && f.duration_ticks == 1 && f.dx == 0));
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib -p shimeji shimeji_ee::physics`
Expected: FAIL — `not implemented` panics from the `unimplemented!()` bodies.

- [ ] **Step 4: Implement `bake_fall`/`bake_jump`**

Replace the two `unimplemented!()` function bodies in `src/importer/shimeji_ee/physics.rs` with:

```rust
use crate::format::animation::Frame;

/// shimeji-ee's default gravity for com.group_finity.mascot.action.Jump when the action
/// doesn't declare its own -- Jump only ever exposes VelocityParam in the mascots inspected,
/// never a Gravity override, so this baked implementation reuses shimeji-ee's own well-known
/// default gravity constant.
const JUMP_GRAVITY: f64 = 1.0;

/// Replicates com.group_finity.mascot.action.Fall's per-tick integration: velocity
/// accumulates downward gravity every tick, decaying by the configured resistance factors.
/// Runs for a fixed `ticks` budget (chosen by the caller) rather than until the mascot
/// reaches the floor -- this module has no floor-position feedback -- and the state
/// machine's existing border-transition handling takes over once the window actually
/// reaches the floor, ending the fall well before the budget is likely to run out.
pub fn bake_fall(gravity: f64, resistance_x: f64, resistance_y: f64, sprite: u32, ticks: u32) -> Vec<Frame> {
    simulate(0.0, 0.0, gravity, resistance_x, resistance_y, sprite, ticks)
}

/// Replicates com.group_finity.mascot.action.Jump: an initial upward velocity (from the
/// action's VelocityParam) decaying under gravity, producing a rise-then-fall arc.
pub fn bake_jump(velocity_param: f64, sprite: u32, ticks: u32) -> Vec<Frame> {
    simulate(0.0, -velocity_param, JUMP_GRAVITY, 0.0, 0.0, sprite, ticks)
}

fn simulate(vx0: f64, vy0: f64, gravity: f64, resistance_x: f64, resistance_y: f64, sprite: u32, ticks: u32) -> Vec<Frame> {
    let mut vx = vx0;
    let mut vy = vy0;
    let mut frames = Vec::with_capacity(ticks as usize);
    for _ in 0..ticks {
        vy += gravity;
        vx *= 1.0 - resistance_x;
        vy *= 1.0 - resistance_y;
        frames.push(Frame { sprite, dx: vx.round() as i32, dy: vy.round() as i32, duration_ticks: 1 });
    }
    frames
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib -p shimeji shimeji_ee::physics`
Expected: all 4 tests PASS.

- [ ] **Step 6: Run the full suite**

Run: `cargo test`
Expected: all existing tests still pass; the new `shimeji_ee` module compiles cleanly (it's `pub`, so no dead-code warnings even though nothing calls it yet).

- [ ] **Step 7: Commit**

```bash
git add src/importer/mod.rs src/importer/shimeji_ee/mod.rs src/importer/shimeji_ee/physics.rs
git commit -m "feat: bake shimeji-ee Fall/Jump physics into frame sequences

shimeji-ee's Fall/Jump actions are backed by live Java physics classes
with no baked animation data, just a single static pose + parameters.
shimeji_rs has no live-physics engine -- every animation it plays is a
pre-authored frame list -- so this reimplements the two classes'
known gravity/resistance formulas to synthesize a real multi-frame
sequence from that single pose, matching what pc_import_v1's own
exporter already does for the same problem."
```

---

### Task 3: XML parsing (`src/importer/shimeji_ee/xml.rs`) and the `ShimejiEeError` type

**Files:**
- Create: `src/importer/shimeji_ee/xml.rs`
- Modify: `src/importer/shimeji_ee/mod.rs` (add `pub mod xml;` and `ShimejiEeError`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `RawAction`, `RawAnimationBlock`, `RawPose`, `RawActionRef`, `RawBehavior`, `RawBehaviorRef` (all `pub`, in `shimeji_ee::xml`), `pub fn parse_actions(xml: &str) -> Result<Vec<RawAction>, ShimejiEeError>`, `pub fn parse_behaviors(xml: &str) -> Result<Vec<RawBehavior>, ShimejiEeError>`. `pub enum ShimejiEeError` in `shimeji_ee` (mod.rs), with one variant so far: `Xml(#[from] roxmltree::Error)`. Task 4 adds `MissingRequiredAction`, Task 6 adds `NoSprites`, Task 7 adds `Io`.

- [ ] **Step 1: Add `ShimejiEeError` and declare the module**

Replace the contents of `src/importer/shimeji_ee/mod.rs` with:

```rust
pub mod physics;
pub mod xml;

#[derive(Debug, thiserror::Error)]
pub enum ShimejiEeError {
    #[error("invalid shimeji-ee XML: {0}")]
    Xml(#[from] roxmltree::Error),
}
```

- [ ] **Step 2: Write the failing tests**

Create `src/importer/shimeji_ee/xml.rs`:

```rust
use super::ShimejiEeError;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct RawAction {
    pub name: String,
    pub kind: String,
    pub border_type: Option<String>,
    pub class: Option<String>,
    pub loop_flag: bool,
    pub params: HashMap<String, f64>,
    pub animations: Vec<RawAnimationBlock>,
    pub refs: Vec<RawActionRef>,
}

#[derive(Debug, Clone, Default)]
pub struct RawAnimationBlock {
    pub condition: Option<String>,
    pub poses: Vec<RawPose>,
}

#[derive(Debug, Clone)]
pub struct RawPose {
    pub image: String,
    pub velocity: (i32, i32),
    pub duration: u32,
}

#[derive(Debug, Clone)]
pub struct RawActionRef {
    pub name: String,
    pub duration: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RawBehavior {
    pub name: String,
    pub frequency: u32,
    pub hidden: bool,
    pub condition: Option<String>,
    pub next: Vec<RawBehaviorRef>,
}

#[derive(Debug, Clone)]
pub struct RawBehaviorRef {
    pub name: String,
    pub frequency: u32,
    pub condition: Option<String>,
}

pub fn parse_actions(xml: &str) -> Result<Vec<RawAction>, ShimejiEeError> {
    unimplemented!()
}

pub fn parse_behaviors(xml: &str) -> Result<Vec<RawBehavior>, ShimejiEeError> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACTIONS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<Mascot xmlns="http://www.group-finity.com/Mascot">
    <ActionList>
        <Action Name="Stand" Type="Stay" BorderType="Floor">
            <Animation>
                <Pose Image="/stand.png" ImageAnchor="64,128" Velocity="0,0" Duration="250" />
            </Animation>
        </Action>
        <Action Name="Falling" Type="Embedded" Class="com.group_finity.mascot.action.Fall" Gravity="2" RegistanceX="0.05" RegistanceY="0.1">
            <Animation>
                <Pose Image="/falling.png" ImageAnchor="64,128" Velocity="0,0" Duration="250" />
            </Animation>
        </Action>
        <Action Name="Pinched" Type="Embedded" Class="com.group_finity.mascot.action.Dragged">
            <Animation Condition="#{FootX &lt; mascot.environment.cursor.x}">
                <Pose Image="/pinched_left.png" ImageAnchor="64,128" Velocity="0,0" Duration="5" />
            </Animation>
            <Animation Condition="#{FootX &gt;= mascot.environment.cursor.x}">
                <Pose Image="/pinched_right.png" ImageAnchor="64,128" Velocity="0,0" Duration="5" />
            </Animation>
        </Action>
    </ActionList>
    <ActionList>
        <Action Name="Fall" Type="Sequence" Loop="false">
            <ActionReference Name="Falling"/>
        </Action>
        <Action Name="ThrowIe" Type="Sequence" Loop="false">
            <ActionReference Name="WalkWithIe" TargetX="${mascot.environment.activeIE.left}" />
            <ActionReference Name="Stand" Duration="20" />
        </Action>
    </ActionList>
</Mascot>"#;

    const BEHAVIORS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<Mascot xmlns="http://www.group-finity.com/Mascot">
    <BehaviorList>
        <Behavior Name="Stand" Frequency="0">
            <NextBehavior Add="false">
                <BehaviorReference Name="Walk" Frequency="100" />
                <BehaviorReference Name="ChaseMouse" Frequency="50" Condition="${mascot.environment.activeIE.exists}" />
            </NextBehavior>
        </Behavior>
        <Condition Condition="#{mascot.environment.floor.isOn(mascot.anchor)}">
            <Behavior Name="Walk" Frequency="50" />
        </Condition>
    </BehaviorList>
</Mascot>"#;

    #[test]
    fn parses_a_literal_pose_list_action() {
        let actions = parse_actions(ACTIONS_XML).unwrap();
        let stand = actions.iter().find(|a| a.name == "Stand").unwrap();
        assert_eq!(stand.kind, "Stay");
        assert_eq!(stand.border_type.as_deref(), Some("Floor"));
        assert!(stand.class.is_none());
        assert_eq!(stand.animations.len(), 1);
        assert_eq!(stand.animations[0].poses.len(), 1);
        assert_eq!(stand.animations[0].poses[0].image, "/stand.png");
        assert_eq!(stand.animations[0].poses[0].velocity, (0, 0));
        assert_eq!(stand.animations[0].poses[0].duration, 250);
    }

    #[test]
    fn parses_embedded_action_params_as_numbers() {
        let actions = parse_actions(ACTIONS_XML).unwrap();
        let falling = actions.iter().find(|a| a.name == "Falling").unwrap();
        assert_eq!(falling.class.as_deref(), Some("com.group_finity.mascot.action.Fall"));
        assert_eq!(falling.params.get("Gravity"), Some(&2.0));
        assert_eq!(falling.params.get("RegistanceX"), Some(&0.05));
        assert_eq!(falling.params.get("RegistanceY"), Some(&0.1));
    }

    #[test]
    fn parses_multiple_conditioned_animation_blocks() {
        let actions = parse_actions(ACTIONS_XML).unwrap();
        let pinched = actions.iter().find(|a| a.name == "Pinched").unwrap();
        assert_eq!(pinched.animations.len(), 2);
        assert!(pinched.animations[0].condition.is_some());
        assert_eq!(pinched.animations[1].poses[0].image, "/pinched_right.png");
    }

    #[test]
    fn parses_sequence_action_references_including_el_and_literal_durations() {
        let actions = parse_actions(ACTIONS_XML).unwrap();
        let fall = actions.iter().find(|a| a.name == "Fall").unwrap();
        assert_eq!(fall.kind, "Sequence");
        assert_eq!(fall.refs.len(), 1);
        assert_eq!(fall.refs[0].name, "Falling");

        let throw = actions.iter().find(|a| a.name == "ThrowIe").unwrap();
        assert_eq!(throw.refs.len(), 2);
        assert_eq!(throw.refs[1].name, "Stand");
        assert_eq!(throw.refs[1].duration.as_deref(), Some("20"));
    }

    #[test]
    fn rejects_malformed_xml() {
        let err = parse_actions("<not-even-closed>").unwrap_err();
        assert!(matches!(err, ShimejiEeError::Xml(_)));
    }

    #[test]
    fn parses_behavior_next_references_with_conditions() {
        let behaviors = parse_behaviors(BEHAVIORS_XML).unwrap();
        let stand = behaviors.iter().find(|b| b.name == "Stand").unwrap();
        assert_eq!(stand.next.len(), 2);
        assert_eq!(stand.next[0].name, "Walk");
        assert_eq!(stand.next[0].frequency, 100);
        assert!(stand.next[0].condition.is_none());
        assert_eq!(stand.next[1].name, "ChaseMouse");
        assert!(stand.next[1].condition.is_some());
    }

    #[test]
    fn behavior_inside_a_condition_wrapper_inherits_its_condition() {
        let behaviors = parse_behaviors(BEHAVIORS_XML).unwrap();
        let walk = behaviors.iter().find(|b| b.name == "Walk").unwrap();
        assert!(walk.condition.is_some());
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib -p shimeji shimeji_ee::xml`
Expected: FAIL — `not implemented` panics.

- [ ] **Step 4: Implement `parse_actions`/`parse_behaviors`**

Replace the two `unimplemented!()` bodies and add the helper functions below them in `src/importer/shimeji_ee/xml.rs` (everything above `#[cfg(test)]` stays; only the two `pub fn` bodies change and new private functions are added after them):

```rust
pub fn parse_actions(xml: &str) -> Result<Vec<RawAction>, ShimejiEeError> {
    let doc = roxmltree::Document::parse(xml)?;
    let mut actions = Vec::new();
    for list in element_children(doc.root_element(), "ActionList") {
        for node in element_children(list, "Action") {
            actions.push(parse_action(node));
        }
    }
    Ok(actions)
}

pub fn parse_behaviors(xml: &str) -> Result<Vec<RawBehavior>, ShimejiEeError> {
    let doc = roxmltree::Document::parse(xml)?;
    let mut behaviors = Vec::new();
    collect_behaviors(doc.root_element(), None, &mut behaviors);
    Ok(behaviors)
}

fn element_children<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    tag: &str,
) -> Vec<roxmltree::Node<'a, 'input>> {
    node.children()
        .filter(|n| n.is_element())
        .flat_map(|child| {
            if child.tag_name().name() == tag {
                vec![child]
            } else {
                element_children(child, tag)
            }
        })
        .collect()
}

fn parse_action(node: roxmltree::Node) -> RawAction {
    let name = node.attribute("Name").unwrap_or_default().to_string();
    let kind = node.attribute("Type").unwrap_or_default().to_string();
    let border_type = node.attribute("BorderType").map(str::to_string);
    let class = node.attribute("Class").map(str::to_string);
    let loop_flag = node.attribute("Loop") == Some("true");

    let mut params = HashMap::new();
    for attr in node.attributes() {
        if let Ok(value) = attr.value().parse::<f64>() {
            params.insert(attr.name().to_string(), value);
        }
    }

    let mut animations = Vec::new();
    let mut refs = Vec::new();
    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "Animation" => animations.push(parse_animation_block(child)),
            "ActionReference" => refs.push(parse_action_ref(child)),
            _ => {}
        }
    }

    RawAction { name, kind, border_type, class, loop_flag, params, animations, refs }
}

fn parse_animation_block(node: roxmltree::Node) -> RawAnimationBlock {
    let condition = node.attribute("Condition").map(str::to_string);
    let poses = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "Pose")
        .map(parse_pose)
        .collect();
    RawAnimationBlock { condition, poses }
}

fn parse_pose(node: roxmltree::Node) -> RawPose {
    let image = node.attribute("Image").unwrap_or_default().to_string();
    let velocity = node.attribute("Velocity").map(parse_velocity).unwrap_or((0, 0));
    let duration = node.attribute("Duration").and_then(|v| v.parse().ok()).unwrap_or(1);
    RawPose { image, velocity, duration }
}

fn parse_velocity(value: &str) -> (i32, i32) {
    let mut parts = value.split(',').map(|p| p.trim().parse::<i32>().unwrap_or(0));
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0))
}

fn parse_action_ref(node: roxmltree::Node) -> RawActionRef {
    RawActionRef {
        name: node.attribute("Name").unwrap_or_default().to_string(),
        duration: node.attribute("Duration").map(str::to_string),
    }
}

fn collect_behaviors(node: roxmltree::Node, inherited_condition: Option<&str>, out: &mut Vec<RawBehavior>) {
    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "Behavior" => {
                let mut behavior = parse_behavior(child);
                if behavior.condition.is_none() {
                    behavior.condition = inherited_condition.map(str::to_string);
                }
                out.push(behavior);
            }
            "BehaviorList" => collect_behaviors(child, inherited_condition, out),
            "Condition" => {
                let condition = child.attribute("Condition").or(inherited_condition);
                collect_behaviors(child, condition, out);
            }
            _ => {}
        }
    }
}

fn parse_behavior(node: roxmltree::Node) -> RawBehavior {
    let name = node.attribute("Name").unwrap_or_default().to_string();
    let frequency = node.attribute("Frequency").and_then(|v| v.parse().ok()).unwrap_or(0);
    let hidden = node.attribute("Hidden") == Some("true");
    let condition = node.attribute("Condition").map(str::to_string);

    let mut next = Vec::new();
    for next_behavior in node.children().filter(|n| n.is_element() && n.tag_name().name() == "NextBehavior") {
        for reference in next_behavior.children().filter(|n| n.is_element() && n.tag_name().name() == "BehaviorReference") {
            next.push(RawBehaviorRef {
                name: reference.attribute("Name").unwrap_or_default().to_string(),
                frequency: reference.attribute("Frequency").and_then(|v| v.parse().ok()).unwrap_or(0),
                condition: reference.attribute("Condition").map(str::to_string),
            });
        }
    }

    RawBehavior { name, frequency, hidden, condition, next }
}
```

Note: `element_children` recurses into non-matching elements too (so `parse_actions` finds `Action` elements even though real files sometimes nest an extra wrapper, and finds both of `test_mascot.zip`'s two top-level `<ActionList>` blocks) — this is deliberately more permissive than a single flat `.children()` scan.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib -p shimeji shimeji_ee::xml`
Expected: all 8 tests PASS.

- [ ] **Step 6: Run the full suite**

Run: `cargo test`
Expected: all existing tests still pass.

- [ ] **Step 7: Commit**

```bash
git add src/importer/shimeji_ee/mod.rs src/importer/shimeji_ee/xml.rs
git commit -m "feat: parse shimeji-ee actions.xml/behaviors.xml into raw structs

Uses roxmltree for direct tree-walking rather than serde-style
struct-per-element deserialization, since the XML mixes attributes
with irregular, order-dependent child elements (repeated Animation
blocks, ActionReference chains) that don't fit a derived Deserialize
shape well."
```

---

### Task 4: Action mapping (`src/importer/shimeji_ee/mapping.rs`, part A)

**Files:**
- Create: `src/importer/shimeji_ee/mapping.rs`
- Modify: `src/importer/shimeji_ee/mod.rs` (add `pub mod mapping;`, add `MissingRequiredAction` error variant)

**Interfaces:**
- Consumes: `xml::{RawAction, RawAnimationBlock, RawPose}` (Task 3), `physics::{bake_fall, bake_jump}` (Task 2), `crate::format::animation::{Animation, AutoBehavior, ChoiceItem, Direction, EngineEventKind, EventRule, Frame, LoopMode, SurfaceType, TimerRule}` (Task 1).
- Produces: `pub struct SpriteFile { pub source: PathBuf, pub index: u32 }`, and a private `map_actions(actions: &[RawAction], img_dir: &Path) -> Result<MappedActions, ShimejiEeError>` whose `MappedActions { animations: Vec<Animation>, events: Vec<EventRule>, default_animation: String, sprites: Vec<SpriteFile>, skipped: Vec<String>, leaf: HashMap<String, String>, sequences: HashMap<String, (String, String)> }` fields are consumed directly by Task 5 (which lives in the same file) and, via `map_to_schema`, by Task 7.

- [ ] **Step 1: Add the `MissingRequiredAction` error variant and declare the module**

In `src/importer/shimeji_ee/mod.rs`, add `pub mod mapping;` below `pub mod xml;`, and add a variant to `ShimejiEeError`:

```rust
    #[error("required action '{0}' is missing or could not be translated")]
    MissingRequiredAction(String),
```

- [ ] **Step 2: Write the failing tests**

Create `src/importer/shimeji_ee/mapping.rs`:

```rust
use super::physics::{bake_fall, bake_jump};
use super::xml::{RawAction, RawAnimationBlock, RawPose};
use super::ShimejiEeError;
use crate::format::animation::{
    Animation, AutoBehavior, ChoiceItem, Direction, EngineEventKind, EventRule, Frame, LoopMode, SurfaceType, TimerRule,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const PHYSICS_TICKS: u32 = 240;
const SEQUENCE_STEP_FALLBACK_TICKS: u32 = 60;

#[derive(Debug, Clone)]
pub struct SpriteFile {
    pub source: PathBuf,
    pub index: u32,
}

pub(super) struct MappedActions {
    pub animations: Vec<Animation>,
    pub events: Vec<EventRule>,
    pub default_animation: String,
    pub sprites: Vec<SpriteFile>,
    pub skipped: Vec<String>,
    pub leaf: HashMap<String, String>,
    pub sequences: HashMap<String, (String, String)>,
}

pub(super) fn map_actions(actions: &[RawAction], img_dir: &Path) -> Result<MappedActions, ShimejiEeError> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literal_action(name: &str, kind: &str, border_type: Option<&str>, poses: Vec<RawPose>) -> RawAction {
        RawAction {
            name: name.to_string(),
            kind: kind.to_string(),
            border_type: border_type.map(str::to_string),
            class: None,
            loop_flag: false,
            params: HashMap::new(),
            animations: vec![RawAnimationBlock { condition: None, poses }],
            refs: Vec::new(),
        }
    }

    fn pose(image: &str, dx: i32, dy: i32, duration: u32) -> RawPose {
        RawPose { image: image.to_string(), velocity: (dx, dy), duration }
    }

    fn sequence(name: &str, refs: Vec<(&str, Option<&str>)>) -> RawAction {
        RawAction {
            name: name.to_string(),
            kind: "Sequence".to_string(),
            border_type: None,
            class: None,
            loop_flag: false,
            params: HashMap::new(),
            animations: Vec::new(),
            refs: refs
                .into_iter()
                .map(|(n, d)| super::super::xml::RawActionRef { name: n.to_string(), duration: d.map(str::to_string) })
                .collect(),
        }
    }

    fn fall_and_dragged() -> Vec<RawAction> {
        let mut falling = literal_action("Falling", "Embedded", None, vec![pose("/falling.png", 0, 0, 250)]);
        falling.class = Some("com.group_finity.mascot.action.Fall".to_string());
        falling.params.insert("Gravity".to_string(), 2.0);

        let mut dragged_pose_action = literal_action("Pinched", "Embedded", None, vec![pose("/pinched.png", 0, 0, 5)]);
        dragged_pose_action.class = Some("com.group_finity.mascot.action.Dragged".to_string());

        vec![
            falling,
            dragged_pose_action,
            sequence("Fall", vec![("Falling", None)]),
            sequence("Dragged", vec![("Pinched", None)]),
        ]
    }

    #[test]
    fn maps_a_literal_move_action_with_loop_mode_and_frames() {
        let actions = {
            let mut a = fall_and_dragged();
            a.push(literal_action("Walk", "Move", Some("Floor"), vec![pose("/walk1.png", -2, 0, 6), pose("/walk2.png", -2, 0, 6)]));
            a
        };
        let mapped = map_actions(&actions, Path::new("/img")).unwrap();
        let walk = mapped.animations.iter().find(|a| a.key == "Walk").unwrap();
        assert_eq!(walk.kind, SurfaceType::Ground);
        assert_eq!(walk.loop_mode, LoopMode::Loop);
        assert_eq!(walk.frames.len(), 2);
        assert_eq!(walk.frames[0].dx, -2);
        assert_eq!(walk.frames[0].duration_ticks, 6);
    }

    #[test]
    fn animate_kind_is_oneshot() {
        let actions = {
            let mut a = fall_and_dragged();
            a.push(literal_action("Bouncing", "Animate", Some("Floor"), vec![pose("/b1.png", 0, 0, 4), pose("/b2.png", 0, 0, 4)]));
            a
        };
        let mapped = map_actions(&actions, Path::new("/img")).unwrap();
        let bouncing = mapped.animations.iter().find(|a| a.key == "Bouncing").unwrap();
        assert_eq!(bouncing.loop_mode, LoopMode::Oneshot);
    }

    #[test]
    fn collapses_multiple_conditioned_blocks_to_the_last_one_and_records_it_as_skipped() {
        let mut pinched = literal_action("Pinched2", "Embedded", None, vec![]);
        pinched.class = Some("com.group_finity.mascot.action.Dragged".to_string());
        pinched.animations = vec![
            RawAnimationBlock { condition: Some("a".to_string()), poses: vec![pose("/left.png", 0, 0, 5)] },
            RawAnimationBlock { condition: Some("b".to_string()), poses: vec![pose("/right.png", 0, 0, 5)] },
        ];
        let actions = {
            let mut a = fall_and_dragged();
            a.push(pinched);
            a
        };
        let mapped = map_actions(&actions, Path::new("/img")).unwrap();
        let mapped_action = mapped.animations.iter().find(|a| a.key == "Pinched2").unwrap();
        assert_eq!(mapped_action.frames.len(), 1);
        assert_eq!(mapped.sprites.iter().find(|s| s.index == mapped_action.frames[0].sprite).unwrap().source, Path::new("/img/right.png"));
        assert!(mapped.skipped.contains(&"Pinched2".to_string()));
    }

    #[test]
    fn bakes_fall_physics_into_a_multi_frame_oneshot_animation() {
        let mapped = map_actions(&fall_and_dragged(), Path::new("/img")).unwrap();
        let falling = mapped.animations.iter().find(|a| a.key == "Falling").unwrap();
        assert_eq!(falling.kind, SurfaceType::Air);
        assert_eq!(falling.loop_mode, LoopMode::Oneshot);
        assert!(falling.frames.len() > 1, "baked fall should produce many frames, got {}", falling.frames.len());
        assert!(falling.frames[1].dy > falling.frames[0].dy, "gravity should keep accelerating dy");
    }

    #[test]
    fn degrades_unsupported_embedded_classes_to_a_single_static_frame() {
        let mut walk_with_ie = literal_action("WalkWithIe", "Embedded", Some("Floor"), vec![pose("/ie.png", -2, 0, 6)]);
        walk_with_ie.class = Some("com.group_finity.mascot.action.WalkWithIE".to_string());
        let actions = {
            let mut a = fall_and_dragged();
            a.push(walk_with_ie);
            a
        };
        let mapped = map_actions(&actions, Path::new("/img")).unwrap();
        let mapped_action = mapped.animations.iter().find(|a| a.key == "WalkWithIe").unwrap();
        assert_eq!(mapped_action.frames.len(), 1);
        assert_eq!(mapped_action.frames[0].dx, 0);
        assert_eq!(mapped_action.frames[0].dy, 0);
        assert!(mapped.skipped.contains(&"WalkWithIe".to_string()));
    }

    #[test]
    fn zero_animation_embedded_actions_are_unmapped() {
        let mut offset = literal_action("Offset", "Embedded", None, vec![]);
        offset.animations = Vec::new();
        offset.class = Some("com.group_finity.mascot.action.Offset".to_string());
        let actions = {
            let mut a = fall_and_dragged();
            a.push(offset);
            a
        };
        let mapped = map_actions(&actions, Path::new("/img")).unwrap();
        assert!(mapped.animations.iter().all(|a| a.key != "Offset"));
        assert!(!mapped.leaf.contains_key("Offset"));
    }

    #[test]
    fn sequence_chains_oneshot_steps_via_on_finish() {
        let actions = {
            let mut a = fall_and_dragged();
            a.push(literal_action("Bouncing", "Animate", Some("Floor"), vec![pose("/b.png", 0, 0, 4)]));
            a.push(sequence("Land", vec![("Falling", None), ("Bouncing", None)]));
            a
        };
        let mapped = map_actions(&actions, Path::new("/img")).unwrap();
        let falling = mapped.animations.iter().find(|a| a.key == "Falling").unwrap();
        let auto = falling.auto.as_ref().expect("Falling should have gained an auto rule from the chain");
        assert_eq!(auto.on_finish.len(), 1);
        assert_eq!(auto.on_finish[0].to, "Bouncing");
        assert_eq!(mapped.sequences.get("Land"), Some(&("Falling".to_string(), "Bouncing".to_string())));
    }

    #[test]
    fn sequence_chains_loop_steps_via_on_timer_using_literal_duration() {
        let actions = {
            let mut a = fall_and_dragged();
            a.push(literal_action("Walk", "Move", Some("Floor"), vec![pose("/w.png", -2, 0, 6)]));
            a.push(sequence("Stroll", vec![("Walk", Some("42")), ("Falling", None)]));
            a
        };
        let mapped = map_actions(&actions, Path::new("/img")).unwrap();
        let walk = mapped.animations.iter().find(|a| a.key == "Walk").unwrap();
        let auto = walk.auto.as_ref().unwrap();
        assert_eq!(auto.on_timer.len(), 1);
        assert_eq!(auto.on_timer[0].min_ticks, 42);
        assert_eq!(auto.on_timer[0].max_ticks, 42);
        assert_eq!(auto.on_timer[0].choices[0].to, "Falling");
    }

    #[test]
    fn sequence_chains_loop_steps_via_on_timer_using_fallback_when_duration_is_an_el_expression() {
        let actions = {
            let mut a = fall_and_dragged();
            a.push(literal_action("Walk", "Move", Some("Floor"), vec![pose("/w.png", -2, 0, 6)]));
            a.push(sequence("Stroll", vec![("Walk", Some("${Math.random()*100}")), ("Falling", None)]));
            a
        };
        let mapped = map_actions(&actions, Path::new("/img")).unwrap();
        let walk = mapped.animations.iter().find(|a| a.key == "Walk").unwrap();
        assert_eq!(walk.auto.as_ref().unwrap().on_timer[0].min_ticks, SEQUENCE_STEP_FALLBACK_TICKS);
    }

    #[test]
    fn drops_a_whole_sequence_that_references_an_unmapped_action() {
        let mut offset = literal_action("Offset", "Embedded", None, vec![]);
        offset.animations = Vec::new();
        let actions = {
            let mut a = fall_and_dragged();
            a.push(offset);
            a.push(literal_action("Stand", "Stay", Some("Floor"), vec![pose("/s.png", 0, 0, 250)]));
            a.push(sequence("FallFromWall", vec![("Offset", None), ("Stand", None)]));
            a
        };
        let mapped = map_actions(&actions, Path::new("/img")).unwrap();
        assert!(!mapped.sequences.contains_key("FallFromWall"));
        assert!(mapped.skipped.contains(&"FallFromWall".to_string()));
    }

    #[test]
    fn wires_fall_and_dragged_into_top_level_events() {
        let mapped = map_actions(&fall_and_dragged(), Path::new("/img")).unwrap();
        assert_eq!(mapped.default_animation, "Falling");
        assert!(mapped.events.iter().any(|e| e.event == EngineEventKind::DragEnd && e.to.as_deref() == Some("Falling")));
        assert!(mapped.events.iter().any(|e| e.event == EngineEventKind::FlingEnd && e.to.as_deref() == Some("Falling")));
        assert!(mapped.events.iter().any(|e| e.event == EngineEventKind::DragStart && e.to.as_deref() == Some("Pinched")));
    }

    #[test]
    fn missing_fall_action_is_an_error() {
        let mut dragged_only = fall_and_dragged();
        dragged_only.retain(|a| a.name != "Fall" && a.name != "Falling");
        let err = map_actions(&dragged_only, Path::new("/img")).unwrap_err();
        assert!(matches!(err, ShimejiEeError::MissingRequiredAction(name) if name == "Fall"));
    }

    #[test]
    fn sprite_paths_are_resolved_under_the_img_dir_and_deduplicated() {
        let mapped = map_actions(&fall_and_dragged(), Path::new("/mascots/img/Test")).unwrap();
        assert!(mapped.sprites.iter().any(|s| s.source == Path::new("/mascots/img/Test/falling.png")));
        let indices: std::collections::HashSet<u32> = mapped.sprites.iter().map(|s| s.index).collect();
        assert_eq!(indices.len(), mapped.sprites.len(), "every sprite index must be unique");
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib -p shimeji shimeji_ee::mapping`
Expected: FAIL — `not implemented` panic from `map_actions`.

- [ ] **Step 4: Implement `map_actions`**

Replace the `unimplemented!()` body and add the private helper functions below it, in `src/importer/shimeji_ee/mapping.rs` (insert after the `map_actions` signature, before `#[cfg(test)]`):

```rust
pub(super) fn map_actions(actions: &[RawAction], img_dir: &Path) -> Result<MappedActions, ShimejiEeError> {
    let mut skipped = Vec::new();
    let mut sprite_index: HashMap<String, u32> = HashMap::new();
    let mut sprites = Vec::new();
    let mut animations = Vec::new();
    let mut leaf: HashMap<String, String> = HashMap::new();

    for action in actions.iter().filter(|a| a.kind != "Sequence") {
        if let Some(mapped) = map_leaf_action(action, img_dir, &mut sprite_index, &mut sprites, &mut skipped) {
            leaf.insert(action.name.clone(), mapped.key.clone());
            animations.push(mapped);
        }
    }

    let mut sequences: HashMap<String, (String, String)> = HashMap::new();
    for action in actions.iter().filter(|a| a.kind == "Sequence") {
        match resolve_sequence(action, &leaf, &sequences, &mut animations) {
            Some(anchors) => {
                sequences.insert(action.name.clone(), anchors);
            }
            None => skipped.push(action.name.clone()),
        }
    }

    let default_animation = resolve_entry("Fall", &leaf, &sequences)
        .ok_or_else(|| ShimejiEeError::MissingRequiredAction("Fall".to_string()))?;
    let dragged_entry = resolve_entry("Dragged", &leaf, &sequences)
        .ok_or_else(|| ShimejiEeError::MissingRequiredAction("Dragged".to_string()))?;

    let events = vec![
        wildcard_event(EngineEventKind::DragEnd, &default_animation),
        wildcard_event(EngineEventKind::FlingEnd, &default_animation),
        wildcard_event(EngineEventKind::DragStart, &dragged_entry),
    ];

    Ok(MappedActions { animations, events, default_animation, sprites, skipped, leaf, sequences })
}

fn wildcard_event(event: EngineEventKind, to: &str) -> EventRule {
    EventRule {
        event,
        from: Some("*".to_string()),
        when: None,
        facing: None,
        to: Some(to.to_string()),
        set_facing: None,
        choices: None,
        min_level: None,
        max_level: None,
        allowed_types: None,
    }
}

fn surface_type(border_type: Option<&str>) -> SurfaceType {
    match border_type {
        Some("Floor") => SurfaceType::Ground,
        Some("Wall") => SurfaceType::Wall,
        Some("Ceiling") => SurfaceType::Ceiling,
        _ => SurfaceType::Air,
    }
}

fn intern_sprite(image: &str, img_dir: &Path, sprite_index: &mut HashMap<String, u32>, sprites: &mut Vec<SpriteFile>) -> u32 {
    if let Some(&index) = sprite_index.get(image) {
        return index;
    }
    let index = sprites.len() as u32;
    sprites.push(SpriteFile { source: img_dir.join(image.trim_start_matches('/')), index });
    sprite_index.insert(image.to_string(), index);
    index
}

fn literal_frame(pose: &RawPose, img_dir: &Path, sprite_index: &mut HashMap<String, u32>, sprites: &mut Vec<SpriteFile>) -> Frame {
    Frame {
        sprite: intern_sprite(&pose.image, img_dir, sprite_index, sprites),
        dx: pose.velocity.0,
        dy: pose.velocity.1,
        duration_ticks: pose.duration.max(1),
    }
}

fn first_sprite(block: &RawAnimationBlock, img_dir: &Path, sprite_index: &mut HashMap<String, u32>, sprites: &mut Vec<SpriteFile>) -> u32 {
    block.poses.first().map(|p| intern_sprite(&p.image, img_dir, sprite_index, sprites)).unwrap_or(0)
}

const FALL_CLASS: &str = "com.group_finity.mascot.action.Fall";
const JUMP_CLASS: &str = "com.group_finity.mascot.action.Jump";
const DRAGGED_CLASS: &str = "com.group_finity.mascot.action.Dragged";

fn map_leaf_action(
    action: &RawAction,
    img_dir: &Path,
    sprite_index: &mut HashMap<String, u32>,
    sprites: &mut Vec<SpriteFile>,
    skipped: &mut Vec<String>,
) -> Option<Animation> {
    if action.animations.len() > 1 {
        skipped.push(action.name.clone());
    }
    let block = action.animations.last()?;

    let frames = match action.class.as_deref() {
        Some(FALL_CLASS) => {
            let sprite = first_sprite(block, img_dir, sprite_index, sprites);
            let gravity = action.params.get("Gravity").copied().unwrap_or(1.0);
            let resistance_x = action.params.get("RegistanceX").copied().unwrap_or(0.0);
            let resistance_y = action.params.get("RegistanceY").copied().unwrap_or(0.0);
            bake_fall(gravity, resistance_x, resistance_y, sprite, PHYSICS_TICKS)
        }
        Some(JUMP_CLASS) => {
            let sprite = first_sprite(block, img_dir, sprite_index, sprites);
            let velocity = action.params.get("VelocityParam").copied().unwrap_or(0.0);
            bake_jump(velocity, sprite, PHYSICS_TICKS)
        }
        Some(DRAGGED_CLASS) => {
            block.poses.iter().map(|p| literal_frame(p, img_dir, sprite_index, sprites)).collect()
        }
        Some(_other) => {
            skipped.push(action.name.clone());
            let sprite = first_sprite(block, img_dir, sprite_index, sprites);
            vec![Frame { sprite, dx: 0, dy: 0, duration_ticks: 1 }]
        }
        None => block.poses.iter().map(|p| literal_frame(p, img_dir, sprite_index, sprites)).collect(),
    };

    let loop_mode = if action.kind == "Animate" || matches!(action.class.as_deref(), Some(FALL_CLASS) | Some(JUMP_CLASS)) {
        LoopMode::Oneshot
    } else {
        LoopMode::Loop
    };

    Some(Animation {
        key: action.name.clone(),
        kind: surface_type(action.border_type.as_deref()),
        subtype: action.kind.clone(),
        level: 1,
        loop_mode,
        direction: Direction::Any,
        frames,
        auto: None,
        border_transitions: Vec::new(),
        event_transitions: Vec::new(),
    })
}

fn resolve_entry(name: &str, leaf: &HashMap<String, String>, sequences: &HashMap<String, (String, String)>) -> Option<String> {
    leaf.get(name).cloned().or_else(|| sequences.get(name).map(|(entry, _)| entry.clone()))
}

fn resolve_exit(name: &str, leaf: &HashMap<String, String>, sequences: &HashMap<String, (String, String)>) -> Option<String> {
    leaf.get(name).cloned().or_else(|| sequences.get(name).map(|(_, exit)| exit.clone()))
}

fn resolve_sequence(
    action: &RawAction,
    leaf: &HashMap<String, String>,
    sequences: &HashMap<String, (String, String)>,
    animations: &mut [Animation],
) -> Option<(String, String)> {
    let mut resolved = Vec::with_capacity(action.refs.len());
    for r in &action.refs {
        let entry = resolve_entry(&r.name, leaf, sequences)?;
        let exit = resolve_exit(&r.name, leaf, sequences)?;
        resolved.push((entry, exit));
    }
    if resolved.is_empty() {
        return None;
    }

    for i in 0..resolved.len().saturating_sub(1) {
        let (_, from_exit) = resolved[i].clone();
        let (to_entry, _) = resolved[i + 1].clone();
        let duration_ticks = action.refs[i].duration.as_deref().and_then(|d| d.parse::<u32>().ok());
        link(animations, &from_exit, &to_entry, duration_ticks);
    }

    let entry = resolved.first().unwrap().0.clone();
    let exit = resolved.last().unwrap().1.clone();
    Some((entry, exit))
}

fn link(animations: &mut [Animation], from_key: &str, to_key: &str, duration_ticks: Option<u32>) {
    let Some(anim) = animations.iter_mut().find(|a| a.key == from_key) else { return };
    let choice = ChoiceItem { to: to_key.to_string(), weight: 1.0, set_facing: None, min_level: None };
    let auto = anim.auto.get_or_insert_with(AutoBehavior::default);

    if matches!(anim.loop_mode, LoopMode::Oneshot) {
        auto.on_finish.push(choice);
    } else {
        let ticks = duration_ticks.unwrap_or(SEQUENCE_STEP_FALLBACK_TICKS);
        auto.on_timer.push(TimerRule { choices: vec![choice], min_ticks: ticks, max_ticks: ticks, chance: 1.0 });
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib -p shimeji shimeji_ee::mapping`
Expected: all tests PASS.

- [ ] **Step 6: Run the full suite**

Run: `cargo test`
Expected: all existing tests still pass.

- [ ] **Step 7: Commit**

```bash
git add src/importer/shimeji_ee/mod.rs src/importer/shimeji_ee/mapping.rs
git commit -m "feat: map shimeji-ee actions into shimeji_rs Animations/events

Literal Stay/Move/Animate actions become Animations directly; Fall/Jump
embedded physics classes get baked multi-frame arcs (Task 2); other
embedded classes degrade to a single static frame; Sequence actions
chain their steps via auto.on_finish (Oneshot steps) or auto.on_timer
(Loop steps, since only on_timer can target an arbitrary next
animation regardless of loop mode -- on_finish never fires for Loop
animations, and max_duration_ticks can only ever return to the
schema's single global default_animation). Fall and Dragged are
required: importing a mascot missing either now fails fast instead of
producing a bundle that freezes on release, the same bug class already
fixed for pc_import_v1."
```

---

### Task 5: Behavior mapping and `map_to_schema` (`src/importer/shimeji_ee/mapping.rs`, part B)

**Files:**
- Modify: `src/importer/shimeji_ee/mapping.rs`

**Interfaces:**
- Consumes: `MappedActions` (Task 4, same file), `xml::RawBehavior` (Task 3).
- Produces: `pub fn map_to_schema(actions: &[RawAction], behaviors: &[RawBehavior], img_dir: &Path) -> Result<(AnimationSchema, Vec<SpriteFile>, Vec<String>), ShimejiEeError>` — this is the function Task 7 calls.

- [ ] **Step 1: Write the failing tests**

Add to `src/importer/shimeji_ee/mapping.rs`'s `#[cfg(test)] mod tests` block (add the import at the top of the block: `use super::xml::RawBehavior;` alongside the existing `use super::*;`):

```rust
    fn behavior(name: &str, next: Vec<(&str, u32, Option<&str>)>) -> RawBehavior {
        RawBehavior {
            name: name.to_string(),
            frequency: 0,
            hidden: false,
            condition: None,
            next: next
                .into_iter()
                .map(|(n, freq, cond)| super::super::xml::RawBehaviorRef {
                    name: n.to_string(),
                    frequency: freq,
                    condition: cond.map(str::to_string),
                })
                .collect(),
        }
    }

    #[test]
    fn map_to_schema_attaches_behaviors_choices_to_the_exit_anchor() {
        let mut actions = fall_and_dragged();
        actions.push(literal_action("Walk", "Move", Some("Floor"), vec![pose("/w.png", -2, 0, 6)]));
        actions.push(literal_action("Stand", "Stay", Some("Floor"), vec![pose("/s.png", 0, 0, 250)]));
        let behaviors = vec![behavior("Stand", vec![("Walk", 100, None), ("Unknown", 1, None), ("Walk", 50, Some("${cond}"))])];

        let (schema, _sprites, skipped) = map_to_schema(&actions, &behaviors, Path::new("/img")).unwrap();

        let stand = schema.animations.iter().find(|a| a.key == "Stand").unwrap();
        let auto = stand.auto.as_ref().expect("Stand should have gained an on_timer rule from behaviors.xml");
        assert_eq!(auto.on_timer.len(), 1);
        assert_eq!(auto.on_timer[0].choices.len(), 1, "the unresolvable ref and the condition-gated ref must be dropped");
        assert_eq!(auto.on_timer[0].choices[0].to, "Walk");
        assert_eq!(auto.on_timer[0].choices[0].weight, 100.0);
        assert!(skipped.contains(&"Unknown".to_string()));
        assert!(skipped.iter().filter(|s| s.as_str() == "Walk").count() >= 1, "the condition-gated Walk ref must be recorded as skipped too");
    }

    #[test]
    fn map_to_schema_skips_a_behavior_whose_name_does_not_resolve() {
        let actions = fall_and_dragged();
        let behaviors = vec![behavior("GhostBehavior", vec![("Falling", 1, None)])];
        let (_schema, _sprites, skipped) = map_to_schema(&actions, &behaviors, Path::new("/img")).unwrap();
        assert!(skipped.contains(&"GhostBehavior".to_string()));
    }

    #[test]
    fn map_to_schema_appends_behaviors_choices_onto_an_existing_on_finish_from_chaining() {
        let mut actions = fall_and_dragged();
        actions.push(literal_action("Bouncing", "Animate", Some("Floor"), vec![pose("/b.png", 0, 0, 4)]));
        actions.push(sequence("Land", vec![("Falling", None), ("Bouncing", None)]));
        let behaviors = vec![behavior("Falling", vec![("Bouncing", 5, None)])];

        let (schema, _sprites, _skipped) = map_to_schema(&actions, &behaviors, Path::new("/img")).unwrap();
        let falling = schema.animations.iter().find(|a| a.key == "Falling").unwrap();
        let on_finish = &falling.auto.as_ref().unwrap().on_finish;
        assert_eq!(on_finish.len(), 2, "the chain's own link plus the behaviors.xml choice");
    }

    #[test]
    fn map_to_schema_produces_a_shimeji_ee_import_schema_id() {
        let (schema, _sprites, _skipped) = map_to_schema(&fall_and_dragged(), &[], Path::new("/img")).unwrap();
        assert_eq!(schema.schema_id, "shimeji_ee_import_v1");
        assert_eq!(schema.default_animation, "Falling");
        assert_eq!(schema.initial_candidates, vec!["Falling".to_string()]);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib -p shimeji shimeji_ee::mapping`
Expected: FAIL — `map_to_schema` doesn't exist yet (compile error).

- [ ] **Step 3: Implement `apply_behaviors` and `map_to_schema`**

Add to `src/importer/shimeji_ee/mapping.rs`, above `#[cfg(test)]`:

```rust
const BEHAVIOR_FALLBACK_TICKS: u32 = 300;

pub fn map_to_schema(
    actions: &[RawAction],
    behaviors: &[xml::RawBehavior],
    img_dir: &Path,
) -> Result<(crate::format::animation::AnimationSchema, Vec<SpriteFile>, Vec<String>), ShimejiEeError> {
    let MappedActions { mut animations, events, default_animation, sprites, mut skipped, leaf, sequences } =
        map_actions(actions, img_dir)?;

    apply_behaviors(behaviors, &mut animations, &leaf, &sequences, &mut skipped);

    let schema = crate::format::animation::AnimationSchema {
        schema_id: "shimeji_ee_import_v1".to_string(),
        version: 1,
        default_animation: default_animation.clone(),
        initial_candidates: vec![default_animation],
        animations,
        events,
    };

    Ok((schema, sprites, skipped))
}

fn apply_behaviors(
    behaviors: &[xml::RawBehavior],
    animations: &mut [Animation],
    leaf: &HashMap<String, String>,
    sequences: &HashMap<String, (String, String)>,
    skipped: &mut Vec<String>,
) {
    for behavior in behaviors {
        if behavior.next.is_empty() {
            continue;
        }
        let Some(exit) = resolve_exit(&behavior.name, leaf, sequences) else {
            skipped.push(behavior.name.clone());
            continue;
        };

        let mut choices = Vec::new();
        for r in &behavior.next {
            if r.condition.is_some() {
                skipped.push(r.name.clone());
                continue;
            }
            match resolve_entry(&r.name, leaf, sequences) {
                Some(entry) => choices.push(ChoiceItem { to: entry, weight: r.frequency as f64, set_facing: None, min_level: None }),
                None => skipped.push(r.name.clone()),
            }
        }
        if choices.is_empty() {
            continue;
        }

        let Some(anim) = animations.iter_mut().find(|a| a.key == exit) else { continue };
        let auto = anim.auto.get_or_insert_with(AutoBehavior::default);

        // on_finish/on_timer here follow the same loop_mode-driven rule Sequence-step
        // chaining (`link`, above) uses, so any rule chaining already attached always uses
        // the same mechanism we're about to use: extend the existing on_finish list, or add
        // our own dedicated on_timer rule alongside whatever chaining already added.
        if matches!(anim.loop_mode, LoopMode::Oneshot) {
            auto.on_finish.extend(choices);
        } else {
            auto.on_timer.push(TimerRule { choices, min_ticks: BEHAVIOR_FALLBACK_TICKS, max_ticks: BEHAVIOR_FALLBACK_TICKS, chance: 1.0 });
        }
    }
}
```

Add `use super::xml;` near the top of the file (alongside the existing `use super::xml::{RawAction, RawAnimationBlock, RawPose};`) so `xml::RawBehavior` resolves in the new function signatures.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib -p shimeji shimeji_ee::mapping`
Expected: all tests PASS (Task 4's tests and Task 5's new ones).

- [ ] **Step 5: Run the full suite**

Run: `cargo test`
Expected: all existing tests still pass.

- [ ] **Step 6: Commit**

```bash
git add src/importer/shimeji_ee/mapping.rs
git commit -m "feat: translate behaviors.xml into weighted auto choices

Each Behavior's NextBehavior weights become ChoiceItems on its exit
anchor (the last surviving step of the Action/Sequence it names),
attached via on_finish or on_timer following the same loop_mode rule
Sequence-step chaining uses. A ref gated by any Condition attribute is
dropped (best-effort translation; no EL evaluator exists to check it),
as is a Behavior whose own name doesn't resolve to a mapped animation."
```

---

### Task 6: Detection (`src/importer/shimeji_ee/detect.rs`)

**Files:**
- Create: `src/importer/shimeji_ee/detect.rs`
- Modify: `src/importer/shimeji_ee/mod.rs` (add `pub mod detect;`, add `NoSprites` error variant)

**Interfaces:**
- Consumes: nothing new (plain `std::fs`/`std::path`).
- Produces: `pub struct DetectedMascot { pub actions_xml: PathBuf, pub behaviors_xml: Option<PathBuf>, pub img_dir: PathBuf }`, `pub fn detect(root: &Path) -> Result<Option<DetectedMascot>, ShimejiEeError>` — Task 7 calls this.

- [ ] **Step 1: Add the `NoSprites` error variant and declare the module**

In `src/importer/shimeji_ee/mod.rs`, add `pub mod detect;` below `pub mod xml;`, and add a variant to `ShimejiEeError`:

```rust
    #[error("could not locate a usable img/<character> sprite folder for this actions.xml")]
    NoSprites,
```

- [ ] **Step 2: Write the failing tests**

Create `src/importer/shimeji_ee/detect.rs`:

```rust
use super::ShimejiEeError;
use std::path::{Path, PathBuf};

pub struct DetectedMascot {
    pub actions_xml: PathBuf,
    pub behaviors_xml: Option<PathBuf>,
    pub img_dir: PathBuf,
}

pub fn detect(root: &Path) -> Result<Option<DetectedMascot>, ShimejiEeError> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }

    #[test]
    fn returns_none_when_no_actions_xml_exists() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("manifest.json"), b"{}").unwrap();
        assert!(detect(tmp.path()).unwrap().is_none());
    }

    #[test]
    fn detects_a_bare_shared_conf_character_folder() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("conf/actions.xml"));
        touch(&tmp.path().join("conf/behaviors.xml"));
        touch(&tmp.path().join("img/TestMascot/stand.png"));

        let detected = detect(tmp.path()).unwrap().unwrap();
        assert_eq!(detected.actions_xml, tmp.path().join("conf/actions.xml"));
        assert_eq!(detected.behaviors_xml, Some(tmp.path().join("conf/behaviors.xml")));
        assert_eq!(detected.img_dir, tmp.path().join("img/TestMascot"));
    }

    #[test]
    fn detects_a_per_character_conf_override() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("conf/Miku/actions.xml"));
        touch(&tmp.path().join("conf/Miku/behaviors.xml"));
        touch(&tmp.path().join("img/Miku/stand.png"));

        let detected = detect(tmp.path()).unwrap().unwrap();
        assert_eq!(detected.img_dir, tmp.path().join("img/Miku"));
        assert_eq!(detected.behaviors_xml, Some(tmp.path().join("conf/Miku/behaviors.xml")));
    }

    #[test]
    fn detects_a_full_app_distribution_nested_under_a_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("shimejiee/conf/actions.xml"));
        touch(&tmp.path().join("shimejiee/conf/behaviors.xml"));
        touch(&tmp.path().join("shimejiee/img/Shimeji/shime1.png"));
        touch(&tmp.path().join("shimejiee/Shimeji-ee.jar"));

        let detected = detect(tmp.path()).unwrap().unwrap();
        assert_eq!(detected.img_dir, tmp.path().join("shimejiee/img/Shimeji"));
    }

    #[test]
    fn missing_behaviors_xml_is_optional_not_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("conf/actions.xml"));
        touch(&tmp.path().join("img/TestMascot/stand.png"));

        let detected = detect(tmp.path()).unwrap().unwrap();
        assert!(detected.behaviors_xml.is_none());
    }

    #[test]
    fn errors_when_no_img_folder_can_be_resolved() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("conf/actions.xml"));

        let err = detect(tmp.path()).unwrap_err();
        assert!(matches!(err, ShimejiEeError::NoSprites));
    }

    #[test]
    fn errors_when_the_resolved_character_folder_has_no_png_files() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("conf/actions.xml"));
        touch(&tmp.path().join("img/TestMascot/readme.txt"));

        let err = detect(tmp.path()).unwrap_err();
        assert!(matches!(err, ShimejiEeError::NoSprites));
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib -p shimeji shimeji_ee::detect`
Expected: FAIL — `not implemented` panic.

- [ ] **Step 4: Implement `detect`**

Replace the `unimplemented!()` body in `src/importer/shimeji_ee/detect.rs` and add the private helpers below it:

```rust
const MAX_DEPTH: usize = 6;

pub fn detect(root: &Path) -> Result<Option<DetectedMascot>, ShimejiEeError> {
    let Some(actions_xml) = find_actions_xml(root, 0) else { return Ok(None) };
    let conf_dir = actions_xml.parent().expect("actions.xml always has a parent directory").to_path_buf();

    let app_root = find_app_root(&conf_dir).ok_or(ShimejiEeError::NoSprites)?;
    let img_root = app_root.join("img");

    let img_dir = if conf_dir == app_root.join("conf") {
        let character = first_subdirectory(&img_root).ok_or(ShimejiEeError::NoSprites)?;
        img_root.join(character)
    } else {
        let character = conf_dir.file_name().and_then(|n| n.to_str()).ok_or(ShimejiEeError::NoSprites)?;
        img_root.join(character)
    };

    if !has_png(&img_dir) {
        return Err(ShimejiEeError::NoSprites);
    }

    let behaviors_xml = conf_dir.join("behaviors.xml");
    let behaviors_xml = behaviors_xml.is_file().then_some(behaviors_xml);

    Ok(Some(DetectedMascot { actions_xml, behaviors_xml, img_dir }))
}

fn find_actions_xml(dir: &Path, depth: usize) -> Option<PathBuf> {
    if depth > MAX_DEPTH {
        return None;
    }
    let entries: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().collect();
    for entry in &entries {
        let path = entry.path();
        if path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some("actions.xml") {
            return Some(path);
        }
    }
    for entry in &entries {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_actions_xml(&path, depth + 1) {
                return Some(found);
            }
        }
    }
    None
}

fn find_app_root(conf_dir: &Path) -> Option<PathBuf> {
    let mut candidate = conf_dir.to_path_buf();
    loop {
        if candidate.join("img").is_dir() {
            return Some(candidate);
        }
        candidate = candidate.parent()?.to_path_buf();
    }
}

fn first_subdirectory(dir: &Path) -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names.into_iter().next()
}

fn has_png(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .any(|e| e.path().extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("png")))
        })
        .unwrap_or(false)
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib -p shimeji shimeji_ee::detect`
Expected: all 7 tests PASS.

- [ ] **Step 6: Run the full suite**

Run: `cargo test`
Expected: all existing tests still pass.

- [ ] **Step 7: Commit**

```bash
git add src/importer/shimeji_ee/mod.rs src/importer/shimeji_ee/detect.rs
git commit -m "feat: locate actions.xml/behaviors.xml/img inside a shimeji-ee zip

Handles both the bare community-shared character-folder shape and a
full Shimeji-ee Java app distribution (actions.xml nested arbitrarily
deep, e.g. under a shimejiee/ prefix), plus a per-character conf
override. behaviors.xml is optional; a resolvable but sprite-less
character folder is an error, not a silent 'not this format'."
```

---

### Task 7: Orchestration (`src/importer/shimeji_ee/mod.rs`: `try_import`/`write_into`) and the test fixture

**Files:**
- Modify: `src/importer/shimeji_ee/mod.rs`
- Create: `tests/fixtures/fixture_shimeji_ee/conf/actions.xml`
- Create: `tests/fixtures/fixture_shimeji_ee/conf/behaviors.xml`

**Interfaces:**
- Consumes: `detect::detect` (Task 6), `xml::{parse_actions, parse_behaviors}` (Task 3), `mapping::map_to_schema` (Task 5), `crate::format::manifest::*` and `crate::format::animation::AnimationSchema` (Task 1, now `Serialize`), `crate::format::bundle::MascotBundle` (existing, unmodified).
- Produces: `pub struct ShimejiEeImport { pub skipped: Vec<String>, /* manifest/animation/sprites stay private */ }` with `pub fn write_into(&self, dir: &Path) -> Result<(), ShimejiEeError>`, and `pub fn try_import(extracted_root: &Path) -> Result<Option<ShimejiEeImport>, ShimejiEeError>` — Task 8 calls both of these from `src/importer/mod.rs`.

- [ ] **Step 1: Add the `Io` error variant**

In `src/importer/shimeji_ee/mod.rs`, add a variant to `ShimejiEeError`:

```rust
    #[error("could not write synthesized bundle: {0}")]
    Io(#[from] std::io::Error),
```

- [ ] **Step 2: Create the fixture XML files**

Create `tests/fixtures/fixture_shimeji_ee/conf/actions.xml`:

```xml
<?xml version="1.0" encoding="UTF-8" ?>
<Mascot xmlns="http://www.group-finity.com/Mascot" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
    xsi:schemaLocation="http://www.group-finity.com/Mascot Mascot.xsd">

    <ActionList>
        <Action Name="Stand" Type="Stay" BorderType="Floor">
            <Animation>
                <Pose Image="/stand.png" ImageAnchor="64,128" Velocity="0,0" Duration="250" />
            </Animation>
        </Action>

        <Action Name="Walk" Type="Move" BorderType="Floor">
            <Animation>
                <Pose Image="/walk1.png" ImageAnchor="64,128" Velocity="-2,0" Duration="6" />
                <Pose Image="/walk2.png" ImageAnchor="64,128" Velocity="-2,0" Duration="6" />
            </Animation>
        </Action>

        <Action Name="Falling" Type="Embedded" Class="com.group_finity.mascot.action.Fall" RegistanceX="0.05" RegistanceY="0.1" Gravity="2">
            <Animation>
                <Pose Image="/falling.png" ImageAnchor="64,128" Velocity="0,0" Duration="250" />
            </Animation>
        </Action>

        <Action Name="Pinched" Type="Embedded" Class="com.group_finity.mascot.action.Dragged">
            <Animation Condition="#{FootX &lt; mascot.environment.cursor.x}">
                <Pose Image="/pinched_left.png" ImageAnchor="64,128" Velocity="0,0" Duration="5" />
            </Animation>
            <Animation Condition="#{FootX &gt;= mascot.environment.cursor.x}">
                <Pose Image="/pinched_right.png" ImageAnchor="64,128" Velocity="0,0" Duration="5" />
            </Animation>
        </Action>

        <Action Name="WalkWithIe" Type="Embedded" Class="com.group_finity.mascot.action.WalkWithIE" BorderType="Floor" IeOffsetX="0" IeOffsetY="-64">
            <Animation>
                <Pose Image="/ieaction.png" ImageAnchor="64,128" Velocity="-2,0" Duration="6" />
            </Animation>
        </Action>

        <Action Name="Offset" Type="Embedded" Class="com.group_finity.mascot.action.Offset" />
    </ActionList>

    <ActionList>
        <Action Name="Fall" Type="Sequence" Loop="false">
            <ActionReference Name="Falling"/>
        </Action>

        <Action Name="Dragged" Type="Sequence" Loop="true">
            <ActionReference Name="Pinched"/>
        </Action>

        <Action Name="FallFromWall" Type="Sequence" Loop="false">
            <ActionReference Name="Offset" X="${mascot.lookRight ? -1 : 1}" />
            <ActionReference Name="Stand" />
        </Action>
    </ActionList>
</Mascot>
```

Create `tests/fixtures/fixture_shimeji_ee/conf/behaviors.xml`:

```xml
<?xml version="1.0" encoding="UTF-8" ?>
<Mascot xmlns="http://www.group-finity.com/Mascot" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
    xsi:schemaLocation="http://www.group-finity.com/Mascot Mascot.xsd">
    <BehaviorList>
        <Behavior Name="Stand" Frequency="0">
            <NextBehavior Add="false">
                <BehaviorReference Name="Walk" Frequency="100" />
                <BehaviorReference Name="SitAndSpin" Frequency="1" />
                <BehaviorReference Name="Walk" Frequency="50" Condition="${mascot.environment.activeIE.exists}" />
            </NextBehavior>
        </Behavior>
    </BehaviorList>
</Mascot>
```

This fixture is hand-authored and made up (no real character's art, names, or animation data — same convention as `tests/fixtures/fixture_bundle/`, see commit `98c821c`) but exercises every mapping rule: a literal `Stay`/`Move` pair, baked `Fall` physics, a collapsed multi-condition `Pinched` action, a degraded `WalkWithIe`, a zero-`Animation` `Offset` action that can never be mapped, a `Sequence` (`FallFromWall`) that must be dropped entirely because it references that unmappable `Offset`, and a `behaviors.xml` with one valid ref, one unresolvable-name ref, and one condition-gated ref.

- [ ] **Step 3: Write the failing test**

Replace the contents of `src/importer/shimeji_ee/mod.rs` (keep the existing `pub mod` lines and `ShimejiEeError` enum with all four variants accumulated so far — `Xml`, `MissingRequiredAction`, `NoSprites`, `Io` — and add everything below):

```rust
pub mod detect;
pub mod mapping;
pub mod physics;
pub mod xml;

use crate::format::animation::AnimationSchema;
use crate::format::manifest::{AnimationSchemaRef, AuthorInfo, LicenseInfo, Manifest, PreviewInfo, SpriteSheetInfo};
use mapping::SpriteFile;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ShimejiEeError {
    #[error("invalid shimeji-ee XML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("required action '{0}' is missing or could not be translated")]
    MissingRequiredAction(String),
    #[error("could not locate a usable img/<character> sprite folder for this actions.xml")]
    NoSprites,
    #[error("could not write synthesized bundle: {0}")]
    Io(#[from] std::io::Error),
}

pub struct ShimejiEeImport {
    manifest: Manifest,
    animation: AnimationSchema,
    sprites: Vec<SpriteFile>,
    pub skipped: Vec<String>,
}

pub fn try_import(extracted_root: &Path) -> Result<Option<ShimejiEeImport>, ShimejiEeError> {
    unimplemented!()
}

impl ShimejiEeImport {
    pub fn write_into(&self, dir: &Path) -> Result<(), ShimejiEeError> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::animation::EngineEventKind;
    use crate::format::bundle::MascotBundle;
    use std::fs;

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

    fn write_placeholder_png(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbaImage::new(4, 4).save(path).unwrap();
    }

    fn build_fixture_source() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        copy_dir_recursive(Path::new("tests/fixtures/fixture_shimeji_ee/conf"), &tmp.path().join("conf"));
        for name in ["stand.png", "walk1.png", "walk2.png", "falling.png", "pinched_left.png", "pinched_right.png", "ieaction.png"] {
            write_placeholder_png(&tmp.path().join("img/TestMascot").join(name));
        }
        tmp
    }

    #[test]
    fn try_import_translates_and_loads_the_fixture_mascot() {
        let src = build_fixture_source();

        let import = try_import(src.path()).unwrap().expect("fixture must be detected as shimeji-ee");
        assert!(import.skipped.iter().any(|s| s == "Pinched"));
        assert!(import.skipped.iter().any(|s| s == "WalkWithIe"));
        assert!(import.skipped.iter().any(|s| s == "FallFromWall"));

        let dest = tempfile::tempdir().unwrap();
        import.write_into(dest.path()).unwrap();

        let bundle = MascotBundle::load(dest.path()).unwrap();
        assert_eq!(bundle.animation.default_animation, "Falling");
        assert!(bundle.animation.events.iter().any(|e| e.event == EngineEventKind::DragStart && e.to.as_deref() == Some("Pinched")));
        assert!(bundle.animation.events.iter().any(|e| e.event == EngineEventKind::DragEnd && e.to.as_deref() == Some("Falling")));
    }

    #[test]
    fn try_import_returns_none_for_a_non_shimeji_ee_directory() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("manifest.json"), b"{}").unwrap();
        assert!(try_import(tmp.path()).unwrap().is_none());
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test --lib -p shimeji shimeji_ee::tests`
Expected: FAIL — `not implemented` panics from `try_import`/`write_into`.

- [ ] **Step 5: Implement `try_import`/`write_into`**

Replace the two `unimplemented!()` bodies:

```rust
pub fn try_import(extracted_root: &Path) -> Result<Option<ShimejiEeImport>, ShimejiEeError> {
    let Some(detected) = detect::detect(extracted_root)? else { return Ok(None) };

    let actions = xml::parse_actions(&std::fs::read_to_string(&detected.actions_xml)?)?;
    let behaviors = match &detected.behaviors_xml {
        Some(path) => xml::parse_behaviors(&std::fs::read_to_string(path)?)?,
        None => Vec::new(),
    };

    let character_name = detected
        .img_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("mascot")
        .to_string();

    let (animation, sprites, skipped) = mapping::map_to_schema(&actions, &behaviors, &detected.img_dir)?;

    let manifest = Manifest {
        schema_version: 1,
        name: character_name.clone(),
        name_slug: slugify(&character_name),
        category: "imported".to_string(),
        category_slug: "imported".to_string(),
        description: String::new(),
        bundle_version: 1,
        min_app_version: 1,
        levels: 1,
        origin: "SHIMEJI_EE_IMPORT".to_string(),
        animation_schema: AnimationSchemaRef {
            path: "animation.json".to_string(),
            schema_id: animation.schema_id.clone(),
            version: animation.version,
        },
        sprites: SpriteSheetInfo {
            kind: "SEQUENCE".to_string(),
            base_path: "sprites/".to_string(),
            file_pattern: "%04d.png".to_string(),
            sprite_count: sprites.len() as u32,
            size: [128, 128],
        },
        preview: PreviewInfo { thumbnail: String::new() },
        author: AuthorInfo { name: "Unknown".to_string() },
        license: LicenseInfo { kind: "UNKNOWN".to_string(), text: String::new(), attribution: String::new() },
    };

    Ok(Some(ShimejiEeImport { manifest, animation, sprites, skipped }))
}

fn slugify(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect()
}

impl ShimejiEeImport {
    pub fn write_into(&self, dir: &Path) -> Result<(), ShimejiEeError> {
        std::fs::write(dir.join("manifest.json"), serde_json::to_string_pretty(&self.manifest).expect("Manifest always serializes"))?;
        std::fs::write(dir.join("animation.json"), serde_json::to_string_pretty(&self.animation).expect("AnimationSchema always serializes"))?;

        let sprites_dir = dir.join("sprites");
        std::fs::create_dir_all(&sprites_dir)?;
        for sprite in &self.sprites {
            std::fs::copy(&sprite.source, sprites_dir.join(format!("{:04}.png", sprite.index)))?;
        }

        Ok(())
    }
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test --lib -p shimeji shimeji_ee`
Expected: all tests across `physics`, `xml`, `mapping`, `detect`, and this module's own `tests` PASS.

- [ ] **Step 7: Run the full suite**

Run: `cargo test`
Expected: all existing tests still pass.

- [ ] **Step 8: Commit**

```bash
git add src/importer/shimeji_ee/mod.rs tests/fixtures/fixture_shimeji_ee
git commit -m "feat: orchestrate shimeji-ee XML translation into a loadable bundle

try_import ties detection, XML parsing, and mapping together and
synthesizes a Manifest/AnimationSchema; write_into serializes them to
manifest.json/animation.json and copies sprites into sprites/, so the
existing, unmodified MascotBundle::load can validate and load the
result exactly like a native JSON bundle. Adds a hand-authored
synthetic fixture (tests/fixtures/fixture_shimeji_ee/) exercising every
mapping rule, per the project's no-real-mascot-content-in-tests
convention."
```

---

### Task 8: Wire into `import_zip`, update `App`, fix existing tests, add zip-level integration tests

**Files:**
- Modify: `src/importer/mod.rs`
- Modify: `src/app.rs`
- Modify: `tests/importer.rs`

**Interfaces:**
- Consumes: `shimeji_ee::{try_import, ShimejiEeError}` (Task 7).
- Produces: `pub struct ImportResult { pub entry: CatalogEntry, pub skipped: Vec<String> }`, replacing `CatalogEntry` as `import_zip`'s success type. `ImportError` gains `ShimejiEe(#[from] shimeji_ee::ShimejiEeError)`.

- [ ] **Step 1: Update `src/importer/mod.rs`**

Replace its contents:

```rust
pub mod catalog;
pub mod shimeji_ee;

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
    #[error("could not translate shimeji-ee mascot: {0}")]
    ShimejiEe(#[from] shimeji_ee::ShimejiEeError),
}

pub struct ImportResult {
    pub entry: CatalogEntry,
    pub skipped: Vec<String>,
}

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

    let slug = bundle.manifest.name_slug.clone();
    let target_dir = library_root.join(&slug);
    if target_dir.exists() {
        return Err(ImportError::Catalog(CatalogError::DuplicateSlug { slug }));
    }

    std::fs::create_dir_all(library_root)?;
    copy_dir_recursive(scratch.path(), &target_dir)?;

    let entry = CatalogEntry { slug: slug.clone(), name: bundle.manifest.name.clone(), dir: target_dir };
    add_entry(library_root, entry.clone())?;
    Ok(ImportResult { entry, skipped })
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

(`copy_dir_recursive` and the crate imports are unchanged from before this task — only the `ImportError`/`ImportResult`/`import_zip` items above it changed.)

- [ ] **Step 2: Update `src/app.rs`'s `import_from_bytes`**

Find the existing method (currently `Ok(_entry) => { ... }`) and replace it:

```rust
    pub fn import_from_bytes(&mut self, zip_bytes: &[u8]) {
        match shimeji::importer::import_zip(zip_bytes, &self.library_root) {
            Ok(result) => {
                self.catalog = shimeji::importer::catalog::load_catalog(&self.library_root).unwrap_or_default();
                self.refresh_tray_menu();
                for name in &result.skipped {
                    crate::logging::log_error("import", &format!("shimeji-ee: could not translate '{name}', skipped"));
                }
            }
            Err(err) => crate::logging::log_error("import", &err.to_string()),
        }
    }
```

- [ ] **Step 3: Fix the 3 existing tests in `tests/importer.rs` that destructure `CatalogEntry` fields directly**

Replace the whole file's contents (all three tests need their `import_zip(...)`-result variable renamed from `entry` to `result` and field access updated from `entry.slug`/`entry.name`/`entry.dir` to `result.entry.slug`/`result.entry.name`/`result.entry.dir`):

```rust
use shimeji::importer::{import_zip, ImportError};
use shimeji::importer::catalog::load_catalog;
use std::fs;

#[test]
fn imports_the_sample_zip() {
    let tmp = tempfile::tempdir().unwrap();
    let library_root = tmp.path();
    let zip_bytes = fs::read("tests/fixtures/fixture_bundle.zip").unwrap();

    let result = import_zip(&zip_bytes, library_root).unwrap();
    assert_eq!(result.entry.slug, "sample_mascot");
    assert_eq!(result.entry.name, "sample_mascot");
    assert!(result.entry.dir.join("manifest.json").is_file());
    assert!(result.entry.dir.join("sprites/0000.webp").is_file());
    assert!(result.skipped.is_empty());

    let catalog = load_catalog(library_root).unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].slug, "sample_mascot");
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
    let zip_bytes = fs::read("tests/fixtures/fixture_bundle.zip").unwrap();

    import_zip(&zip_bytes, library_root).unwrap();
    let err = import_zip(&zip_bytes, library_root).unwrap_err();
    assert!(matches!(err, ImportError::Catalog(_)));
}
```

- [ ] **Step 4: Run the existing tests to verify they compile and pass again**

Run: `cargo test --test importer`
Expected: all 3 tests PASS.

- [ ] **Step 5: Write the failing zip-shape integration tests**

Append to `tests/importer.rs`:

```rust
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
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

fn write_placeholder_png(path: &std::path::Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbaImage::new(4, 4).save(path).unwrap();
}

fn build_shimeji_ee_source_tree(root: &std::path::Path) {
    copy_dir_recursive(std::path::Path::new("tests/fixtures/fixture_shimeji_ee/conf"), &root.join("conf"));
    for name in ["stand.png", "walk1.png", "walk2.png", "falling.png", "pinched_left.png", "pinched_right.png", "ieaction.png"] {
        write_placeholder_png(&root.join("img/TestMascot").join(name));
    }
}

fn zip_dir_with_prefix(src: &std::path::Path, prefix: &str) -> Vec<u8> {
    use std::io::Write;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    add_dir_to_zip(&mut writer, src, prefix, options);
    writer.finish().unwrap().into_inner()
}

fn add_dir_to_zip<W: std::io::Write + std::io::Seek>(
    writer: &mut zip::ZipWriter<W>,
    dir: &std::path::Path,
    prefix: &str,
    options: zip::write::SimpleFileOptions,
) {
    use std::io::Write;
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        if entry.file_type().unwrap().is_dir() {
            add_dir_to_zip(writer, &entry.path(), &format!("{name}/"), options);
        } else {
            writer.start_file(&name, options).unwrap();
            writer.write_all(&fs::read(entry.path()).unwrap()).unwrap();
        }
    }
}

#[test]
fn imports_a_bare_shimeji_ee_character_folder_zip() {
    let source = tempfile::tempdir().unwrap();
    build_shimeji_ee_source_tree(source.path());
    let zip_bytes = zip_dir_with_prefix(source.path(), "");

    let tmp = tempfile::tempdir().unwrap();
    let result = import_zip(&zip_bytes, tmp.path()).unwrap();

    assert_eq!(result.entry.name, "TestMascot");
    assert!(!result.skipped.is_empty(), "the fixture is designed to exercise several degraded/dropped paths");

    let bundle = shimeji::format::bundle::MascotBundle::load(&result.entry.dir).unwrap();
    assert_eq!(bundle.animation.default_animation, "Falling");
}

#[test]
fn imports_a_shimeji_ee_zip_nested_under_a_full_app_distribution_prefix() {
    let source = tempfile::tempdir().unwrap();
    build_shimeji_ee_source_tree(&source.path().join("shimejiee"));
    let zip_bytes = zip_dir_with_prefix(source.path(), "");

    let tmp = tempfile::tempdir().unwrap();
    let result = import_zip(&zip_bytes, tmp.path()).unwrap();

    let bundle = shimeji::format::bundle::MascotBundle::load(&result.entry.dir).unwrap();
    assert_eq!(bundle.animation.default_animation, "Falling");
}
```

Also add `use shimeji::importer::ImportResult;` is not needed (fields are accessed via `result.entry`/`result.skipped` without naming the type), but the `image` crate must be reachable from this integration test file: since `shimeji` is a lib crate and `image` is one of its dependencies (not a dev-dependency), add it to `[dev-dependencies]` in `Cargo.toml` too so `tests/importer.rs` (a separate compilation unit from `src/`) can call `image::RgbaImage`:

```bash
cargo add --dev image --no-default-features --features png
```

- [ ] **Step 6: Run the tests to verify they fail, then pass**

Run: `cargo test --test importer`
Expected: first run FAILS if the `image` dev-dependency is missing (compile error `use of undeclared crate or module 'image'`) — add it per Step 5's `cargo add` command, then re-run. Expected after: all 5 tests in `tests/importer.rs` PASS.

- [ ] **Step 7: Run the full suite**

Run: `cargo test`
Expected: every test in the crate passes — `cargo test` output should show 0 failures across the lib tests, `tests/importer.rs`, `tests/bundle_validation.rs`, and any other integration test files.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock src/importer/mod.rs src/app.rs tests/importer.rs
git commit -m "feat: accept shimeji-ee XML zips through import_zip

import_zip now falls back to the shimeji-ee translator (Tasks 2-7)
when a zip has no manifest.json at its root, and its success type
grows a `skipped` list (translated-but-degraded/dropped action and
behavior names) alongside the existing CatalogEntry so callers can
surface a heads-up -- App::import_from_bytes logs them via the
existing log_error path. Covers both the bare-character-folder and
full-app-distribution-prefix zip shapes end to end."
```

---

## Self-Review

**Spec coverage:**

| Spec section | Task |
|---|---|
| Architecture / `import_zip` fallback, `ImportResult` | 8 |
| New dependency `roxmltree` | 1 |
| Supporting change: `Serialize` on format types | 1 |
| Detection (both shapes, per-character override, optional behaviors.xml) | 6 |
| XML parsing (`RawAction`/`RawAnimationBlock`/`RawPose`/`RawActionRef`/`RawBehavior`/`RawBehaviorRef`) | 3 |
| Mapping: literal actions, loop_mode rule | 4 |
| Mapping: Fall/Jump physics baking hookup | 2, 4 |
| Mapping: Dragged, degraded embedded classes | 4 |
| Mapping: Sequence chaining (`on_finish`/`on_timer` split), whole-Sequence dropping | 4 |
| Mapping: entry/exit anchor resolution, Fall/Dragged required-action wiring | 4 |
| Sprite collection | 4 |
| Physics baking formulas | 2 |
| behaviors.xml → idle wandering, ref/behavior dropping, append-not-overwrite | 5 |
| Error handling (`ShimejiEeError` variants) | 3, 4, 6, 7 |
| Testing: synthetic fixture | 7 |
| Testing: unit tests per module | 2, 3, 4, 5, 6, 7 |
| Testing: integration test, both zip shapes | 8 |

Every spec section maps to at least one task. No gaps found.

**Placeholder scan:** no "TBD"/"TODO"/"add error handling"-style steps — every step above has literal code. The one intentionally-deferred item (`unimplemented!()` bodies) is the standard red-green-refactor scaffold each task's own later step replaces, not a plan placeholder.

**Type consistency check performed:**
- `SpriteFile { source: PathBuf, index: u32 }` — introduced in Task 4, used identically in Task 5 (`map_to_schema`'s return tuple), Task 7 (`ShimejiEeImport.sprites`, `write_into`), consistent throughout.
- `bake_fall(gravity, resistance_x, resistance_y, sprite, ticks)` / `bake_jump(velocity_param, sprite, ticks)` — defined in Task 2, called with matching argument order/types in Task 4.
- `MappedActions` fields (`animations`, `events`, `default_animation`, `sprites`, `skipped`, `leaf`, `sequences`) — defined in Task 4, destructured with the exact same field names in Task 5's `map_to_schema`.
- `DetectedMascot { actions_xml, behaviors_xml, img_dir }` — defined in Task 6, consumed with the same field names in Task 7's `try_import`.
- `ShimejiEeError` variants accumulate additively across Tasks 3/4/6/7 (`Xml`, `MissingRequiredAction`, `NoSprites`, `Io`) — Task 7's Step 1 restates the full four-variant enum explicitly so there's no ambiguity about the accumulated state by the time `mod.rs` is rewritten wholesale.
- `ImportResult { entry: CatalogEntry, skipped: Vec<String> }` — defined and consumed consistently in Task 8 across `src/importer/mod.rs`, `src/app.rs`, and `tests/importer.rs`.

No mismatches found.
