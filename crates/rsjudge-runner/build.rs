#[rustversion::nightly]
fn main() {
    // tokio::process::Command has neither groups() nor as_std_mut(),
    // so we can't use it by now.
    #[cfg(any())]
    println!("cargo:rustc-cfg=setgroups");
}

#[rustversion::not(nightly)]
fn main() {}
