// SPDX-License-Identifier: Apache-2.0

use std::{thread::sleep, time::Duration};

fn main() {
    println!("Trying to sleep for 10s.");
    sleep(Duration::from_secs(10));
}
