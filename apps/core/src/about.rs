//! What every front end's About page shows: links and who made the app.

pub use crate::contributors::{ALL as CONTRIBUTORS, Contributor};
pub use crate::update::{PROJECT_URL, VERSION};

pub const WEBSITE_URL: &str = "https://serverremote.razza.io/";
pub const MIT_LICENSE_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/blob/main/LICENSE-MIT");
pub const APACHE_LICENSE_URL: &str =
    concat!(env!("CARGO_PKG_REPOSITORY"), "/blob/main/LICENSE-APACHE");
/// The icon set connectors and the app draw from.
pub const CARBON_URL: &str = "https://carbondesignsystem.com/elements/icons/library/";
