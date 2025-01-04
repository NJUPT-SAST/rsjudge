// SPDX-License-Identifier: Apache-2.0

fn main() {
    if rustversion::cfg!(since(1.80)) {
        println!("cargo:rustc-check-cfg=cfg(feature, values(\"setgroups\"))");
    }
    if rustversion::cfg!(nightly) {
        println!("cargo:rustc-cfg=feature=\"setgroups\"");
    }
}
