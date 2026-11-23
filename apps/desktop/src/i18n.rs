//! Translations: Fluent files under `i18n/<language>/`, embedded at build
//! time. English is the fallback for anything a translation lacks, and the
//! [`fl!`] macro checks message IDs against it at compile time.

use std::sync::LazyLock;

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::unic_langid::LanguageIdentifier;
use i18n_embed::{DesktopLanguageRequester, LanguageLoader};
use rust_embed::RustEmbed;

use crate::persistence::settings::Language;

#[derive(RustEmbed)]
#[folder = "i18n"]
struct Localizations;

pub static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("the embedded English translation loads");
    // Fluent wraps arguments in Unicode isolation marks by default, which
    // the UI font draws as boxes.
    loader.set_use_isolating(false);
    loader
});

/// Looks up a message by ID, with optional `name = value` arguments.
macro_rules! fl {
    ($id:literal) => {
        i18n_embed_fl::fl!($crate::i18n::LOADER, $id)
    };
    ($id:literal, $($args:tt)*) => {
        i18n_embed_fl::fl!($crate::i18n::LOADER, $id, $($args)*)
    };
}
pub(crate) use fl;

/// Switches the UI to `language`, or to the system's preferred languages.
pub fn select(language: Language) {
    let requested = match language {
        Language::System => DesktopLanguageRequester::requested_languages(),
        Language::English => vec![identifier("en")],
        Language::Romanian => vec![identifier("ro")],
    };
    if let Err(error) = i18n_embed::select(&*LOADER, &Localizations, &requested) {
        eprintln!("failed to select a language from {requested:?}: {error}");
    }
}

fn identifier(tag: &str) -> LanguageIdentifier {
    tag.parse().expect("built-in language tags are valid")
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use super::*;

    /// Every language besides English.
    const TRANSLATIONS: &[&str] = &["ro"];

    /// A loader of its own, so tests don't switch the app's language.
    fn loader(tags: &[&str]) -> FluentLanguageLoader {
        let loader = fluent_language_loader!();
        let languages: Vec<_> = tags.iter().map(|tag| identifier(tag)).collect();
        loader
            .load_languages(&Localizations, &languages)
            .expect("the embedded translations load");
        loader.set_use_isolating(false);
        loader
    }

    fn message_ids(tag: &str) -> BTreeSet<String> {
        loader(&[tag]).with_message_iter(&identifier(tag), |messages| {
            messages.map(|message| message.id.name.to_owned()).collect()
        })
    }

    #[test]
    fn translations_have_exactly_the_english_messages() {
        let english = message_ids("en");
        for &tag in TRANSLATIONS {
            let translated = message_ids(tag);
            let missing: Vec<_> = english.difference(&translated).collect();
            let extra: Vec<_> = translated.difference(&english).collect();
            assert!(
                missing.is_empty() && extra.is_empty(),
                "{tag}: missing {missing:?}, unknown {extra:?}"
            );
        }
    }

    #[test]
    fn plurals_follow_each_language() {
        let cases = [
            ("en", 1, "The open connection will be closed."),
            ("en", 2, "All open connections will be closed."),
            ("ro", 1, "Conexiunea deschisă va fi închisă."),
            ("ro", 2, "Toate conexiunile deschise vor fi închise."),
            ("ro", 20, "Toate conexiunile deschise vor fi închise."),
        ];
        for (tag, count, expected) in cases {
            let message =
                loader(&[tag, "en"]).get_args("quit-message", HashMap::from([("count", count)]));
            assert_eq!(message, expected, "{tag}, {count}");
        }
    }
}
