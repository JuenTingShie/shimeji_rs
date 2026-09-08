use super::physics::{bake_fall, bake_jump};
use super::xml;
use super::xml::{RawAction, RawAnimationBlock, RawPose};
use super::ShimejiEeError;
use crate::format::animation::{
    Animation, AutoBehavior, BorderTransition, ChoiceItem, Direction, Edge, EngineEventKind, EventRule, Frame, LoopMode,
    SurfaceType, TimerRule,
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

#[derive(Debug)]
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
        // The engine never actually posts DragEnd (see window/mascot_window.rs's WM_LBUTTONUP
        // handler: every mouse release is classified into Tap or FlingStart, never DragEnd), so
        // those are the two rules that matter for "let go of the mascot" -- DragEnd/FlingEnd are
        // kept only as harmless fallbacks in case that ever changes.
        wildcard_event(EngineEventKind::DragEnd, &default_animation),
        wildcard_event(EngineEventKind::FlingEnd, &default_animation),
        wildcard_event(EngineEventKind::FlingStart, &default_animation),
        wildcard_event(EngineEventKind::Tap, &default_animation),
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
        Some(DRAGGED_CLASS) => block.poses.iter().map(|p| literal_frame(p, img_dir, sprite_index, sprites)).collect(),
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
    // shimeji-ee's own border-hit branch idiom: a physics action (Fall/Jump) immediately
    // followed by a Select choosing floor-landing vs. wall-landing continuations. Recognized
    // narrowly -- exactly this two-step shape, with exactly one floor-conditioned branch and
    // one unconditioned fallback -- rather than interpreting arbitrary Select/Condition logic.
    if let [xml::RawSequenceStep::Reference(physics_ref), xml::RawSequenceStep::Select(branches)] = action.steps.as_slice() {
        if let Some(anchors) = resolve_physics_select(physics_ref, branches, leaf, sequences, animations) {
            return Some(anchors);
        }
        return None;
    }

    let refs: Vec<xml::RawActionRef> = action
        .steps
        .iter()
        .map(|step| match step {
            xml::RawSequenceStep::Reference(r) => Some(r.clone()),
            xml::RawSequenceStep::Select(_) => None,
        })
        .collect::<Option<Vec<_>>>()?;

    resolve_reference_chain(&refs, leaf, sequences, animations)
}

/// Resolves a chain of plain ActionReferences (no Select) -- shared by the common case above
/// and by each branch of a physics Select's own internal ActionReference chain.
fn resolve_reference_chain(
    refs: &[xml::RawActionRef],
    leaf: &HashMap<String, String>,
    sequences: &HashMap<String, (String, String)>,
    animations: &mut [Animation],
) -> Option<(String, String)> {
    let mut resolved = Vec::with_capacity(refs.len());
    for r in refs {
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
        let duration_ticks = refs[i].duration.as_deref().and_then(|d| d.parse::<u32>().ok());
        link(animations, &from_exit, &to_entry, duration_ticks);
    }

    let entry = resolved.first().unwrap().0.clone();
    let exit = resolved.last().unwrap().1.clone();
    Some((entry, exit))
}

