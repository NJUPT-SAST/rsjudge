// SPDX-License-Identifier: Apache-2.0

use capctl::CapState;

fn main() {
    eprintln!("Start normal binary");
    dbg!(CapState::get_current().unwrap());
    eprintln!("End normal binary");
}
