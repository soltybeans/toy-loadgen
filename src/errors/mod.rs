use std::fmt::Display;

#[derive(Debug)]
pub enum LoadGenError {
    InvalidPortError(String),
    NoResultsError,
}

impl std::error::Error for LoadGenError {}

impl Display for LoadGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadGenError::InvalidPortError(port) => write!(f, "[LoadGeneratorError]: {} is an invalid port!", port),
            LoadGenError::NoResultsError => write!(f, "[LoadGeneratorError]: No results are available! Connection issue for full duration of tests.")
        }
    }
}