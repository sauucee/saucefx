//! Links the Rust host against the C++ engine libraries built by the
//! Makefile (`make -C src` produces src/build/libdfxdsp_linux.a and
//! src/build/libfxpw.a). fxpw embeds the PipeWire pipeline, so the final
//! binary also needs the PipeWire/PulseAudio shared libraries, the C++
//! runtime and libm — the same set the Makefile links for saucefxrt.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let app_root = manifest_dir
        .join("..")
        .join("..")
        .join("src")
        .canonicalize()
        .expect("locate the SauceFX src directory");
    let dsp = app_root.join("build").join("libdfxdsp_linux.a");
    let fxpw = app_root.join("build").join("libfxpw.a");
    if !dsp.is_file() || !fxpw.is_file() {
        panic!(
            "missing {} or {} — build the C++ engine first: make -C src",
            dsp.display(),
            fxpw.display()
        );
    }
    println!("cargo:rerun-if-changed={}", dsp.display());
    println!("cargo:rerun-if-changed={}", fxpw.display());
    println!(
        "cargo:rustc-link-search=native={}",
        app_root.join("build").display()
    );

    // fxpw depends on the DSP, so fxpw links first.
    println!("cargo:rustc-link-lib=static=fxpw");
    println!("cargo:rustc-link-lib=static=dfxdsp_linux");
    println!("cargo:rustc-link-lib=dylib=stdc++");
    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=m");

    for flag in pkg_config_libs() {
        println!("cargo:rustc-link-lib=dylib={flag}");
    }
    // libpulse.so (the devel symlink) may be absent when only the runtime
    // package is installed; the GNU linker accepts the exact versioned file
    // instead of the unresolved -lpulse.
    // The DSP engine's static objects are non-PIC, and the toolchain's
    // bundled lld refuses them in a PIE. The C++ JUCE app links the same
    // objects with GNU ld, which relaxes them; for the Rust host the
    // executable is built without PIE. (Packaging builds that ship the
    // engine compiled with -fPIC can drop this.)
    println!("cargo:rustc-link-arg=-no-pie");
    if !Path::new("/usr/lib64/libpulse.so").exists() {
        println!("cargo:rustc-link-arg=-Wl,-l:libpulse.so.0");
    }
}

/// The library names pkg-config reports for the PipeWire client set,
/// falling back to the Fedora package names when pkg-config is absent.
fn pkg_config_libs() -> Vec<String> {
    let output = Command::new("pkg-config")
        .args(["--libs", "libpipewire-0.3"])
        .output();
    match output {
        Ok(result) if result.status.success() => String::from_utf8_lossy(&result.stdout)
            .split_whitespace()
            .filter_map(|flag| flag.strip_prefix("-l").map(str::to_string))
            .collect(),
        _ => vec!["pipewire-0.3".to_string()],
    }
}