fn resolve_physics_select(
    physics_ref: &xml::RawActionRef,
    branches: &[xml::RawSelectBranch],
    leaf: &HashMap<String, String>,
    sequences: &HashMap<String, (String, String)>,
    animations: &mut [Animation],
) -> Option<(String, String)> {
    let physics_entry = resolve_entry(&physics_ref.name, leaf, sequences)?;
    resolve_exit(&physics_ref.name, leaf, sequences)?;

    let [floor_branch, wall_branch] = branches else { return None };
    let is_floor_branch = floor_branch.condition.as_deref().is_some_and(|c| c.to_lowercase().contains("floor"));
    if !is_floor_branch || wall_branch.condition.is_some() {
        return None;
    }

    let (floor_entry, floor_exit) = resolve_reference_chain(&floor_branch.refs, leaf, sequences, animations)?;
    let (wall_entry, _wall_exit) = resolve_reference_chain(&wall_branch.refs, leaf, sequences, animations)?;

    // A physics leaf like Falling is typically referenced by many different Sequences
    // (Fall, Thrown, several Jump-landing variants in a real mascot) that all resolve to the
    // same floor/wall outcome -- add each border edge at most once rather than accumulating a
    // duplicate entry per referencing Sequence. If a later Sequence disagrees about the
    // target for the same edge, the first one processed wins silently; shimeji_rs's model has
    // no notion of "which calling Sequence" a shared leaf animation is currently serving.
    let physics_anim = animations.iter_mut().find(|a| a.key == physics_entry)?;
    push_border_transition_once(physics_anim, Edge::Bottom, &floor_entry);
    push_border_transition_once(physics_anim, Edge::Left, &wall_entry);
    push_border_transition_once(physics_anim, Edge::Right, &wall_entry);

    Some((physics_entry, floor_exit))
}

fn push_border_transition_once(anim: &mut Animation, when: Edge, to: &str) {
    if anim.border_transitions.iter().any(|bt| bt.when == when) {
        return;
    }
    anim.border_transitions.push(BorderTransition {
        when,
        facing: None,
        choices: vec![ChoiceItem { to: to.to_string(), weight: 1.0, set_facing: None, min_level: None }],
    });
}

