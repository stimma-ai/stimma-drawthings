use std::{env, fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
    tonic_prost_build::configure()
        .build_server(true)
        .compile_protos(&["schemas/image_service.proto"], &["schemas"])?;
    let generated=fs::read_to_string("src/generated/config_generated.rs")?
        .replace("#[derive(Debug, Clone, PartialEq)]\npub struct ","#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\n#[serde(default, deny_unknown_fields)]\npub struct ");
    let out = PathBuf::from(env::var("OUT_DIR")?);
    fs::write(out.join("config_generated.rs"), generated)?;
    println!("cargo:rerun-if-changed=schemas/image_service.proto");
    println!("cargo:rerun-if-changed=src/generated/config_generated.rs");
    native_schema(&out)?;
    manager_assets(&out)?;
    println!("cargo:rerun-if-changed=schemas/config.fbs");
    Ok(())
}

fn native_schema(out: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use serde_json::{json, Value};
    let source = fs::read_to_string("schemas/config.fbs")?;
    let mut types = std::collections::BTreeMap::<String, Value>::new();
    for chunk in source.split("enum ").skip(1) {
        let name = chunk.split(':').next().unwrap().trim();
        let body = chunk.split('{').nth(1).unwrap().split('}').next().unwrap();
        let variants: Vec<_> = body
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        types.insert(name.into(), json!({"type":"string","enum":variants}));
    }
    for chunk in source.split("table ").skip(1) {
        let name = chunk.split('{').next().unwrap().trim();
        let body = chunk.split('{').nth(1).unwrap().split('}').next().unwrap();
        let mut properties = serde_json::Map::new();
        for line in body.split(';') {
            if line.contains("deprecated") {
                continue;
            }
            let Some((field, decl)) = line.trim().split_once(':') else {
                continue;
            };
            let kind = decl.trim().split([' ', '=', '(']).next().unwrap();
            let schema = match kind {
                "string" => json!({"type":"string"}),
                "bool" => json!({"type":"boolean"}),
                "float" => json!({"type":"number"}),
                "ubyte" => json!({"type":"integer","minimum":0,"maximum":255}),
                "ushort" => json!({"type":"integer","minimum":0,"maximum":65535}),
                "uint" => json!({"type":"integer","minimum":0,"maximum":4294967295u64}),
                "int" => json!({"type":"integer","minimum":-2147483648i64,"maximum":2147483647}),
                "long" => json!({"type":"integer"}),
                "[string]" => json!({"type":"array","items":{"type":"string"}}),
                k if k.starts_with('[') => {
                    json!({"type":"array","items":types[&k[1..k.len()-1]].clone()})
                }
                k => types
                    .get(k)
                    .ok_or(format!("Unknown schema type {k}"))?
                    .clone(),
            };
            properties.insert(field.trim().into(), schema);
        }
        types.insert(
            name.into(),
            json!({"type":"object","additionalProperties":false,"properties":properties}),
        );
    }
    fs::write(
        out.join("native_schema.json"),
        serde_json::to_vec(&types["GenerationConfiguration"])?,
    )?;
    Ok(())
}

fn manager_assets(out: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    fn walk(
        root: &std::path::Path,
        dir: &std::path::Path,
        rows: &mut Vec<String>,
    ) -> std::io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(root, &path, rows)?;
                continue;
            }
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let mime = match path.extension().and_then(|s| s.to_str()).unwrap_or("") {
                "html" => "text/html; charset=utf-8",
                "css" => "text/css",
                "js" => "text/javascript",
                "svg" => "image/svg+xml",
                "woff2" => "font/woff2",
                _ => "application/octet-stream",
            };
            rows.push(format!("{name:?} => Some(({mime:?}, include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/manager-ui/dist/{name}\")))),"));
        }
        Ok(())
    }
    let mut rows = Vec::new();
    walk(
        std::path::Path::new("manager-ui/dist"),
        std::path::Path::new("manager-ui/dist"),
        &mut rows,
    )?;
    rows.sort();
    fs::write(out.join("manager_assets.rs"), format!("fn embedded_asset(path: &str) -> Option<(&'static str, &'static [u8])> {{ match path {{ {} _ => None }} }}", rows.join("\n")))?;
    println!("cargo:rerun-if-changed=manager-ui/dist");
    Ok(())
}
