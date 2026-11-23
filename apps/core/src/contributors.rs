//! The people who made the app, for the About page. The list and avatars are
//! fetched from GitHub by `scripts/fetch-contributors.py`.

pub struct Contributor {
    pub login: &'static str,
    /// A small square photo (PNG or JPEG).
    pub avatar: &'static [u8],
}

#[path = "../assets/contributors/generated.rs"]
mod generated;

pub use generated::ALL;
