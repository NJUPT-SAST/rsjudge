// SPDX-License-Identifier: Apache-2.0

//! Language option for judging code.

use indexmap::IndexMap;

/// Language option for judging code.
pub struct LanguageOption {
    /// Name of the language.
    pub name: String,
    /// Customizable options for the language.
    pub config: IndexMap<String, ConfigValue>,
}

/// Configuration value for a specific config item.
pub enum ConfigValue {
    /// A boolean configuration.
    Bool(bool),
    /// An enumeration configuration.
    Enum(String),
}
