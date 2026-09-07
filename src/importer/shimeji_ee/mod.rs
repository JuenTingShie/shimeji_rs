pub mod mapping;
pub mod physics;
pub mod xml;

#[derive(Debug, thiserror::Error)]
pub enum ShimejiEeError {
    #[error("invalid shimeji-ee XML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("required action '{0}' is missing or could not be translated")]
    MissingRequiredAction(String),
}
