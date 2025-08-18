// SPDX-License-Identifier: Apache-2.0

use std::io::Write as _;

use chrono::{Local, SubsecRound};
use env_logger::fmt::style::{AnsiColor, Style};
use env_logger::{Builder, Env};

pub(crate) fn setup_logger() {
    Builder::from_env(
        Env::new()
            .filter_or("RSJUDGE_LOG", "info")
            .write_style("RSJUDGE_LOG_STYLE"),
    )
    .format(|f, record| {
        const SUBTLE: Style = AnsiColor::BrightBlack.on_default();
        let level = record.level();
        let level_style = f.default_level_style(level);
        writeln!(
            f,
            "{SUBTLE}[{SUBTLE:#}{} {level_style}{level:<5}{level_style:#} {}{SUBTLE}]{SUBTLE:#} {}",
            Local::now().trunc_subsecs(3),
            record.target(),
            record.args()
        )
    })
    .init();
}
