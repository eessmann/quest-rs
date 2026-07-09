use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_QUEST_ROOT: &str = "/Users/erich/Projects/opt/quest";
const MANIFEST_PATH: &str = "quest-sys/generated/api_coverage.json";
const GENERATED_NAMES_PATH: &str = "quest-sys/generated/generated_names.txt";

const CORE_GENERATED: &[&str] = &[
    "applyCompMatr",
    "applyHadamard",
    "applyPauliX",
    "applyPauliY",
    "applyPauliZ",
    "applyQubitMeasurement",
    "applyQubitMeasurementAndGetProb",
    "applyTrotterizedUnitaryTimeEvolution",
    "calcTotalProb",
    "createCompMatr",
    "createDensityQureg",
    "createInlinePauliStrSum",
    "createKrausMap",
    "createQureg",
    "createSuperOp",
    "finalizeQuESTEnv",
    "getQuESTEnv",
    "getQuESTEnvironmentString",
    "getQuregAmp",
    "getQuregAmps",
    "initArbitraryPureState",
    "initCustomQuESTEnv",
    "initPlusState",
    "initQuESTEnv",
    "initZeroState",
    "isQuESTEnvInit",
    "leftapplyCompMatr",
    "mixDephasing",
    "rightapplyCompMatr",
    "setCompMatr",
    "setQuESTInputErrorHandler",
    "syncQuESTEnv",
];

const COVERED_BY_RAII: &[(&str, &str)] = &[
    ("destroyCompMatr", "CompMatr Drop"),
    ("destroyDiagMatr", "DiagMatr Drop"),
    ("destroyFullStateDiagMatr", "FullStateDiagMatr Drop"),
    ("destroyKrausMap", "KrausMap Drop"),
    ("destroyPauliStrSum", "PauliStrSum Drop"),
    ("destroyQureg", "Qureg Drop"),
    ("destroySuperOp", "SuperOp Drop"),
];

#[derive(Debug)]
struct ApiItem {
    name: String,
    header: String,
    signature: String,
    status: String,
    reason: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("generate-quest-bindings") => generate(args.any(|arg| arg == "--check")),
        Some(command) => Err(format!("unknown xtask command: {command}").into()),
        None => {
            eprintln!("usage: cargo run -p xtask -- generate-quest-bindings [--check]");
            Ok(())
        }
    }
}

fn generate(check: bool) -> Result<(), Box<dyn std::error::Error>> {
    let workspace = env::current_dir()?;
    let quest_root = find_quest_root()?;
    let include_dir = quest_root.join("include");
    let quest_h = include_dir.join("quest.h");

    if !quest_h.is_file() {
        return Err(format!("missing QuEST umbrella header at {}", quest_h.display()).into());
    }

    let ast_names = parse_clang_ast_function_names(&quest_h, &include_dir)?;
    let generated_names = load_generated_names(&workspace)?;
    let mut items = parse_header_api(&quest_root)?;
    classify_items(&mut items, &ast_names, &generated_names);

    let manifest = render_manifest(&quest_root, &items)?;
    let manifest_path = workspace.join(MANIFEST_PATH);

    if check {
        let existing = fs::read_to_string(&manifest_path)?;
        if existing != manifest {
            return Err(format!(
                "{} is stale; rerun xtask generate-quest-bindings",
                MANIFEST_PATH
            )
            .into());
        }
        return Ok(());
    }

    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&manifest_path, manifest)?;
    println!("wrote {}", manifest_path.display());
    Ok(())
}

fn load_generated_names(workspace: &Path) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let mut names = CORE_GENERATED
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<BTreeSet<_>>();

    let generated_path = workspace.join(GENERATED_NAMES_PATH);
    let text = fs::read_to_string(&generated_path)?;
    for line in text.lines() {
        let name = line.trim();
        if !name.is_empty() && !name.starts_with('#') {
            names.insert(name.to_owned());
        }
    }

    Ok(names)
}

fn find_quest_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    for var in ["QUEST_DIR", "QUEST_ROOT"] {
        if let Ok(value) = env::var(var)
            && let Some(root) = normalize_quest_root(Path::new(&value))
        {
            return Ok(root);
        }
    }

    normalize_quest_root(Path::new(DEFAULT_QUEST_ROOT))
        .ok_or_else(|| format!("could not locate QuEST root at {DEFAULT_QUEST_ROOT}").into())
}

