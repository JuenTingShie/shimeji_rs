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

#[cfg(test)]
mod tests {
    use super::*;
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
