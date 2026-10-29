// SPDX-License-Identifier: Apache-2.0

//! Rendering of the command templates in `executors.toml`.
//!
//! A language definition stores its build and run commands as templates, with
//! three kinds of placeholder:
//!
//! - `{src_file}`, `{out_file}` and `{tmp_dir}` name the files of a run. They
//!   are shell-quoted, because a run directory may contain spaces.
//! - `{...name}` expands to the fragments of every language option whose
//!   `target` is `name`, separated by spaces. The fragments come from the
//!   option defaults, overridden by the values in the [`LanguageOption`].
//! - anything else is an error, so a typo in a template fails the run instead
//!   of reaching the shell.

use std::collections::HashMap;
use std::path::Path;

use rsjudge_traits::language::config::LanguageDef;
use rsjudge_traits::language::option::LanguageOption;

/// The paths a command template may refer to.
#[derive(Debug, Clone, Copy)]
pub struct RenderPaths<'a> {
    /// The source file of the submission.
    pub src_file: &'a Path,

    /// The file the compiler writes the program to.
    pub out_file: &'a Path,

    /// The directory of the run.
    pub tmp_dir: &'a Path,
}

/// Render `template` for `def`, with `option` overriding the defaults.
///
/// # Errors
///
/// Returns a message when the template names an unknown placeholder, when an
/// override does not fit its option, or when an override names an option the
/// language does not define.
pub fn render(
    template: &str,
    def: &LanguageDef,
    option: &LanguageOption,
    paths: &RenderPaths<'_>,
) -> Result<String, String> {
    let fragments = collect_fragments(def, option)?;

    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find('{') {
        rendered.push_str(&rest[..start]);

        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            return Err(format!("unterminated placeholder in `{template}`"));
        };

        let name = &after[..end];
        rendered.push_str(&resolve(name, paths, &fragments)?);

        rest = &after[end + 1..];
    }
    rendered.push_str(rest);

    Ok(rendered)
}

/// Resolve one placeholder.
fn resolve(
    name: &str,
    paths: &RenderPaths<'_>,
    fragments: &HashMap<String, Vec<String>>,
) -> Result<String, String> {
    if let Some(group) = name.strip_prefix("...") {
        return Ok(fragments.get(group).cloned().unwrap_or_default().join(" "));
    }

    let path = match name {
        "src_file" => paths.src_file,
        "out_file" => paths.out_file,
        "tmp_dir" => paths.tmp_dir,
        _ => return Err(format!("unknown placeholder `{{{name}}}`")),
    };

    Ok(shell_words::quote(path.to_string_lossy().as_ref()).into_owned())
}

/// Group the fragment every option contributes under the option's target.
fn collect_fragments(
    def: &LanguageDef,
    option: &LanguageOption,
) -> Result<HashMap<String, Vec<String>>, String> {
    let mut fragments: HashMap<String, Vec<String>> = HashMap::new();

    for (name, config) in def.options() {
        let fragment = config
            .resolve(option.config.get(name))
            .map_err(|message| format!("option `{name}`: {message}"))?;

        if let Some(fragment) = fragment {
            fragments
                .entry(config.target().to_owned())
                .or_default()
                .push(fragment);
        }
    }

    for name in option.config.keys() {
        if !def.options().contains_key(name) {
            return Err(format!(
                "option `{name}` is not defined by language `{}`",
                option.name
            ));
        }
    }

    Ok(fragments)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rsjudge_traits::language::config::LanguageDef;
    use rsjudge_traits::language::option::{ConfigValue, LanguageOption};

    use super::{RenderPaths, render};

    fn language() -> LanguageDef {
        toml::from_str(
            r#"
exec_type = "binary"
compile = "gcc {...flags} -o {out_file} {src_file}"

[options.O2]
type = "bool"
target = "flags"
default = true
enable = "-O2"

[options.version]
type = "enum"
target = "flags"
default = "C99"

[options.version.variants]
C99 = "-std=c99"
C11 = "-std=c11"
"#,
        )
        .expect("invalid language definition")
    }

    fn paths() -> (PathBuf, PathBuf, PathBuf) {
        (
            PathBuf::from("/tmp/run/src file.c"),
            PathBuf::from("/tmp/run/program"),
            PathBuf::from("/tmp/run"),
        )
    }

    fn option(config: &[(&str, ConfigValue)]) -> LanguageOption {
        LanguageOption {
            name: "C".to_owned(),
            config: config
                .iter()
                .map(|(name, value)| ((*name).to_owned(), value.clone()))
                .collect(),
        }
    }

    #[test]
    fn renders_defaults_and_quotes_paths() {
        let (src, out, tmp) = paths();
        let paths = RenderPaths {
            src_file: &src,
            out_file: &out,
            tmp_dir: &tmp,
        };

        let rendered = render(
            "gcc {...flags} -o {out_file} {src_file}",
            &language(),
            &option(&[]),
            &paths,
        )
        .expect("failed to render");

        assert_eq!(
            rendered,
            "gcc -O2 -std=c99 -o /tmp/run/program '/tmp/run/src file.c'"
        );
    }

    #[test]
    fn overrides_disable_and_switch_options() {
        let (src, out, tmp) = paths();
        let paths = RenderPaths {
            src_file: &src,
            out_file: &out,
            tmp_dir: &tmp,
        };
        let option = option(&[
            ("O2", ConfigValue::Bool(false)),
            ("version", ConfigValue::Enum("C11".to_owned())),
        ]);

        let rendered =
            render("{...flags}", &language(), &option, &paths).expect("failed to render");

        assert_eq!(rendered, "-std=c11");
    }

    #[test]
    fn rejects_unknown_placeholders() {
        let (src, out, tmp) = paths();
        let paths = RenderPaths {
            src_file: &src,
            out_file: &out,
            tmp_dir: &tmp,
        };

        let error = render("{flags}", &language(), &option(&[]), &paths).unwrap_err();

        assert!(error.contains("unknown placeholder"));
    }

    #[test]
    fn rejects_unknown_options() {
        let (src, out, tmp) = paths();
        let paths = RenderPaths {
            src_file: &src,
            out_file: &out,
            tmp_dir: &tmp,
        };
        let option = option(&[("sanitize", ConfigValue::Bool(true))]);

        let error = render("{...flags}", &language(), &option, &paths).unwrap_err();

        assert!(error.contains("not defined"));
    }

    #[test]
    fn rejects_a_mismatched_option_value() {
        let (src, out, tmp) = paths();
        let paths = RenderPaths {
            src_file: &src,
            out_file: &out,
            tmp_dir: &tmp,
        };
        let option = option(&[("O2", ConfigValue::Enum("on".to_owned()))]);

        let error = render("{...flags}", &language(), &option, &paths).unwrap_err();

        assert!(error.contains("expected a boolean"));
    }
}
