use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[allow(dead_code)]
#[path = "../build-support/quest.rs"]
mod quest_build_support;

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "quest-sys-build-support-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(path.join("include"))
            .expect("temporary QuEST include directory should be created");
        fs::write(path.join("include/quest.h"), "")
            .expect("temporary QuEST umbrella header should be created");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn normalizes_roots_from_lib_and_lib64_package_directories() {
    let quest = TestDirectory::new();

    for library_directory in ["lib", "lib64"] {
        let package_directory = quest.path().join(library_directory).join("cmake/QuEST");
        fs::create_dir_all(&package_directory)
            .expect("temporary CMake package directory should be created");

        assert_eq!(
            quest_build_support::normalize_quest_root(&package_directory),
            Some(quest.path().to_path_buf())
        );
    }
}

#[test]
fn explicit_quest_root_precedes_and_deduplicates_cmake_prefix_paths() {
    let explicit = PathBuf::from("/opt/quest");
    let fallback = PathBuf::from("/opt/other");
    let encoded = env::join_paths([fallback.as_path(), explicit.as_path()])
        .expect("test paths should be encodable");

    assert_eq!(
        quest_build_support::collect_prefix_paths(Some(explicit.clone()), Some(&encoded)),
        vec![explicit, fallback]
    );
}

#[test]
fn linux_runtime_paths_include_shared_libraries_but_not_static_archives() {
    let libraries = vec![
        "/opt/quest/lib64/libQuEST.so.4".to_owned(),
        "/opt/quest/lib64/libQuEST.a".to_owned(),
        "/opt/vendor/lib/libdependency.so".to_owned(),
        "gomp".to_owned(),
    ];

    assert_eq!(
        quest_build_support::runtime_library_directories(
            "linux",
            Some("/opt/quest/lib64/libQuEST.so.4.3.0"),
            &libraries,
        ),
        vec![
            PathBuf::from("/opt/quest/lib64"),
            PathBuf::from("/opt/vendor/lib"),
        ]
    );
}

#[test]
fn macos_runtime_paths_include_dylibs_but_not_static_archives() {
    let libraries = vec![
        "/opt/quest/lib/libQuEST.4.dylib".to_owned(),
        "/opt/quest/lib/libQuEST.a".to_owned(),
    ];

    assert_eq!(
        quest_build_support::runtime_library_directories("macos", None, &libraries),
        vec![PathBuf::from("/opt/quest/lib")]
    );
}

#[test]
fn runtime_path_metadata_round_trips() {
    let paths = vec![
        PathBuf::from("/opt/quest shared/lib"),
        PathBuf::from("/opt/vendor/lib"),
    ];
    let encoded = quest_build_support::encode_runtime_library_paths(&paths)
        .expect("runtime paths should be encodable");

    assert_eq!(env::split_paths(&encoded).collect::<Vec<_>>(), paths);
    assert_eq!(
        quest_build_support::rpath_link_arg(Path::new("/opt/quest shared/lib")),
        "-Wl,-rpath,/opt/quest shared/lib"
    );
}
