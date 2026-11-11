//! Print an AU's parameters and their default (freshly-loaded) values.
//!
//! AU hosting is macOS-only; this example is a no-op elsewhere.

#[cfg(all(target_os = "macos", feature = "au"))]
fn run() {
    let pat = std::env::args()
        .nth(1)
        .expect("usage: dump_params <au-substring>")
        .to_lowercase();
    let found = headroom::host::au::scan()
        .into_iter()
        .find(|a| a.name.to_lowercase().contains(&pat))
        .expect("no AU matches");
    let (loaded, _ins) = headroom::host::au::load(&found, 48_000.0, 512).expect("load");
    println!(
        "{} — {} parameters (fresh-load defaults):",
        found.name,
        loaded.params().len()
    );
    for p in loaded.params() {
        println!("  {:<40} = {}", p.name, p.display_value());
    }
}

#[cfg(not(all(target_os = "macos", feature = "au")))]
fn run() {
    eprintln!("au_params needs macOS and the `au` feature (Audio Unit hosting).");
}

fn main() {
    run();
}
