fn main() {
    // Embedded manifests keep Windows' installer-detection heuristics out of
    // the picture and make the elevation requirement explicit.
    for (bin, level) in [
        ("cleanup-fixture", "asInvoker"),
        ("cleanup-disposable-app", "asInvoker"),
        ("cleanup-disposable-app-admin", "requireAdministrator"),
    ] {
        println!("cargo:rustc-link-arg-bin={bin}=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bin={bin}=/MANIFESTUAC:level='{level}'");
    }
}
