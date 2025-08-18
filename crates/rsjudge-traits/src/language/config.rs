// SPDX-License-Identifier: Apache-2.0

//! Language representation from configuration file.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Language definition from configuration file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageDef {
    #[serde(flatten)]
    exec_type: ExecType,

    #[serde(default)]
    options: IndexMap<String, ConfigDef>,
    version: Option<String>,
}

/// Execution type of the language.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "exec_type")]
pub enum ExecType {
    /// The language is compiled to a binary, which can be executed directly.
    Binary {
        /// Compilation command for the language.
        compile: String,
    },
    /// The language is compiled to an intermediate representation, which is
    /// then executed with another command.
    ByteCode {
        /// Compilation command for the language.
        compile: String,
        /// Execution command for the intermediate representation.
        execute: String,
    },
    /// The language is executed directly with a command.
    SourceCode {
        /// An optional command to check the syntax of the code.
        check: Option<String>,
        /// Execution command for the source code.
        execute: String,
    },
}

/// Additional dynamic configuration definition for the language.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ConfigDef {
    /// A boolean configuration.
    Bool {
        /// Where to put the `enable` value. Often a variable in compilation or
        /// execution command.
        target: String,
        /// Default value of the configuration.
        default: bool,
        /// Value to enable the configuration.
        enable: String,
    },
    /// An enumeration configuration.
    Enum {
        /// Where to put the value of the `variants` configuration
        target: String,
        /// Default variant of the configuration.
        default: String,
        /// Variants of the configuration with their corresponding values.
        variants: IndexMap<String, String>,
    },
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Read;

    use indexmap::{IndexMap, indexmap};
    use toml::toml;

    use super::{ConfigDef, ExecType, LanguageDef};

    #[test]
    fn test_language() {
        let c_def = LanguageDef {
            exec_type: ExecType::Binary {
                compile: "gcc {...flags} -o {out_file} {src_file}".into(),
            },

            options: indexmap! {
                "O2".into() => ConfigDef::Bool {
                    default: true,
                    target: "flags".into(),
                    enable: "-O2".into()
                },
                "version".into() => ConfigDef::Enum {
                    default: "C99".into(),
                    variants: indexmap! {
                        "C99".into() => "-std=c99".into(),
                        "C11".into() => "-std=c11".into()
                    },
                    target: "flags".into()
                }
            },
            version: Some("$(gcc --version)".into()),
        };

        let languages = HashMap::from([("C".to_string(), c_def)]);

        let json = serde_json::to_string_pretty(&languages).unwrap();

        println!("{json}");

        println!(
            "{:#?}",
            serde_json::from_str::<HashMap<String, LanguageDef>>(&json).unwrap()
        );

        let toml = toml::to_string(&languages).unwrap();
        println!("{toml}");
    }

    #[test]
    fn test_deserialize() {
        let toml = toml! {
            [C]
            message = "使用 $(gcc --version)。"
            exec_type = "binary"
            compile = "gcc {...flags} -lm -Wall -o {out_file} {src_file}"

            [C.options.O2]
            type = "bool"
            target = "flags"
            default = true
            enable = "-O2"

            [C.options.version]
            type = "enum"
            target = "flags"
            default = "C99"

            [C.options.version.variants]
            C99 = "-std=c99"
            C11 = "-std=c11"

            ["C++"]
            exec_type = "binary"
            compile = "g++ {...flags} -o {out_file} {src_file}"

            ["C++".options.O2]
            type = "bool"
            target = "flags"
            default = true
            enable = "-O2"

            ["C++".options.version]
            type = "enum"
            target = "flags"
            default = "C++17"

            ["C++".options.version.variants]
            "C++98" = "-std=c++98"
            "C++11" = "-std=c++11"
            "C++14" = "-std=c++14"
            "C++17" = "-std=c++17"
            "C++20" = "-std=c++20"
        };

        let languages: HashMap<String, LanguageDef> = toml::from_str(&toml.to_string()).unwrap();

        println!("{languages:#?}");
    }

    #[test]
    fn deserialize_config_demo() {
        let mut demo = File::open("../../config-demo/executors.toml").unwrap();
        let mut input = String::new();
        demo.read_to_string(&mut input).unwrap();
        let output = toml::from_str::<IndexMap<String, LanguageDef>>(&input).unwrap();

        println!("{output:#?}");
    }
}
