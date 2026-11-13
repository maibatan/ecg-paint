//! Errors carry the stage they happened in. Every error ends the conversion with exit
//! code −2, like the original; the stage only makes the message useful.

use std::fmt;

pub const ARGS: &str = "args";
pub const INPUT: &str = "input";
pub const PARSE: &str = "parse";
pub const RENDER: &str = "render";
pub const SAVE: &str = "save";

#[derive(Debug)]
pub struct Error {
    pub stage: &'static str,
    pub message: String,
}

impl Error {
    pub fn new(stage: &'static str, message: impl Into<String>) -> Self {
        Error {
            stage,
            message: message.into(),
        }
    }

    /// A parse error at a file offset (the original's internal codes are kept in the text).
    pub fn at(offset: usize, message: impl Into<String>) -> Self {
        Error::new(PARSE, format!("offset {:#x}: {}", offset, message.into()))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.stage, self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
