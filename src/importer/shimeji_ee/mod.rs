pub mod physics;
pub mod xml;

#[derive(Debug, thiserror::Error)]
pub enum ShimejiEeError {
    #[error("invalid shimeji-ee XML: {0}")]
    Xml(#[from] roxmltree::Error),
}
