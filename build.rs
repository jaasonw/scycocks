use embed_manifest::manifest::{ActiveCodePage, DpiAwareness};
use embed_manifest::{embed_manifest, new_manifest};

fn main() {
    // Common Controls v6 gives native themed controls; without it Windows draws classic ones.
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        embed_manifest(
            new_manifest("Scycocks")
                .active_code_page(ActiveCodePage::Utf8)
                .dpi_awareness(DpiAwareness::PerMonitorV2),
        )
        .expect("embed Windows manifest");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