fn normalize_quest_root(candidate: &Path) -> Option<PathBuf> {
    let mut current = candidate;
    loop {
        if current.join("include/quest.h").is_file() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

fn parse_clang_ast_function_names(
    quest_h: &Path,
    include_dir: &Path,
) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let clang = env::var("CXX")
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            let homebrew = PathBuf::from("/opt/homebrew/opt/llvm/bin/clang++");
            homebrew.is_file().then_some(homebrew)
        })
        .unwrap_or_else(|| PathBuf::from("clang++"));

    let output = Command::new(&clang)
        .arg("-std=c++20")
        .arg("-x")
        .arg("c++")
        .arg("-I")
        .arg(include_dir)
        .arg("-Xclang")
        .arg("-ast-dump=json")
        .arg("-fsyntax-only")
        .arg(quest_h)
        .output()?;

    if !output.status.success() {
        return Err(format!(
            "{} failed to parse {}:\n{}",
            clang.display(),
            quest_h.display(),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let json = String::from_utf8(output.stdout)?;
    Ok(extract_function_names_from_ast_json(&json))
}

fn extract_function_names_from_ast_json(json: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut recent_function_decl = false;

    for line in json.lines() {
        let line = line.trim();
        if line == r#""kind": "FunctionDecl","# {
            recent_function_decl = true;
            continue;
        }

        if recent_function_decl && line.starts_with(r#""name": ""#) {
            if let Some(name) = line
                .trim_start_matches(r#""name": ""#)
                .trim_end_matches(',')
                .strip_suffix('"')
                && !name.starts_with('_')
            {
                names.insert(name.to_owned());
            }
            recent_function_decl = false;
        }
    }

    names
}

fn parse_header_api(quest_root: &Path) -> Result<Vec<ApiItem>, Box<dyn std::error::Error>> {
    let include_root = quest_root.join("include");
    let quest_include = include_root.join("quest/include");
    let mut headers = Vec::new();
    headers.push(include_root.join("quest.h"));

    for entry in fs::read_dir(&quest_include)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "h")
            && path.file_name().is_none_or(|name| name != "deprecated.h")
        {
            headers.push(path);
        }
    }
    headers.sort();

    let mut items_by_key = BTreeMap::new();
    for header in headers {
        let rel_header = header
            .strip_prefix(&include_root)
            .unwrap_or(&header)
            .display()
            .to_string();
        let text = fs::read_to_string(&header)?;
        for signature in extract_function_signatures(&text) {
            if let Some(name) = extract_function_name(&signature) {
                let key = format!("{name}\n{signature}");
                items_by_key.entry(key).or_insert_with(|| ApiItem {
                    name,
                    header: rel_header.clone(),
                    signature,
                    status: "unclassified".to_owned(),
                    reason: String::new(),
                });
            }
        }
    }

    Ok(items_by_key.into_values().collect())
}

fn extract_function_signatures(text: &str) -> Vec<String> {
    let mut signatures = Vec::new();
    let mut in_block_comment = false;
    let mut pending = String::new();

    for raw_line in text.lines() {
        let line = strip_comments(raw_line, &mut in_block_comment);
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("using namespace")
        {
            continue;
        }

        pending.push(' ');
        pending.push_str(trimmed);

        if trimmed.ends_with(';') {
            let signature = pending.split_whitespace().collect::<Vec<_>>().join(" ");
            pending.clear();

            if signature.contains('(')
                && !signature.contains("typedef")
                && !signature.contains("using ")
                && !signature.contains("operator ")
                && !signature.contains("static_assert")
            {
                signatures.push(signature.trim_end_matches(';').to_owned());
            }
        }
    }

    signatures
}

fn strip_comments(line: &str, in_block_comment: &mut bool) -> String {
    let mut out = String::new();
    let mut rest = line;

    loop {
        if *in_block_comment {
            if let Some(end) = rest.find("*/") {
                rest = &rest[end + 2..];
                *in_block_comment = false;
            } else {
                return out;
            }
        }

        let block_start = rest.find("/*");
        let line_start = rest.find("//");
        match (block_start, line_start) {
            (Some(block), Some(line)) if line < block => {
                out.push_str(&rest[..line]);
                return out;
            }
            (Some(block), _) => {
                out.push_str(&rest[..block]);
                rest = &rest[block + 2..];
                *in_block_comment = true;
            }
            (None, Some(line)) => {
                out.push_str(&rest[..line]);
                return out;
            }
            (None, None) => {
                out.push_str(rest);
                return out;
            }
        }
    }
}

