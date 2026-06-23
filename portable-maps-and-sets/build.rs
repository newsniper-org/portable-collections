use rustc_version::{version_meta, Channel};

fn main() {
    // Set custom cfg flags depending on the release channel
    let channel = match version_meta().unwrap().channel {
        Channel::Stable => "stable",
        Channel::Beta => "beta",
        Channel::Nightly => "nightly",
        Channel::Dev => "dev"
    };
    println!("cargo::rustc-cfg=toolchain_channel=\"{channel}\"");
}