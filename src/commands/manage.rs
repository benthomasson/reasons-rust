use rusqlite::Connection;
use std::path::Path;
use crate::db;

pub fn cmd_init(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if db_path.exists() {
        eprintln!("Database already exists: {}", db_path.display());
        std::process::exit(1);
    }
    db::init_db(db_path)?;
    println!("Initialized reasons database: {}", db_path.display());
    Ok(())
}

pub fn cmd_status(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let (total, in_count) = db::node_count(conn)?;
    let out_count = total - in_count;

    let all_justs = db::load_all_justifications(conn)?;
    let mut nodes_with_justs = std::collections::HashSet::new();
    for j in &all_justs {
        nodes_with_justs.insert(j.node_id.clone());
    }
    let premises = total - nodes_with_justs.len();
    let derived = nodes_with_justs.len();

    let nogood_count = db::nogood_count(conn)?;
    let updated_at = db::load_meta(conn, "updated_at")?.unwrap_or_default();

    println!("reasons.db");
    println!("  Nodes: {} ({} IN, {} OUT)", total, in_count, out_count);
    println!("  Premises: {}", premises);
    println!("  Derived: {}", derived);
    println!("  Nogoods: {}", nogood_count);
    if !updated_at.is_empty() {
        println!("  Last updated: {}", updated_at);
    }
    Ok(())
}

pub fn cmd_log(conn: &Connection, limit: usize) -> Result<(), Box<dyn std::error::Error>> {
    let entries = db::load_propagation_log(conn, limit)?;
    if entries.is_empty() {
        println!("No propagation log entries.");
        return Ok(());
    }
    for (ts, action, target, value) in &entries {
        println!("{}  {}  {}  {}", ts, action, target, value);
    }
    Ok(())
}

pub fn cmd_propagate(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let nodes = db::load_all_nodes(conn)?;
    let justs = db::load_all_justifications(conn)?;
    let mut net = crate::tms::Network::load(nodes, justs);

    let changed = net.recompute_all();
    if changed.is_empty() {
        println!("No changes.");
    } else {
        for id in &changed {
            if let Some(node) = net.nodes.get(id) {
                db::update_node_truth(conn, id, &node.truth_value)?;
                db::log_propagation(conn, "propagate", id, &node.truth_value)?;
                println!("  {} -> {}", id, node.truth_value);
            }
        }
        let now = chrono::Utc::now().to_rfc3339();
        db::set_meta(conn, "updated_at", &now)?;
        println!("Propagated {} changes.", changed.len());
    }
    Ok(())
}

pub fn cmd_update(
    conn: &Connection,
    node_id: &str,
    text: Option<&str>,
    source: Option<&str>,
    source_url: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let node = db::load_node(conn, node_id)?;
    if node.is_none() {
        eprintln!("Node not found: {}", node_id);
        std::process::exit(1);
    }

    let now = chrono::Utc::now().to_rfc3339();
    if let Some(t) = text {
        db::update_node_field(conn, node_id, "text", t)?;
    }
    if let Some(s) = source {
        db::update_node_field(conn, node_id, "source", s)?;
    }
    if let Some(u) = source_url {
        db::update_node_field(conn, node_id, "source_url", u)?;
    }
    db::update_node_field(conn, node_id, "updated_at", &now)?;
    db::rebuild_fts(conn)?;
    println!("Updated {}", node_id);
    Ok(())
}

pub fn cmd_set_metadata(
    conn: &Connection,
    node_id: &str,
    key: &str,
    value: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let node = db::load_node(conn, node_id)?;
    match node {
        None => {
            eprintln!("Node not found: {}", node_id);
            std::process::exit(1);
        }
        Some(mut node) => {
            let parsed: serde_json::Value = serde_json::from_str(value)
                .unwrap_or_else(|_| serde_json::json!(value));
            if let Some(obj) = node.metadata.as_object_mut() {
                obj.insert(key.to_string(), parsed);
            }
            db::update_node_metadata(conn, node_id, &node.metadata)?;
            println!("Set metadata {}.{}", node_id, key);
            Ok(())
        }
    }
}

pub fn cmd_get_metadata(
    conn: &Connection,
    node_id: &str,
    key: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let node = db::load_node(conn, node_id)?;
    match node {
        None => {
            eprintln!("Node not found: {}", node_id);
            std::process::exit(1);
        }
        Some(node) => {
            match key {
                Some(k) => {
                    if let Some(v) = node.metadata.get(k) {
                        println!("{}", serde_json::to_string_pretty(v)?);
                    } else {
                        println!("null");
                    }
                }
                None => {
                    println!("{}", serde_json::to_string_pretty(&node.metadata)?);
                }
            }
            Ok(())
        }
    }
}

pub fn cmd_trace(conn: &Connection, node_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let nodes = db::load_all_nodes(conn)?;
    let justs = db::load_all_justifications(conn)?;
    let net = crate::tms::Network::load(nodes, justs);

    if net.nodes.get(node_id).is_none() {
        eprintln!("Node not found: {}", node_id);
        std::process::exit(1);
    }

    let premises = net.find_premises(node_id);
    if premises.is_empty() {
        println!("{} is a premise (no supporting premises to trace).", node_id);
    } else {
        println!("Premises supporting {}:", node_id);
        for p in &premises {
            if let Some(node) = net.nodes.get(p) {
                println!("  {}: {}", p, crate::format::truncate(&node.text, 80));
            }
        }
    }
    Ok(())
}

pub fn cmd_convert_to_premise(conn: &Connection, node_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let node = db::load_node(conn, node_id)?;
    if node.is_none() {
        eprintln!("Node not found: {}", node_id);
        std::process::exit(1);
    }

    conn.execute(
        "DELETE FROM justifications WHERE node_id = ?1",
        rusqlite::params![node_id],
    )?;
    println!("Converted {} to premise (removed all justifications).", node_id);
    Ok(())
}
