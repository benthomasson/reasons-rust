use rusqlite::Connection;
use std::path::Path;
use crate::db;
use crate::types::{Node, Justification, Nogood};
use crate::tms::Network;

pub fn cmd_import_beliefs(
    conn: &Connection,
    file_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(file_path)?;
    let mut lines = content.lines().peekable();
    let mut count = 0;

    // Skip YAML frontmatter
    if lines.peek().map_or(false, |l| l.trim() == "---") {
        lines.next();
        while let Some(line) = lines.next() {
            if line.trim() == "---" {
                break;
            }
        }
    }

    let mut current_id: Option<String> = None;
    let mut current_status: Option<String> = None;
    let mut current_text = String::new();
    let mut current_source = String::new();
    let mut current_source_url = String::new();
    let mut current_depends: Vec<String> = Vec::new();
    let mut current_unless: Vec<String> = Vec::new();
    let mut in_claims = false;
    let mut in_repos = false;
    let mut in_nogoods = false;

    let flush = |conn: &Connection,
                 id: &str,
                 text: &str,
                 status: &str,
                 source: &str,
                 source_url: &str,
                 depends: &[String],
                 unless: &[String]|
                 -> Result<(), Box<dyn std::error::Error>> {
        if id.is_empty() || text.is_empty() {
            return Ok(());
        }

        let mut node = Node::new(id.to_string(), text.to_string());
        node.source = source.to_string();
        node.source_url = source_url.to_string();
        if status == "OUT" || status == "STALE" {
            node.truth_value = "OUT".to_string();
        }

        db::save_node(conn, &node)?;

        if !depends.is_empty() || !unless.is_empty() {
            let j = Justification::new_sl(
                id.to_string(),
                depends.to_vec(),
                unless.to_vec(),
                String::new(),
            );
            db::save_justification(conn, &j)?;
        }
        Ok(())
    };

    for line in lines {
        if line.starts_with("## Claims") {
            in_claims = true;
            in_repos = false;
            in_nogoods = false;
            continue;
        }
        if line.starts_with("## Repos") {
            if let Some(id) = current_id.take() {
                flush(conn, &id, current_text.trim(), current_status.as_deref().unwrap_or("IN"),
                    &current_source, &current_source_url, &current_depends, &current_unless)?;
                count += 1;
            }
            in_claims = false;
            in_repos = true;
            in_nogoods = false;
            continue;
        }
        if line.starts_with("## Nogoods") {
            if let Some(id) = current_id.take() {
                flush(conn, &id, current_text.trim(), current_status.as_deref().unwrap_or("IN"),
                    &current_source, &current_source_url, &current_depends, &current_unless)?;
                count += 1;
            }
            in_claims = false;
            in_repos = false;
            in_nogoods = true;
            continue;
        }

        if in_repos {
            if let Some(rest) = line.strip_prefix("- ") {
                if let Some((name, path)) = rest.split_once(": ") {
                    conn.execute(
                        "INSERT OR REPLACE INTO repos (name, path) VALUES (?1, ?2)",
                        rusqlite::params![name.trim(), path.trim()],
                    )?;
                }
            }
            continue;
        }

        if in_nogoods {
            // Minimal nogood parsing
            continue;
        }

        if !in_claims && current_id.is_none() {
            if line.starts_with("### ") {
                in_claims = true;
            } else {
                continue;
            }
        }

        if line.starts_with("### ") {
            if let Some(id) = current_id.take() {
                flush(conn, &id, current_text.trim(), current_status.as_deref().unwrap_or("IN"),
                    &current_source, &current_source_url, &current_depends, &current_unless)?;
                count += 1;
            }

            let header = &line[4..];
            let parts: Vec<&str> = header.splitn(3, ' ').collect();
            current_id = Some(parts.first().unwrap_or(&"").to_string());
            current_status = parts.get(1).map(|s| {
                s.trim_start_matches('[').trim_end_matches(']').to_string()
            });
            current_text = String::new();
            current_source = String::new();
            current_source_url = String::new();
            current_depends = Vec::new();
            current_unless = Vec::new();
        } else if line.starts_with("- Source URL: ") {
            current_source_url = line[14..].to_string();
        } else if line.starts_with("- Source: ") {
            current_source = line[10..].to_string();
        } else if line.starts_with("- Depends on: ") {
            current_depends = line[14..].split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        } else if line.starts_with("- Unless: ") {
            current_unless = line[10..].split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        } else if !line.starts_with("- ") && current_id.is_some() {
            if !current_text.is_empty() && !line.is_empty() {
                current_text.push(' ');
            }
            current_text.push_str(line.trim());
        }
    }

    if let Some(id) = current_id.take() {
        flush(conn, &id, current_text.trim(), current_status.as_deref().unwrap_or("IN"),
            &current_source, &current_source_url, &current_depends, &current_unless)?;
        count += 1;
    }

    // Recompute truth values
    let all_nodes = db::load_all_nodes(conn)?;
    let all_justs = db::load_all_justifications(conn)?;
    let mut net = Network::load(all_nodes, all_justs);
    let changed = net.recompute_all();
    for cid in &changed {
        if let Some(n) = net.nodes.get(cid) {
            db::update_node_truth(conn, cid, &n.truth_value)?;
        }
    }

    db::rebuild_fts(conn)?;
    let now = chrono::Utc::now().to_rfc3339();
    db::set_meta(conn, "updated_at", &now)?;

    println!("Imported {} beliefs from {}", count, file_path.display());
    Ok(())
}

