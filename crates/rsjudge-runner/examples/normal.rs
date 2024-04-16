// SPDX-License-Identifier: Apache-2.0

use capctl::FullCapState;

fn main() {
    eprintln!("Start normal binary");
    dbg!(FullCapState::get_current().unwrap());
    eprintln!("End normal binary");
}
