// SPDX-License-Identifier: Apache-2.0

fn main() {
    println!("cargo:rustc-check-cfg=cfg(feature, values(\"setgroups\"))");

    if rustversion::cfg!(nightly) {
        println!("cargo:rustc-cfg=feature=\"setgroups\"");
    }
}
