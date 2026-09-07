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

#[cfg(test)]
mod tests {
    use super::*;

    const ACTIONS_XML: &str = r##"<?xml version="1.0" encoding="UTF-8" ?>
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
</Mascot>"##;

    const BEHAVIORS_XML: &str = r##"<?xml version="1.0" encoding="UTF-8" ?>
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
</Mascot>"##;

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
