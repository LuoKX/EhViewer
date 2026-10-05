//! Network request and response parsing for `EhViewer`'s Rust-first core.
//!
//! This crate owns remote-protocol knowledge such as URL shapes and HTML/JSON
//! response parsing. It depends on [`eh_domain`] for stable output contracts and
//! deliberately has no Android or JNI dependency. Parsers accept response bytes
//! or text and remain host-testable with checked-in offline fixtures.

macro_rules! regex {
    ($pattern:literal $(,)?) => {{
        static REGEX: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
        REGEX.get_or_init(|| {
            regex_lite::Regex::new($pattern).expect("a source-controlled regex must compile")
        })
    }};
}

pub mod auth;
pub mod cache_keys;
pub mod gallery;
pub mod home;
mod html;
pub mod news;
pub mod reader;
pub mod tag_translation;
#[cfg(test)]
mod test_support;
mod text;
pub mod torrent;
pub mod user_config;
