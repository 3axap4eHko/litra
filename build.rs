fn main() {
    slint_build::compile("src/ui.slint").expect("Failed to compile ui.slint");

    #[cfg(windows)]
    if std::path::Path::new("assets/icon.ico").exists() {
        embed_resource::compile("resources.rc", embed_resource::NONE);
    }
}
