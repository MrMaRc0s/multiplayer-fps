use std::env;
use std::path::Path;

fn main() {
    // Vendored SDL2 + transitive deps live in `.lib/` (gitignored, extracted
    // from Ubuntu debs via `apt-get download`). Emit link search + rpath so
    // both `cc` (GNU ld) and `rust-lld` (`-fuse-ld=lld`) can find `-lSDL2`
    // even when `RUSTFLAGS` env overrides `.cargo/config.toml` rustflags
    // (Cargo gives `RUSTFLAGS` precedence over config-file rustflags).
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let lib_dir = format!("{manifest_dir}/.lib/usr/lib/x86_64-linux-gnu");
    let pulse_dir = format!("{lib_dir}/pulseaudio");

    for dir in [&lib_dir, &pulse_dir] {
        if Path::new(dir).is_dir() {
            println!("cargo:rustc-link-search=native={dir}");
            // Use RPATH (not RUNPATH) so transitive SDL2 deps (libpulse,
            // libsamplerate, libXss, libdecor, ...) are also found.
            // --disable-new-dtags forces old RPATH tag which is transitive,
            // whereas default RUNPATH is not searched for indirect deps.
            println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags,-rpath,{dir}");
        }
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.lib/usr/lib/x86_64-linux-gnu/");
}
