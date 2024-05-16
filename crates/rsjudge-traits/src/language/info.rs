// SPDX-License-Identifier: Apache-2.0

//! Language information for plugins.

use indexmap::IndexMap;

/// Supported language information.
pub struct LanguageInfo {
    /// Name of the language.
    pub name: String,
    /// Customizable options for the language.
    pub config: IndexMap<String, ConfigInfo>,
}

/// Configuration information for the language.
pub enum ConfigInfo {
    /// A boolean configuration.
    Bool,
    /// An enumeration configuration.
    Enum(Vec<String>),
}
