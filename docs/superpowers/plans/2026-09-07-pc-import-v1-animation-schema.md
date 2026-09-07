# pc_import_v1 Animation Schema Compatibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let `MascotBundle::load` accept mascot bundles whose `animation.json` declares `schema_id: "pc_import_v1"` and whose frames omit `dx`/`dy`, so bundles like the user's `momo.zip` import successfully instead of failing.

**Architecture:** Two independent, additive relaxations to the existing JSON parsing in `src/format/animation.rs` and `src/format/bundle.rs`. No new modules, no changes to `manifest.rs`, the importer, or the state machine.

**Tech Stack:** Rust, `serde`/`serde_json` (existing dependencies, no new ones).

**Spec:** [docs/superpowers/specs/2026-09-07-pc-import-v1-animation-schema-design.md](../specs/2026-09-07-pc-import-v1-animation-schema-design.md)

## Global Constraints

- Accepted `animation.schema_id` values become exactly `"legacy_default_v1"` and `"pc_import_v1"` — any other value must still be rejected with `BundleError::UnsupportedAnimationSchema`.
- Missing `Frame.dx`/`Frame.dy` in JSON must deserialize to `0`, not fail parsing.
- No third-party or licensed mascot content (e.g. the real `momo.zip`) may be added to the repo as a test fixture — only synthetic fixtures/inline JSON, per commit `98c821c`.
- No changes to `src/format/manifest.rs`, `src/importer/`, or `src/state_machine/`.

---

### Task 1: Default missing `Frame.dx`/`dy` to zero

**Files:**
- Modify: `src/format/animation.rs:31-38` (the `Frame` struct)
- Test: `src/format/animation.rs` (existing `#[cfg(test)] mod tests` block at the bottom of the same file)

**Interfaces:**
- Consumes: nothing new.
- Produces: `Frame { sprite: u32, dx: i32, dy: i32, duration_ticks: u32 }` remains the exact same public shape used by `src/format/bundle.rs` (`frame.sprite`, iterated as `&anim.frames`) — Task 2 relies on `Frame` still being constructible from JSON that omits `dx`/`dy`.

- [ ] **Step 1: Write the failing test**

Open `src/format/animation.rs`. Inside the existing `#[cfg(test)] mod tests { use super::*; ... }` block at the end of the file, add this test function (alongside the existing `parses_sample_animation_schema` test, inside the same `mod tests`):

```rust
    #[test]
    fn frame_defaults_missing_dx_dy_to_zero() {
        let json = r#"{ "sprite": 7, "durationTicks": 602 }"#;
        let frame: Frame = serde_json::from_str(json).unwrap();
        assert_eq!(frame.sprite, 7);
        assert_eq!(frame.dx, 0);
        assert_eq!(frame.dy, 0);
        assert_eq!(frame.duration_ticks, 602);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib frame_defaults_missing_dx_dy_to_zero`
Expected: FAIL — `serde_json::from_str` returns `Err` (missing field `dx`), so the `.unwrap()` panics.

- [ ] **Step 3: Write minimal implementation**

In `src/format/animation.rs`, change the `Frame` struct (currently):

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Frame {
    pub sprite: u32,
    pub dx: i32,
    pub dy: i32,
    #[serde(rename = "durationTicks")]
    pub duration_ticks: u32,
}
```

to:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Frame {
    pub sprite: u32,
    #[serde(default)]
    pub dx: i32,
    #[serde(default)]
    pub dy: i32,
    #[serde(rename = "durationTicks")]
    pub duration_ticks: u32,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib frame_defaults_missing_dx_dy_to_zero`
Expected: PASS

- [ ] **Step 5: Run the full existing test suite to check for regressions**

Run: `cargo test --lib format::animation`
Expected: PASS — `parses_sample_animation_schema` (which asserts `walk_left.frames[0].dx == -2` from `tests/fixtures/fixture_animation.json`, where `dx`/`dy` are always present) still passes unaffected, since `#[serde(default)]` only changes behavior when the field is absent.

- [ ] **Step 6: Commit**

```bash
git add src/format/animation.rs
git commit -m "fix: default missing Frame dx/dy to zero"
```

---

### Task 2: Accept `pc_import_v1` as a second valid animation schema id

**Files:**
- Modify: `src/format/bundle.rs:17` (error message) and `src/format/bundle.rs:52-54` (the schema-id check)
- Test: `tests/bundle_validation.rs` (existing integration test file — reuses its `sample_copy()` helper)

**Interfaces:**
- Consumes: `Frame`'s `#[serde(default)]` on `dx`/`dy` from Task 1 (the regression test below relies on this).
- Produces: nothing new consumed by later tasks — this is the last task in the plan.

- [ ] **Step 1: Write the failing tests**

