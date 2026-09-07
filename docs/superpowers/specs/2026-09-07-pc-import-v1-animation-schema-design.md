# `pc_import_v1` Animation Schema Compatibility

**Goal:** Let shimeji_rs import mascot bundles whose `animation.json` declares
`schemaId: "pc_import_v1"` — a variant produced by at least one external
mascot-creator tool — instead of rejecting them as unsupported.

## Background

A user-supplied bundle (`momo.zip`) is already packaged in shimeji_rs's own
format (`manifest.json` + `animation.json` + `sprites/`, per
`src/format/manifest.rs` / `src/format/animation.rs`), not the unrelated
XML-based format used by the original Java "shimeji-ee" app
(`conf/actions.xml` + `conf/behaviors.xml` + `img/`). It fails to import for
two independent reasons:

1. `MascotBundle::load` (`src/format/bundle.rs:52`) rejects any
   `animation.schema_id` other than the literal string `"legacy_default_v1"`,
   returning `BundleError::UnsupportedAnimationSchema`. `momo.zip`'s
   `animation.json` declares `"schemaId": "pc_import_v1"`.
2. `Frame::dx` and `Frame::dy` (`src/format/animation.rs:32-38`) are
   required `i32` fields with no `#[serde(default)]`. In `momo.zip`'s
   `animation.json`, 106 of 163 frames omit `dx` and 129 of 163 omit `dy`
   entirely (a held pose like `stand_left` simply doesn't move) — so
   `serde_json::from_str::<AnimationSchema>` fails before the schema-id
   check is even reached.

Everything else in `momo.zip` — animation `type`/`subtype`/`loop`/
`direction` values, `auto.onFinish`/`onTimer`/`maxDurationTicks` shapes,
`borderTransitions`, and all 22 `events` entries (`TAP`, `FLING_START`,
`DRAG_START`) — was checked against the current `AnimationSchema`/
`EventRule`/`ChoiceItem` structs and matches exactly. `pc_import_v1` is a
different producer/version tag on the same JSON shape, not a new schema
with new semantics.

## Scope

- Accept `pc_import_v1` as a second valid `animation.schema_id`, alongside
  the existing `legacy_default_v1`.
- Default missing `Frame.dx`/`Frame.dy` to `0` (no displacement), which is
  what their absence means in practice.
- Not in scope: importing the original shimeji-ee XML format
  (`conf/*.xml` + `img/`) — a much larger, separate project the user
  explicitly deferred in favor of this narrower fix.
- Not in scope: any change to `manifest.rs`, the importer
  (`src/importer/mod.rs`), or the state machine — none of momo.zip's
  content exercises anything there that the parser doesn't already handle.

## Design

### `src/format/animation.rs`

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

### `src/format/bundle.rs`

Replace the single-string equality check with a small allowlist:

```rust
const SUPPORTED_ANIMATION_SCHEMAS: &[&str] = &["legacy_default_v1", "pc_import_v1"];

if !SUPPORTED_ANIMATION_SCHEMAS.contains(&animation.schema_id.as_str()) {
    return Err(BundleError::UnsupportedAnimationSchema { found: animation.schema_id.clone() });
}
```

Update the `BundleError::UnsupportedAnimationSchema` message (currently
`"expected legacy_default_v1"`) to name both accepted ids.

## Error handling

No new error variants. `BundleError::UnsupportedAnimationSchema` still
fires for any third schema id; `BundleError::AnimationParse` still fires
for genuinely malformed JSON. Both existing paths are untouched.

## Testing

Per the project's existing convention (commit `98c821c`: real/licensed
mascot art is never committed as a test fixture — synthetic fixtures only),
`momo.zip` itself is not added to the repo. Instead:

- `src/format/animation.rs` unit tests: add a case parsing a small inline
  JSON `Animation`/`Frame` with `dx`/`dy` omitted, asserting both default
  to `0`.
- `src/format/bundle.rs` unit tests: add a case where a synthetic
  `animation.json` (reusing the existing `tests/fixtures/fixture_bundle`
  layout, schema id swapped to `pc_import_v1`) loads successfully via
  `MascotBundle::load`, and a case confirming a third, still-unknown
  schema id is still rejected.
