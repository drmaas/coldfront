// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

/// Failure from validation, parsing, or an upstream weather service.
#[derive(Debug)]
pub enum Error {
    InvalidZip,
    NotFound,
    Upstream(u16),
    Http(String),
    Parse(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidZip => write!(formatter, "enter a 5-digit US ZIP code"),
            Self::NotFound => write!(formatter, "no place found for that ZIP code"),
            Self::Upstream(status) => {
                write!(formatter, "weather service returned {status}")
            }
            Self::Http(message) | Self::Parse(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for Error {}
