use super::ShimejiEeError;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct DetectedMascot {
    pub actions_xml: PathBuf,
    pub behaviors_xml: Option<PathBuf>,
    pub img_dir: PathBuf,
}

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
