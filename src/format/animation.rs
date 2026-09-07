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
    #[serde(default)]
    pub event_transitions: Vec<EventRule>,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_defaults_missing_dx_dy_to_zero() {
        let json = r#"{ "sprite": 7, "durationTicks": 602 }"#;
        let frame: Frame = serde_json::from_str(json).unwrap();
        assert_eq!(frame.sprite, 7);
        assert_eq!(frame.dx, 0);
        assert_eq!(frame.dy, 0);
        assert_eq!(frame.duration_ticks, 602);
    }

    #[test]
    fn animation_defaults_missing_event_transitions_to_empty() {
        let json = r#"{
            "key": "drag", "type": "USER", "subtype": "DRAG", "level": 1,
            "loop": "LOOP", "direction": "ANY",
            "frames": [ { "sprite": 9, "durationTicks": 8 } ]
        }"#;
        let anim: Animation = serde_json::from_str(json).unwrap();
        assert!(anim.event_transitions.is_empty());
    }

    #[test]
    fn parses_inline_event_transitions_on_an_animation() {
        let json = r#"{
            "key": "drag", "type": "USER", "subtype": "DRAG", "level": 1,
            "loop": "LOOP", "direction": "ANY",
            "frames": [ { "sprite": 9, "durationTicks": 8 } ],
            "eventTransitions": [
                { "event": "DRAG_END", "from": "drag", "to": "fall", "setFacing": "RANDOM" }
            ]
        }"#;
        let anim: Animation = serde_json::from_str(json).unwrap();
        assert_eq!(anim.event_transitions.len(), 1);
        assert_eq!(anim.event_transitions[0].event, EngineEventKind::DragEnd);
        assert_eq!(anim.event_transitions[0].from.as_deref(), Some("drag"));
        assert_eq!(anim.event_transitions[0].to.as_deref(), Some("fall"));
    }

    #[test]
    fn parses_sample_animation_schema() {
        let json = std::fs::read_to_string("tests/fixtures/fixture_animation.json").unwrap();
        let schema: AnimationSchema = serde_json::from_str(&json).unwrap();

        assert_eq!(schema.schema_id, "legacy_default_v1");
        assert_eq!(schema.default_animation, "fall");
        assert_eq!(schema.initial_candidates, vec!["fall".to_string()]);
        assert_eq!(schema.animations.len(), 15);
        assert_eq!(schema.events.len(), 12);

        let walk_left = schema
            .animations
            .iter()
            .find(|a| a.key == "walk_left")
            .expect("walk_left animation must exist");
        assert_eq!(walk_left.kind, SurfaceType::Ground);
        assert_eq!(walk_left.loop_mode, LoopMode::Loop);
        assert_eq!(walk_left.direction, Direction::Left);
        assert_eq!(walk_left.frames.len(), 4);
        assert_eq!(walk_left.frames[0].sprite, 3);
        assert_eq!(walk_left.frames[0].dx, -2);
        assert_eq!(walk_left.frames[0].duration_ticks, 6);

        let auto = walk_left.auto.as_ref().expect("walk_left has auto behavior");
        assert_eq!(auto.on_timer.len(), 1);
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
            Some(MaxDurationTicks::Range { min_ticks: 100, max_ticks: 200 })
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
        assert_eq!(tap_level4_ground.choices.as_ref().unwrap().len(), 2);
    }
}
