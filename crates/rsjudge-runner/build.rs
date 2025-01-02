// SPDX-License-Identifier: Apache-2.0

#[rustversion::nightly]
fn main() {
    println!("cargo:rustc-cfg=setgroups");
}

#[rustversion::not(nightly)]
fn main() {}
