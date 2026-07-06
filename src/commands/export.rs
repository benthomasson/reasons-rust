use rusqlite::Connection;
use std::path::Path;
use crate::db;

pub fn cmd_export(
    conn: &Connection,
    output: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let nodes = db::load_all_nodes(conn)?;
    let all_justs = db::load_all_justifications(conn)?;
    let nogoods = db::load_nogoods(conn)?;
    let repos = db::load_repos(conn)?;

    let schema_version = db::load_meta(conn, "schema_version")?.unwrap_or_else(|| "1.0".to_string());
    let project_name = db::load_meta(conn, "project_name")?.unwrap_or_default();
    let created_at = db::load_meta(conn, "created_at")?.unwrap_or_default();
    let updated_at = db::load_meta(conn, "updated_at")?.unwrap_or_default();

    let mut nodes_map = serde_json::Map::new();
    for node in &nodes {
        let node_justs: Vec<_> = all_justs.iter()
            .filter(|j| j.node_id == node.id)
            .map(|j| {
                let mut jmap = serde_json::Map::new();
                jmap.insert("type".to_string(), serde_json::json!(j.jtype));
                jmap.insert("antecedents".to_string(), serde_json::json!(j.antecedents));
                jmap.insert("outlist".to_string(), serde_json::json!(j.outlist));
                jmap.insert("label".to_string(), serde_json::json!(j.label));
                serde_json::Value::Object(jmap)
            })
            .collect();

        let mut filtered_metadata = serde_json::Map::new();
        if let Some(obj) = node.metadata.as_object() {
            for (k, v) in obj {
                if !k.starts_with('_') {
                    filtered_metadata.insert(k.clone(), v.clone());
                }
            }
        }

        let mut nmap = serde_json::Map::new();
        nmap.insert("text".to_string(), serde_json::json!(node.text));
        nmap.insert("truth_value".to_string(), serde_json::json!(node.truth_value));
        nmap.insert("justifications".to_string(), serde_json::json!(node_justs));
        nmap.insert("source".to_string(), serde_json::json!(node.source));
        nmap.insert("source_url".to_string(), serde_json::json!(node.source_url));
        nmap.insert("source_hash".to_string(), serde_json::json!(node.source_hash));
        nmap.insert("date".to_string(), serde_json::json!(node.date));
        nmap.insert("metadata".to_string(), serde_json::Value::Object(filtered_metadata));
        nmap.insert("created_at".to_string(), serde_json::json!(node.created_at));
        nmap.insert("updated_at".to_string(), serde_json::json!(node.updated_at));
        nmap.insert("reviewed_at".to_string(), serde_json::json!(node.reviewed_at));
        nmap.insert("verified_at".to_string(), serde_json::json!(node.verified_at));
        nmap.insert("retracted_at".to_string(), serde_json::json!(node.retracted_at));

        nodes_map.insert(node.id.clone(), serde_json::Value::Object(nmap));
    }

    let nogoods_json: Vec<serde_json::Value> = nogoods.iter().map(|ng| {
        serde_json::json!({
            "id": ng.id,
            "nodes": ng.nodes,
            "discovered": ng.discovered,
            "resolution": ng.resolution,
        })
    }).collect();

    let export = serde_json::json!({
        "meta": {
            "schema_version": schema_version,
            "project_name": project_name,
            "created_at": created_at,
            "updated_at": updated_at,
            "node_count": nodes.len(),
            "generator": format!("reasons/{}", env!("CARGO_PKG_VERSION")),
        },
        "nodes": nodes_map,
        "nogoods": nogoods_json,
        "repos": repos,
    });

    let json_str = serde_json::to_string_pretty(&export)?;

    match output {
        Some(path) => {
            std::fs::write(path, &json_str)?;
            println!("Exported to {}", path.display());
        }
        None => {
            println!("{}", json_str);
        }
    }

    Ok(())
}

pub fn cmd_export_markdown(
    conn: &Connection,
    output: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let nodes = db::load_all_nodes(conn)?;
    let all_justs = db::load_all_justifications(conn)?;
    let nogoods = db::load_nogoods(conn)?;
    let repos = db::load_repos(conn)?;

    let project_name = db::load_meta(conn, "project_name")?.unwrap_or_default();
    let updated_at = db::load_meta(conn, "updated_at")?.unwrap_or_default();

    let mut out = String::new();

    out.push_str("---\n");
    out.push_str(&format!("schema_version: \"1.0\"\n"));
    out.push_str(&format!("project_name: \"{}\"\n", project_name));
    out.push_str(&format!("updated_at: \"{}\"\n", updated_at));
    out.push_str(&format!("node_count: {}\n", nodes.len()));
    out.push_str(&format!("generator: \"reasons/{}\"\n", env!("CARGO_PKG_VERSION")));
    out.push_str("---\n\n");

    out.push_str("# Belief Registry\n\n");
    out.push_str("## Claims\n\n");

    for node in &nodes {
        let node_justs: Vec<_> = all_justs.iter().filter(|j| j.node_id == node.id).collect();

        let status = if node.truth_value == "OUT" {
            let has_stale = node.metadata.get("stale_reason").is_some()
                || node.metadata.get("retract_reason").is_some();
            if has_stale { "STALE" } else { "OUT" }
        } else {
            "IN"
        };

        let beliefs_type = node.metadata
            .get("beliefs_type")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                if node_justs.is_empty() { "OBSERVATION" } else { "DERIVED" }
            });

        out.push_str(&format!("### {} [{}] {}\n", node.id, status, beliefs_type));
        out.push_str(&format!("{}\n\n", node.text));

        if !node.source.is_empty() {
            out.push_str(&format!("- Source: {}\n", node.source));
        }
        if !node.source_url.is_empty() {
            out.push_str(&format!("- Source URL: {}\n", node.source_url));
        }

        let depends: Vec<String> = node_justs.iter()
            .flat_map(|j| j.antecedents.iter().cloned())
            .collect();
        if !depends.is_empty() {
            let unique: Vec<_> = {
                let mut seen = std::collections::HashSet::new();
                depends.into_iter().filter(|d| seen.insert(d.clone())).collect()
            };
            out.push_str(&format!("- Depends on: {}\n", unique.join(", ")));
        }

        let unless: Vec<String> = node_justs.iter()
            .flat_map(|j| j.outlist.iter().cloned())
            .collect();
        if !unless.is_empty() {
            let unique: Vec<_> = {
                let mut seen = std::collections::HashSet::new();
                unless.into_iter().filter(|u| seen.insert(u.clone())).collect()
            };
            out.push_str(&format!("- Unless: {}\n", unique.join(", ")));
        }

        out.push('\n');
    }

    if !repos.is_empty() {
        out.push_str("## Repos\n\n");
        for (name, path) in &repos {
            out.push_str(&format!("- {}: {}\n", name, path));
        }
        out.push('\n');
    }

    if !nogoods.is_empty() {
        out.push_str("## Nogoods\n\n");
        for ng in &nogoods {
            out.push_str(&format!("### {}\n", ng.id));
            if !ng.discovered.is_empty() {
                out.push_str(&format!("- Discovered: {}\n", ng.discovered));
            }
            if !ng.resolution.is_empty() {
                out.push_str(&format!("- Resolution: {}\n", ng.resolution));
            }
            out.push_str(&format!("- Affects: {}\n", ng.nodes.join(", ")));
            out.push('\n');
        }
    }

    match output {
        Some(path) => {
            std::fs::write(path, &out)?;
            println!("Exported to {}", path.display());
        }
        None => {
            print!("{}", out);
        }
    }

    Ok(())
}