fn link(animations: &mut [Animation], from_key: &str, to_key: &str, duration_ticks: Option<u32>) {
    let Some(anim) = animations.iter_mut().find(|a| a.key == from_key) else { return };
    let is_oneshot = matches!(anim.loop_mode, LoopMode::Oneshot);
    let auto = anim.auto.get_or_insert_with(AutoBehavior::default);

    // A leaf step referenced from many different Sequences (e.g. "Bouncing" following every
    // Fall/Thrown/Jump-landing variant in a real mascot) would otherwise accumulate one
    // duplicate choice per referencing Sequence -- add it at most once per target instead.
    if is_oneshot {
        if auto.on_finish.iter().any(|c| c.to == to_key) {
            return;
        }
        auto.on_finish.push(ChoiceItem { to: to_key.to_string(), weight: 1.0, set_facing: None, min_level: None });
    } else {
        if auto.on_timer.iter().any(|rule| rule.choices.iter().any(|c| c.to == to_key)) {
            return;
        }
        let ticks = duration_ticks.unwrap_or(SEQUENCE_STEP_FALLBACK_TICKS);
        let choice = ChoiceItem { to: to_key.to_string(), weight: 1.0, set_facing: None, min_level: None };
        auto.on_timer.push(TimerRule { choices: vec![choice], min_ticks: ticks, max_ticks: ticks, chance: 1.0 });
    }
}

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
        // Structurally identical to legacy_default_v1 (flat top-level `events`, no inline
        // event_transitions) -- reuse that schema_id rather than inventing a new one, since
        // MascotBundle::load's schema allowlist is a Global Constraint this feature never
        // touches.
        schema_id: "legacy_default_v1".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use super::xml::RawBehavior;

    fn literal_action(name: &str, kind: &str, border_type: Option<&str>, poses: Vec<RawPose>) -> RawAction {
        RawAction {
            name: name.to_string(),
            kind: kind.to_string(),
            border_type: border_type.map(str::to_string),
            class: None,
            loop_flag: false,
            params: HashMap::new(),
            animations: vec![RawAnimationBlock { condition: None, poses }],
            steps: Vec::new(),
        }
    }

    fn pose(image: &str, dx: i32, dy: i32, duration: u32) -> RawPose {
        RawPose { image: image.to_string(), velocity: (dx, dy), duration }
    }

    fn action_ref(name: &str, duration: Option<&str>) -> xml::RawActionRef {
        xml::RawActionRef { name: name.to_string(), duration: duration.map(str::to_string) }
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
            steps: refs.into_iter().map(|(n, d)| xml::RawSequenceStep::Reference(action_ref(n, d))).collect(),
        }
    }

    fn sequence_with_select(
        name: &str,
        physics_ref: &str,
        floor_condition: Option<&str>,
        floor_refs: Vec<(&str, Option<&str>)>,
        wall_refs: Vec<(&str, Option<&str>)>,
    ) -> RawAction {
        RawAction {
            name: name.to_string(),
            kind: "Sequence".to_string(),
            border_type: None,
            class: None,
            loop_flag: false,
            params: HashMap::new(),
            animations: Vec::new(),
            steps: vec![
                xml::RawSequenceStep::Reference(action_ref(physics_ref, None)),
                xml::RawSequenceStep::Select(vec![
                    xml::RawSelectBranch {
                        condition: floor_condition.map(str::to_string),
                        refs: floor_refs.into_iter().map(|(n, d)| action_ref(n, d)).collect(),
                    },
                    xml::RawSelectBranch { condition: None, refs: wall_refs.into_iter().map(|(n, d)| action_ref(n, d)).collect() },
                ]),
            ],
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

    /// Mirrors the real test_mascot.zip shape: Fall = [Falling, Select[floor: Bouncing->Stand,
    /// wall: GrabWall]], the exact structure that was silently dropped before Select parsing
    /// existed, leaving Falling with no border_transitions and no way out of its baked physics.
    fn fall_with_border_select_and_dragged() -> Vec<RawAction> {
        let mut falling = literal_action("Falling", "Embedded", None, vec![pose("/falling.png", 0, 0, 250)]);
        falling.class = Some("com.group_finity.mascot.action.Fall".to_string());
        falling.params.insert("Gravity".to_string(), 2.0);

        let mut dragged_pose_action = literal_action("Pinched", "Embedded", None, vec![pose("/pinched.png", 0, 0, 5)]);
        dragged_pose_action.class = Some("com.group_finity.mascot.action.Dragged".to_string());

        vec![
            falling,
            dragged_pose_action,
            literal_action("Bouncing", "Animate", Some("Floor"), vec![pose("/bounce.png", 0, 0, 4)]),
            literal_action("Stand", "Stay", Some("Floor"), vec![pose("/stand.png", 0, 0, 250)]),
            literal_action("GrabWall", "Stay", Some("Wall"), vec![pose("/wall.png", 0, 0, 250)]),
            sequence_with_select(
                "Fall",
                "Falling",
                Some("${mascot.environment.floor.isOn(mascot.anchor)}"),
                vec![("Bouncing", None), ("Stand", Some("150"))],
                vec![("GrabWall", Some("100"))],
            ),
            sequence("Dragged", vec![("Pinched", None)]),
        ]
    }

    #[test]
    fn physics_select_attaches_border_transitions_to_the_physics_animation() {
        let mapped = map_actions(&fall_with_border_select_and_dragged(), Path::new("/img")).unwrap();
        let falling = mapped.animations.iter().find(|a| a.key == "Falling").unwrap();

        let bottom = falling.border_transitions.iter().find(|bt| bt.when == Edge::Bottom).unwrap();
        assert_eq!(bottom.choices[0].to, "Bouncing");

        let left = falling.border_transitions.iter().find(|bt| bt.when == Edge::Left).unwrap();
        let right = falling.border_transitions.iter().find(|bt| bt.when == Edge::Right).unwrap();
        assert_eq!(left.choices[0].to, "GrabWall");
        assert_eq!(right.choices[0].to, "GrabWall");
    }

    #[test]
    fn physics_select_chains_the_floor_branchs_own_steps_and_reports_the_sequences_exit() {
        let mapped = map_actions(&fall_with_border_select_and_dragged(), Path::new("/img")).unwrap();
        let bouncing = mapped.animations.iter().find(|a| a.key == "Bouncing").unwrap();
        let auto = bouncing.auto.as_ref().expect("Bouncing should chain to Stand");
        assert_eq!(auto.on_finish[0].to, "Stand");
        assert_eq!(mapped.sequences.get("Fall"), Some(&("Falling".to_string(), "Stand".to_string())));
    }

    #[test]
    fn physics_select_with_two_floor_conditioned_branches_is_unresolvable() {
        let mut actions = fall_with_border_select_and_dragged();
        let fall = actions.iter_mut().find(|a| a.name == "Fall").unwrap();
        fall.steps = vec![
            xml::RawSequenceStep::Reference(action_ref("Falling", None)),
            xml::RawSequenceStep::Select(vec![
                xml::RawSelectBranch { condition: Some("floor".to_string()), refs: vec![action_ref("Bouncing", None)] },
                xml::RawSelectBranch { condition: Some("floor".to_string()), refs: vec![action_ref("GrabWall", None)] },
            ]),
        ];
        // Fall becomes unresolvable, and Fall is a required action (see
        // missing_fall_action_is_an_error) -- the whole import fails rather than silently
        // producing a bundle with no default_animation.
        let err = map_actions(&actions, Path::new("/img")).unwrap_err();
        assert!(matches!(err, ShimejiEeError::MissingRequiredAction(name) if name == "Fall"));
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
    fn a_leaf_referenced_by_multiple_sequences_gets_only_one_choice_per_target() {
        // Mirrors the real test_mascot.zip mascot: "Falling" is chained to "Bouncing" from several
        // independent Sequences (Land here stands in for Fall/Thrown/JumpFromLeftEdgeOfIE/...).
        let actions = {
            let mut a = fall_and_dragged();
            a.push(literal_action("Bouncing", "Animate", Some("Floor"), vec![pose("/b.png", 0, 0, 4)]));
            a.push(sequence("Land", vec![("Falling", None), ("Bouncing", None)]));
            a.push(sequence("LandAgain", vec![("Falling", None), ("Bouncing", None)]));
            a.push(sequence("LandYetAgain", vec![("Falling", None), ("Bouncing", None)]));
            a
        };
        let mapped = map_actions(&actions, Path::new("/img")).unwrap();
        let falling = mapped.animations.iter().find(|a| a.key == "Falling").unwrap();
        let auto = falling.auto.as_ref().unwrap();
        assert_eq!(auto.on_finish.len(), 1, "three Sequences chaining to the same target must not triple the choice list");
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
        assert!(mapped.events.iter().any(|e| e.event == EngineEventKind::DragStart && e.to.as_deref() == Some("Pinched")));
        // FlingStart and Tap are what the engine actually posts on mouse release (see
        // window/mascot_window.rs) -- without these, releasing a dragged mascot leaves it frozen
        // in its Dragged pose forever, since neither DragEnd nor a "fling"-named animation
        // (both of which the schema also wires, as harmless fallbacks) is ever reached.
        assert!(mapped.events.iter().any(|e| e.event == EngineEventKind::FlingStart && e.to.as_deref() == Some("Falling")));
        assert!(mapped.events.iter().any(|e| e.event == EngineEventKind::Tap && e.to.as_deref() == Some("Falling")));
        assert!(mapped.events.iter().any(|e| e.event == EngineEventKind::DragEnd && e.to.as_deref() == Some("Falling")));
        assert!(mapped.events.iter().any(|e| e.event == EngineEventKind::FlingEnd && e.to.as_deref() == Some("Falling")));
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
    fn map_to_schema_produces_a_legacy_default_v1_schema_id() {
        let (schema, _sprites, _skipped) = map_to_schema(&fall_and_dragged(), &[], Path::new("/img")).unwrap();
        // Structurally identical to a hand-authored legacy_default_v1 bundle (flat top-level
        // events, no inline event_transitions), so it's labeled the same way rather than
        // requiring MascotBundle::load's schema allowlist to be touched.
        assert_eq!(schema.schema_id, "legacy_default_v1");
        assert_eq!(schema.default_animation, "Falling");
        assert_eq!(schema.initial_candidates, vec!["Falling".to_string()]);
    }
}
