use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn collect_notices(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if ty.is_dir() && !matches!(name.as_str(), ".git" | "target" | "node_modules") {
            collect_notices(root, &entry.path(), out)?;
        }
        if ty.is_file() {
            let upper = name.to_ascii_uppercase();
            if [
                "LICENSE",
                "LICENCE",
                "NOTICE",
                "COPYING",
                "THIRDPARTYNOTICE",
            ]
            .iter()
            .any(|prefix| upper.starts_with(prefix))
            {
                out.push((
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    fs::read(entry.path()).map_err(|e| e.to_string())?,
                ));
            }
        }
    }
    Ok(())
}
fn safe_label(s: &str) -> Result<&str, String> {
    if s.is_empty()
        || !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-+".contains(c))
    {
        Err(format!("unsafe package label {s:?}"))
    } else {
        Ok(s)
    }
}
pub fn run(bundle: &Path, manifest: &Path) -> Result<(), String> {
    if !bundle.is_dir() {
        return Err(format!("bundle directory missing: {}", bundle.display()));
    }
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--locked",
            "--filter-platform",
            "x86_64-pc-windows-msvc",
            "--features",
            "cli,directml",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .map_err(|e| format!("cargo metadata unavailable: {e}"))?;
    if !output.status.success() {
        return Err(format!("cargo metadata --locked failed; fetch pinned dependencies first, do not regenerate the release lockfile: {}",String::from_utf8_lossy(&output.stderr)));
    }
    let metadata: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let packages: BTreeMap<&str, &Value> = metadata["packages"]
        .as_array()
        .ok_or("metadata packages missing")?
        .iter()
        .map(|p| Ok((p["id"].as_str().ok_or("package ID missing")?, p)))
        .collect::<Result<_, String>>()?;
    let nodes: BTreeMap<&str, &Value> = metadata["resolve"]["nodes"]
        .as_array()
        .ok_or("metadata resolve missing")?
        .iter()
        .map(|n| Ok((n["id"].as_str().ok_or("node ID missing")?, n)))
        .collect::<Result<_, String>>()?;
    let roots: Vec<_> = packages
        .iter()
        .filter(|(_, p)| p["name"] == "pubg-ocr-engine")
        .map(|(id, _)| *id)
        .collect();
    if roots.len() != 1 {
        return Err("metadata must contain exactly one pubg-ocr-engine package".into());
    }
    let mut stack = vec![(roots[0], true)];
    let mut seen = BTreeSet::new();
    let mut scopes: BTreeMap<&str, bool> = BTreeMap::new();
    while let Some((id, shipped)) = stack.pop() {
        if !seen.insert((id, shipped)) {
            continue;
        }
        scopes
            .entry(id)
            .and_modify(|s| *s |= shipped)
            .or_insert(shipped);
        for dep in nodes
            .get(id)
            .ok_or_else(|| format!("missing resolved node {id}"))?["deps"]
            .as_array()
            .ok_or("node deps missing")?
        {
            let next = dep["pkg"].as_str().ok_or("dependency ID missing")?;
            let proc_macro = packages.get(next).ok_or("dependency package missing")?["targets"]
                .as_array()
                .ok_or("targets missing")?
                .iter()
                .any(|t| {
                    t["kind"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|x| x == "proc-macro"))
                });
            for kind in dep["dep_kinds"]
                .as_array()
                .ok_or("dependency kinds missing")?
            {
                if kind["kind"] == "dev" {
                    continue;
                }
                stack.push((next, shipped && kind["kind"].is_null() && !proc_macro));
            }
        }
    }
    let workspace = Path::new(
        metadata["workspace_root"]
            .as_str()
            .ok_or("workspace root missing")?,
    );
    let notice_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("notices");
    let source_bytes = fs::read(notice_root.join("SOURCES.json")).map_err(|e| e.to_string())?;
    let source_doc: Value = serde_json::from_slice(&source_bytes).map_err(|e| e.to_string())?;
    for item in source_doc["files"]
        .as_array()
        .ok_or("notice sources files missing")?
    {
        let file = item["file"]
            .as_str()
            .ok_or("notice source filename missing")?;
        if item["sha256"].as_str() != Some(super::digest(&notice_root.join(file))?.as_str()) {
            return Err(format!("pinned license text hash mismatch: {file}"));
        }
    }
    let mut inventory = Vec::new();
    let mut copies: Vec<(PathBuf, Vec<u8>)> = vec![(
        PathBuf::from("third_party/NOTICE_SOURCES.json"),
        source_bytes,
    )];
    for (id, shipped) in scopes {
        let p = packages[id];
        let name = p["name"].as_str().ok_or("name missing")?;
        let version = p["version"].as_str().ok_or("version missing")?;
        let license=p["license"].as_str().filter(|l|!l.trim().is_empty()).ok_or_else(||format!("{name} {version}: missing license metadata; supply SPDX license in upstream/vendored Cargo.toml before release"))?;
        let root = Path::new(p["manifest_path"].as_str().ok_or("manifest path missing")?)
            .parent()
            .ok_or("package directory missing")?;
        let label = format!("{}-{}", safe_label(name)?, safe_label(version)?);
        let mut notices = Vec::new();
        collect_notices(root, root, &mut notices)?;
        if let Some(file) = p["license_file"].as_str() {
            let path = root.join(file);
            if !notices
                .iter()
                .any(|(_, bytes)| fs::read(&path).ok().as_ref() == Some(bytes))
            {
                notices.push((
                    "DECLARED-LICENSE".into(),
                    fs::read(&path)
                        .map_err(|e| format!("{name}: declared license_file {file}: {e}"))?,
                ));
            }
        }
        if name == "pubg-ocr-engine" && notices.is_empty() {
            let path = workspace
                .parent()
                .ok_or("repository root missing")?
                .join("LICENSE");
            notices.push(("LICENSE".into(), fs::read(path).map_err(|e| e.to_string())?));
        }
        let supplement = match (name,version) {
            ("profiling"|"profiling-procmacros","1.0.18") => Some(("profiling-LICENSE-MIT","https://raw.githubusercontent.com/aclysma/profiling/8271551172eb6fa4cba47369aedd93790c623df9/LICENSE-MIT")),
            ("pulp-wasm-simd-flag","0.1.1") => Some(("pulp-LICENSE","https://raw.githubusercontent.com/sarah-quinones/pulp/5eb07fd7b68edf0a5e19f71737d315f72a510295/LICENSE")),
            ("zune-inflate","0.2.54") => Some(("zune-LICENSE.md","https://raw.githubusercontent.com/etemesi254/zune-image/69502ce83fdfecdd0beefd677e2abb3781b29d98/LICENSE.md")),
            _ => None,
        };
        if notices.is_empty() {
            if let Some((file, _)) = supplement {
                notices.push((
                    file.into(),
                    fs::read(notice_root.join(file)).map_err(|e| e.to_string())?,
                ));
            }
        }
        notices.sort_by(|a, b| a.0.cmp(&b.0));
        if notices.is_empty() {
            if shipped {
                return Err(format!("{name} {version}: runtime license text missing; provide a reviewed pinned upstream notice before distribution"));
            }
            eprintln!("WARNING {name} {version}: build-only crate contains no LICENSE/NOTICE/COPYING text; SPDX metadata retained, this crate is not distributed as runtime code");
        }
        let notice_paths: Vec<_> = notices
            .iter()
            .map(|(rel, _)| format!("third_party/rust/{label}/{rel}"))
            .collect();
        for ((_, bytes), rel) in notices.into_iter().zip(&notice_paths) {
            copies.push((PathBuf::from(rel), bytes));
        }
        let source = p["source"].as_str().map(str::to_owned).unwrap_or_else(|| {
            root.strip_prefix(workspace.parent().unwrap_or(workspace))
                .map(|rel| format!("repository:{}", rel.to_string_lossy().replace('\\', "/")))
                .unwrap_or_else(|_| format!("local-package:{name}"))
        });
        let mut dependencies = Vec::new();
        for dep in nodes[id]["deps"].as_array().ok_or("node deps missing")? {
            let target = packages[dep["pkg"].as_str().ok_or("dependency ID missing")?];
            for kind in dep["dep_kinds"]
                .as_array()
                .ok_or("dependency kinds missing")?
            {
                if kind["kind"] == "dev" {
                    continue;
                }
                dependencies.push(json!({"name":target["name"],"version":target["version"],"kind":kind["kind"].as_str().unwrap_or("normal"),"target":kind["target"]}));
            }
        }
        dependencies.sort_by_key(|v| v.to_string());
        inventory.push(json!({"dependencies":dependencies,"repository":p["repository"],"name":name,"version":version,"source":source,"license":license,"scope":if shipped{"shipped"}else{"build-only"},"representation":if shipped{"application dependency; may be statically linked or optimized out, not a separate bundled crate file"}else{"build tool/proc-macro dependency; not an application runtime component"},"notice_text_available":!notice_paths.is_empty(),"supplemental_notice_source":supplement.map(|(_,url)|url),"notices":notice_paths}));
    }
    inventory.sort_by_key(|p| {
        (
            p["name"].as_str().unwrap().to_owned(),
            p["version"].as_str().unwrap().to_owned(),
            p["source"].as_str().unwrap().to_owned(),
        )
    });
    for (src, dest) in [
        (
            "onnxruntime-1.20.0/LICENSE",
            "third_party/onnxruntime-1.20.0/LICENSE",
        ),
        (
            "onnxruntime-1.20.0/ThirdPartyNotices.txt",
            "third_party/onnxruntime-1.20.0/ThirdPartyNotices.txt",
        ),
        ("PP-OCR-LICENSE", "third_party/pp-ocrv6/LICENSE"),
    ] {
        copies.push((
            PathBuf::from(dest),
            fs::read(notice_root.join(src))
                .map_err(|e| format!("required pinned notice {src}: {e}"))?,
        ));
    }
    let document = json!({"schema":"pubg-highlight-trim/rust-licenses/v1","target":"x86_64-pc-windows-msvc","features":["cli","directml"],"root_package":"pubg-ocr-engine","cargo_lock_sha256":super::digest(&workspace.join("Cargo.lock"))?,"selection":"resolved normal + build dependency closure; dev and xtask-only excluded; proc-macros and build subgraphs classified build-only","packages":inventory,"native_and_models":[{"name":"ONNX Runtime","version":"1.20.0","license":"MIT","source":"https://github.com/microsoft/onnxruntime/tree/v1.20.0","notices":["third_party/onnxruntime-1.20.0/LICENSE","third_party/onnxruntime-1.20.0/ThirdPartyNotices.txt"]},{"name":"PP-OCRv6 model assets","license":"Apache-2.0","source":"https://github.com/PaddlePaddle/PaddleOCR","notices":["third_party/pp-ocrv6/LICENSE"]}]});
    let mut document = document;
    use sha2::{Digest, Sha256};
    document["notice_files"] = json!(copies.iter().map(|(path,bytes)|json!({"path":path.to_string_lossy().replace('\\',"/"),"sha256":format!("{:x}",Sha256::digest(bytes))})).collect::<Vec<_>>());
    // Validate the entire inventory and gather all bytes before writing anything.
    for (rel, bytes) in copies {
        super::atomic_write(&bundle.join(rel), &bytes)?;
    }
    super::atomic_write(
        &bundle.join("third_party/rust/INVENTORY.json"),
        (serde_json::to_string_pretty(&document).map_err(|e| e.to_string())? + "\n").as_bytes(),
    )?;
    println!(
        "collected {} Rust packages; run xtask manifest AFTER all notices are copied",
        document["packages"].as_array().unwrap().len()
    );
    Ok(())
}