pub fn cmd_import_json(
    conn: &Connection,
    file_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(file_path)?;
    let data: serde_json::Value = serde_json::from_str(&content)?;

    let mut count = 0;

    if let Some(nodes) = data.get("nodes").and_then(|n| n.as_object()) {
        for (id, ndata) in nodes {
            let mut node = Node::new(id.clone(), ndata.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string());
            node.truth_value = ndata.get("truth_value").and_then(|v| v.as_str()).unwrap_or("IN").to_string();
            node.source = ndata.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string();
            node.source_url = ndata.get("source_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            node.source_hash = ndata.get("source_hash").and_then(|v| v.as_str()).unwrap_or("").to_string();
            node.date = ndata.get("date").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if let Some(meta) = ndata.get("metadata") {
                node.metadata = meta.clone();
            }
            node.created_at = ndata.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
            node.updated_at = ndata.get("updated_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
            node.reviewed_at = ndata.get("reviewed_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
            node.verified_at = ndata.get("verified_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
            node.retracted_at = ndata.get("retracted_at").and_then(|v| v.as_str()).unwrap_or("").to_string();

            db::save_node(conn, &node)?;

            if let Some(justs) = ndata.get("justifications").and_then(|v| v.as_array()) {
                for jdata in justs {
                    let antecedents: Vec<String> = jdata.get("antecedents")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default();
                    let outlist: Vec<String> = jdata.get("outlist")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default();
                    let label = jdata.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let jtype = jdata.get("type").and_then(|v| v.as_str()).unwrap_or("SL").to_string();

                    let j = Justification {
                        rowid: 0,
                        node_id: id.clone(),
                        jtype,
                        antecedents,
                        outlist,
                        label,
                    };
                    db::save_justification(conn, &j)?;
                }
            }

            count += 1;
        }
    }

    if let Some(nogoods) = data.get("nogoods").and_then(|n| n.as_array()) {
        for ngdata in nogoods {
            let nogood = Nogood {
                id: ngdata.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                nodes: ngdata.get("nodes")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default(),
                discovered: ngdata.get("discovered").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                resolution: ngdata.get("resolution").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            };
            db::save_nogood(conn, &nogood)?;
        }
    }

    if let Some(repos) = data.get("repos").and_then(|r| r.as_object()) {
        for (name, path) in repos {
            if let Some(p) = path.as_str() {
                conn.execute(
                    "INSERT OR REPLACE INTO repos (name, path) VALUES (?1, ?2)",
                    rusqlite::params![name, p],
                )?;
            }
        }
    }

    // Recompute truth values
    let all_nodes = db::load_all_nodes(conn)?;
    let all_justs = db::load_all_justifications(conn)?;
    let mut net = Network::load(all_nodes, all_justs);
    let changed = net.recompute_all();
    for cid in &changed {
        if let Some(n) = net.nodes.get(cid) {
            db::update_node_truth(conn, cid, &n.truth_value)?;
        }
    }

    db::rebuild_fts(conn)?;
    let now = chrono::Utc::now().to_rfc3339();
    db::set_meta(conn, "updated_at", &now)?;

    println!("Imported {} nodes from {}", count, file_path.display());
    Ok(())
}
