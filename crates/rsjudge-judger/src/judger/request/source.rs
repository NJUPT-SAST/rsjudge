use bytes::Bytes;
use rsjudge_traits::language::option::LanguageOption;

pub struct Source {
    pub(crate) language: LanguageOption,
    pub(crate) code: Bytes,
}

impl Source {
    pub fn new(language: LanguageOption, code: Bytes) -> Self {
        Self { language, code }
    }

    pub fn language(&self) -> &LanguageOption {
        &self.language
    }

    pub fn code(&self) -> &Bytes {
        &self.code
    }
}