Open `tests/bundle_validation.rs`. Add these three tests after the existing `rejects_dangling_transition_target` test, inside the same file (reusing the `sample_copy()` helper already defined at the top):

```rust
#[test]
fn accepts_pc_import_v1_animation_schema() {
    let (_tmp, dir) = sample_copy();
    let animation_path = dir.join("animation.json");
    let text = fs::read_to_string(&animation_path).unwrap();
    let swapped = text.replacen("\"schema_id\": \"legacy_default_v1\"", "\"schema_id\": \"pc_import_v1\"", 1);
    fs::write(&animation_path, swapped).unwrap();

    let bundle = MascotBundle::load(&dir).unwrap();
    assert_eq!(bundle.animation.schema_id, "pc_import_v1");
}

#[test]
fn rejects_unknown_animation_schema_id() {
    let (_tmp, dir) = sample_copy();
    let animation_path = dir.join("animation.json");
    let text = fs::read_to_string(&animation_path).unwrap();
    let swapped = text.replacen("\"schema_id\": \"legacy_default_v1\"", "\"schema_id\": \"totally_unknown_v9\"", 1);
    fs::write(&animation_path, swapped).unwrap();

    let err = MascotBundle::load(&dir).unwrap_err();
    assert!(matches!(err, BundleError::UnsupportedAnimationSchema { found } if found == "totally_unknown_v9"));
}

#[test]
fn accepts_pc_import_v1_bundle_with_frames_missing_dx_dy() {
    let (_tmp, dir) = sample_copy();
    let animation_path = dir.join("animation.json");
    let text = fs::read_to_string(&animation_path).unwrap();
    let swapped = text
        .replacen("\"schema_id\": \"legacy_default_v1\"", "\"schema_id\": \"pc_import_v1\"", 1)
        .replacen("{ \"sprite\": 0, \"dx\": 0, \"dy\": 15, \"durationTicks\": 20 }", "{ \"sprite\": 0, \"durationTicks\": 20 }", 1);
    fs::write(&animation_path, swapped).unwrap();

    let bundle = MascotBundle::load(&dir).unwrap();
    let frame = &bundle.animation.animations[0].frames[0];
    assert_eq!(frame.dx, 0);
    assert_eq!(frame.dy, 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test bundle_validation`
Expected:
- `accepts_pc_import_v1_animation_schema` FAILS — `MascotBundle::load` returns `Err(BundleError::UnsupportedAnimationSchema { found: "pc_import_v1" })`, so `.unwrap()` panics.
- `rejects_unknown_animation_schema_id` PASSES already (current code already rejects anything that isn't `legacy_default_v1`) — this one is a safety-net test, not expected to fail, but run it now to confirm it passes both before and after Task 2's change.
- `accepts_pc_import_v1_bundle_with_frames_missing_dx_dy` FAILS — same `UnsupportedAnimationSchema` error as above.

- [ ] **Step 3: Write minimal implementation**

In `src/format/bundle.rs`, change the error message on line 17 from:

```rust
    #[error("unsupported animation schemaId '{found}' (expected legacy_default_v1)")]
    UnsupportedAnimationSchema { found: String },
```

to:

```rust
    #[error("unsupported animation schemaId '{found}' (expected legacy_default_v1 or pc_import_v1)")]
    UnsupportedAnimationSchema { found: String },
```

Then, above `impl MascotBundle`, add the allowlist constant:

```rust
const SUPPORTED_ANIMATION_SCHEMAS: &[&str] = &["legacy_default_v1", "pc_import_v1"];
```

Then change the schema-id check inside `MascotBundle::load` from:

```rust
        if animation.schema_id != "legacy_default_v1" {
            return Err(BundleError::UnsupportedAnimationSchema { found: animation.schema_id.clone() });
        }
```

to:

```rust
        if !SUPPORTED_ANIMATION_SCHEMAS.contains(&animation.schema_id.as_str()) {
            return Err(BundleError::UnsupportedAnimationSchema { found: animation.schema_id.clone() });
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test bundle_validation`
Expected: PASS — all tests in the file, including the three new ones and the four pre-existing ones (`loads_the_valid_sample_bundle`, `rejects_unsupported_manifest_schema_version`, `rejects_missing_sprite_file`, `rejects_dangling_transition_target`).

- [ ] **Step 5: Run the full test suite to check for regressions**

Run: `cargo test`
Expected: PASS — every test in the crate (unit tests across `src/format/`, `src/state_machine/`, plus `tests/bundle_validation.rs` and `tests/importer.rs`) succeeds.

- [ ] **Step 6: Commit**

```bash
git add src/format/bundle.rs tests/bundle_validation.rs
git commit -m "feat: accept pc_import_v1 as a valid animation schema id"
```
