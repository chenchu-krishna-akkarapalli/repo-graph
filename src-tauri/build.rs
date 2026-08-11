fn main() {
    // Ensure target resource placeholders exist so tauri_build does not fail
    // if mcp_server has not been compiled yet on a clean checkout.
    for profile in &["release", "debug"] {
        let dir = std::path::Path::new("target").join(profile);
        let exe = dir.join("mcp_server.exe");
        if !exe.exists() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(&exe, b"");
        }
        let bin = dir.join("mcp_server");
        if !bin.exists() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(&bin, b"");
        }
    }

    tauri_build::build();
}
