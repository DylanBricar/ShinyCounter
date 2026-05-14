//! Build-time icon generation.
//!
//! Renders the Pokéball procedurally at several resolutions and writes:
//! - `$OUT_DIR/icon.ico` — multi-resolution ICO, embedded into the Windows
//!   executable below.
//! - `$OUT_DIR/icon-1024.png` — high-res PNG used by the macOS release
//!   workflow (`sips` + `iconutil`) to build a proper `.icns` for the `.app`
//!   bundle.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    // ---- ICO: a few standard Windows icon sizes -----------------------------
    let ico_sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
    for &size in &ico_sizes {
        let rgba = render_pokeball(size);
        let image = ico::IconImage::from_rgba_data(size, size, rgba);
        icon_dir.add_entry(
            ico::IconDirEntry::encode(&image).expect("encode ICO sub-image"),
        );
    }
    let ico_path = out_dir.join("icon.ico");
    let mut ico_file = fs::File::create(&ico_path).expect("create icon.ico");
    icon_dir
        .write(&mut ico_file)
        .expect("write multi-resolution ICO");

    // ---- High-res PNG: source for the macOS iconset -------------------------
    let png_size = 1024u32;
    let png_rgba = render_pokeball(png_size);
    let png_path = out_dir.join("icon-1024.png");
    image::save_buffer(
        &png_path,
        &png_rgba,
        png_size,
        png_size,
        image::ColorType::Rgba8,
    )
    .expect("write icon PNG");

    // The release workflow looks these paths up via `cargo metadata`-style
    // glob (`target/<target>/release/build/shiny_counter-*/out/...`). We
    // re-export them as build env so a build-time consumer could also use them.
    println!("cargo:rustc-env=SHINY_ICON_ICO={}", ico_path.display());
    println!("cargo:rustc-env=SHINY_ICON_PNG={}", png_path.display());

    // ---- Windows: embed the ICO into the produced .exe ----------------------
    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        embed_windows_icon(&ico_path);
    }
}

#[cfg(windows)]
fn embed_windows_icon(ico_path: &std::path::Path) {
    let mut res = winresource::WindowsResource::new();
    res.set_icon(
        ico_path
            .to_str()
            .expect("ICO path must be valid UTF-8 on Windows"),
    );
    // Cosmetic metadata shown in File Explorer → Properties → Details.
    res.set("ProductName", "Shiny Counter");
    res.set("FileDescription", "Shiny Counter");
    res.set("CompanyName", "DylanBricar");
    res.set("LegalCopyright", "MIT licensed");
    res.compile().expect("winresource compile");
}

#[cfg(not(windows))]
fn embed_windows_icon(_ico_path: &std::path::Path) {
    // Non-Windows hosts: nothing to embed. Cross-compiling to Windows from
    // Linux would need an mingw resource compiler — not currently used by
    // our CI matrix (windows-latest is always the host for the Windows
    // target).
}

/// Procedural Pokéball: red top, white bottom, black equator + central button
/// ring. Anti-aliased at the requested resolution, transparent corners.
fn render_pokeball(size: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    let f = size as f32;
    let cx = f / 2.0;
    let cy = f / 2.0;
    let r_outer = f / 2.0 - 0.5;
    let r_button_outer = f * 0.14;
    let r_button_inner = f * 0.07;
    let band_half_h = (f * 0.05).max(2.0);

    let red = [232u8, 65, 65, 255];
    let white = [248u8, 248, 248, 255];
    let black = [22u8, 22, 22, 255];
    let transparent = [0u8; 4];

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let r = (dx * dx + dy * dy).sqrt();

            let p = if r > r_outer + 1.0 {
                transparent
            } else if r > r_outer - 1.5 {
                black
            } else if r <= r_button_outer + 0.5 {
                if r <= r_button_inner {
                    white
                } else if r <= r_button_inner + 1.0 {
                    black
                } else if r <= r_button_outer {
                    white
                } else {
                    black
                }
            } else if dy.abs() <= band_half_h {
                black
            } else if dy < 0.0 {
                red
            } else {
                white
            };
            out.extend_from_slice(&p);
        }
    }
    out
}
