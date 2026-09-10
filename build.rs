use std::env;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if !matches!(target_os.as_str(), "linux" | "macos" | "darwin") {
        return;
    }

    let Some(encoded_paths) = env::var_os("DEP_QUEST_RUNTIME_LIBRARY_PATHS") else {
        return;
    };
    for directory in env::split_paths(&encoded_paths).filter(|path| !path.as_os_str().is_empty()) {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", directory.display());
    }
}
