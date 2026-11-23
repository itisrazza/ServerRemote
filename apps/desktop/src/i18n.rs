//! The [`fl!`] macro over the shared translations in `server_remote_app`.
//! Message IDs are checked at compile time against the English file there
//! (see this crate's `i18n.toml`).

pub use server_remote_app::i18n::select;

/// Looks up a message by ID, with optional `name = value` arguments.
macro_rules! fl {
    ($id:literal) => {
        i18n_embed_fl::fl!(server_remote_app::i18n::LOADER, $id)
    };
    ($id:literal, $($args:tt)*) => {
        i18n_embed_fl::fl!(server_remote_app::i18n::LOADER, $id, $($args)*)
    };
}
pub(crate) use fl;
