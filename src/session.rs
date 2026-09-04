use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEntry {
    pub slug: String,
    pub scale: f64,
    pub speed: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("could not read session.json: {0}")]
    Read(#[source] std::io::Error),
    #[error("could not write session.json: {0}")]
    Write(#[source] std::io::Error),
    #[error("session.json is corrupt: {0}")]
    Parse(#[source] serde_json::Error),
}

pub fn load_session(path: &Path) -> Result<Vec<SessionEntry>, SessionError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path).map_err(SessionError::Read)?;
    serde_json::from_str(&text).map_err(SessionError::Parse)
}

pub fn save_session(path: &Path, entries: &[SessionEntry]) -> Result<(), SessionError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(SessionError::Write)?;
    }
    let text = serde_json::to_string_pretty(entries).expect("SessionEntry always serializes");
    std::fs::write(path, text).map_err(SessionError::Write)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_loads_as_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("session.json");
        assert_eq!(load_session(&path).unwrap(), Vec::new());
    }

    #[test]
    fn round_trips_session_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("session.json");
        let entries = vec![
            SessionEntry { slug: "usagi".into(), scale: 1.0, speed: 1.0 },
            SessionEntry { slug: "neko".into(), scale: 1.5, speed: 0.8 },
        ];
        save_session(&path, &entries).unwrap();
        assert_eq!(load_session(&path).unwrap(), entries);
    }

    #[test]
    fn corrupt_json_is_a_parse_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("session.json");
        std::fs::write(&path, "not json").unwrap();
        let err = load_session(&path).unwrap_err();
        assert!(matches!(err, SessionError::Parse(_)));
    }

    #[test]
    fn save_creates_missing_parent_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("dir").join("session.json");
        save_session(&path, &[]).unwrap();
        assert!(path.exists());
    }
}
