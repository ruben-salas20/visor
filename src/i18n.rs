//! Interface language: Spanish or English, fixed when visor starts.
//!
//! Texts are written as pairs next to each other (`tr("es", "en")`) so each
//! string sits beside its translation. There is no catalogue file to keep in step.

use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Es,
    En,
}

impl Lang {
    /// The code that pages read in `__VISOR_LANG__`.
    pub fn code(self) -> &'static str {
        match self {
            Lang::Es => "es",
            Lang::En => "en",
        }
    }
}

static LANG: OnceLock<Lang> = OnceLock::new();

/// The language for a saved choice (`"es"`, `"en"`, or `None` for the
/// system's) and the value of `LANG`. Kept apart from `init` so it can be tested.
fn decide(choice: Option<&str>, locale: Option<&str>) -> Lang {
    match choice {
        Some("es") => Lang::Es,
        Some("en") => Lang::En,
        _ if locale.is_some_and(|l| l.starts_with("es")) => Lang::Es,
        _ => Lang::En,
    }
}

/// Call once, at the start of `main`. A second call changes nothing.
pub fn init(choice: Option<&str>) {
    let locale = std::env::var("LANG").ok();
    let _ = LANG.set(decide(choice, locale.as_deref()));
}

/// Without `init` (tests), Spanish: the texts the tests compare stay the same.
pub fn lang() -> Lang {
    LANG.get().copied().unwrap_or(Lang::Es)
}

pub fn tr(es: &'static str, en: &'static str) -> &'static str {
    match lang() {
        Lang::Es => es,
        Lang::En => en,
    }
}

/// `format!` with the Spanish or the English literal. Arguments are written
/// once per branch, so they must be cheap and free of side effects.
#[macro_export]
macro_rules! trf {
    ($es:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        match $crate::i18n::lang() {
            $crate::i18n::Lang::Es => format!($es $(, $arg)*),
            $crate::i18n::Lang::En => format!($en $(, $arg)*),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_choice_wins_over_locale() {
        assert_eq!(decide(Some("es"), Some("en_US.UTF-8")), Lang::Es);
        assert_eq!(decide(Some("en"), Some("es_ES.UTF-8")), Lang::En);
    }

    #[test]
    fn no_choice_follows_locale() {
        assert_eq!(decide(None, Some("es_ES.UTF-8")), Lang::Es);
        assert_eq!(decide(None, Some("es")), Lang::Es);
        assert_eq!(decide(None, Some("en_US.UTF-8")), Lang::En);
        assert_eq!(decide(None, Some("C.UTF-8")), Lang::En);
    }

    #[test]
    fn no_choice_and_no_locale_is_english() {
        assert_eq!(decide(None, None), Lang::En);
    }

    #[test]
    fn unknown_choice_follows_locale() {
        assert_eq!(decide(Some("fr"), Some("es_ES.UTF-8")), Lang::Es);
        assert_eq!(decide(Some("fr"), None), Lang::En);
    }

    #[test]
    fn tests_without_init_get_spanish() {
        assert_eq!(lang(), Lang::Es);
        assert_eq!(tr("hola", "hello"), "hola");
        let n = 3;
        assert_eq!(trf!("{n} notas", "{n} notes"), "3 notas");
    }
}