fn extract_function_name(signature: &str) -> Option<String> {
    let paren = signature.find('(')?;
    let before = signature[..paren].trim_end();
    let name = before
        .rsplit(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
        .find(|part| !part.is_empty())?;

    if name.starts_with('_') || name.chars().next()?.is_ascii_digit() {
        None
    } else {
        Some(name.to_owned())
    }
}

fn classify_items(
    items: &mut [ApiItem],
    ast_names: &BTreeSet<String>,
    generated_names: &BTreeSet<String>,
) {
    for item in items {
        if generated_names.contains(&item.name) {
            item.status = "generated".to_owned();
            item.reason = if CORE_GENERATED.contains(&item.name.as_str()) {
                "emitted in hand-written core cxx bridge".to_owned()
            } else {
                "emitted in checked-in generated cxx bridge".to_owned()
            };
        } else if let Some((_, owner)) = COVERED_BY_RAII
            .iter()
            .find(|(name, _)| *name == item.name.as_str())
        {
            item.status = "covered-by-raii".to_owned();
            item.reason = format!("covered by Rust-owned {owner}");
        } else if item.signature.contains("MPI_Comm") {
            item.status = "gated".to_owned();
            item.reason =
                "MPI_Comm requires a dedicated mpi Cargo feature and type strategy".to_owned();
        } else if has_callback_or_opaque_pointer(&item.signature) {
            item.status = "gated".to_owned();
            item.reason =
                "signature requires a dedicated safe adapter for callbacks or opaque pointers"
                    .to_owned();
        } else if item.header.ends_with("experimental.h") {
            item.status = "gated".to_owned();
            item.reason =
                "experimental API requires explicit cxx-safe wrapper before enabling".to_owned();
        } else if !ast_names.contains(&item.name) {
            item.status = "gated".to_owned();
            item.reason = "not present in clang FunctionDecl set for this QuEST config".to_owned();
        } else {
            item.status = "gated".to_owned();
            item.reason =
                "requires a dedicated safe wrapper before exposing through cxx".to_owned();
        }
    }
}

fn has_callback_or_opaque_pointer(signature: &str) -> bool {
    signature.contains("void*")
        || signature.contains("void *")
        || signature.contains("Func")
        || signature.contains("(*)")
        || signature.contains("(*")
}

fn render_manifest(
    quest_root: &Path,
    items: &[ApiItem],
) -> Result<String, Box<dyn std::error::Error>> {
    let config = fs::read_to_string(quest_root.join("include/quest/include/config.h"))?;
    let version =
        macro_value(&config, "QUEST_VERSION_STRING").unwrap_or_else(|| "unknown".to_owned());
    let deprecated = macro_value(&config, "QUEST_INCLUDE_DEPRECATED_FUNCTIONS")
        .unwrap_or_else(|| "unknown".to_owned());

    let mut counts = BTreeMap::<&str, usize>::new();
    for item in items {
        *counts.entry(&item.status).or_default() += 1;
    }

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"generator\": \"xtask generate-quest-bindings\",\n");
    out.push_str(&format!(
        "  \"quest_version\": {},\n",
        json_string(&version)
    ));
    out.push_str(&format!(
        "  \"quest_root\": {},\n",
        json_string(&quest_root.display().to_string())
    ));
    out.push_str(&format!(
        "  \"deprecated_apis_included\": {},\n",
        json_string(&deprecated)
    ));
    out.push_str("  \"counts\": {\n");
    for (index, (status, count)) in counts.iter().enumerate() {
        let comma = if index + 1 == counts.len() { "" } else { "," };
        out.push_str(&format!(
            "    {}: {}{}\n",
            json_string(status),
            count,
            comma
        ));
    }
    out.push_str("  },\n");
    out.push_str("  \"items\": [\n");
    for (index, item) in items.iter().enumerate() {
        let comma = if index + 1 == items.len() { "" } else { "," };
        out.push_str("    {\n");
        out.push_str(&format!("      \"name\": {},\n", json_string(&item.name)));
        out.push_str(&format!(
            "      \"header\": {},\n",
            json_string(&item.header)
        ));
        out.push_str(&format!(
            "      \"signature\": {},\n",
            json_string(&item.signature)
        ));
        out.push_str(&format!(
            "      \"status\": {},\n",
            json_string(&item.status)
        ));
        out.push_str(&format!(
            "      \"reason\": {}\n",
            json_string(&item.reason)
        ));
        out.push_str(&format!("    }}{}\n", comma));
    }
    out.push_str("  ]\n");
    out.push_str("}\n");
    Ok(out)
}

fn macro_value(config: &str, name: &str) -> Option<String> {
    config.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix("#define ")?;
        let value = rest.strip_prefix(name)?.trim();
        Some(value.trim_matches('"').to_owned())
    })
}

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}
