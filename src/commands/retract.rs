use rusqlite::Connection;
use crate::db;
use crate::tms::Network;

pub fn cmd_retract(
    conn: &Connection,
    node_id: &str,
    reason: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if db::load_node(conn, node_id)?.is_none() {
        eprintln!("Node not found: {}", node_id);
        std::process::exit(1);
    }

    let all_nodes = db::load_all_nodes(conn)?;
    let all_justs = db::load_all_justifications(conn)?;
    let mut net = Network::load(all_nodes, all_justs);

    let cascaded = net.retract(node_id, reason);

    if let Some(node) = net.nodes.get(node_id) {
        db::save_node(conn, node)?;
        db::log_propagation(conn, "retract", node_id, "OUT")?;
    }

    for cid in &cascaded {
        if let Some(n) = net.nodes.get(cid) {
            db::update_node_truth(conn, cid, &n.truth_value)?;
            db::log_propagation(conn, "propagate", cid, &n.truth_value)?;
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    db::set_meta(conn, "updated_at", &now)?;

    println!("Retracted {}", node_id);
    if !cascaded.is_empty() {
        let summary: Vec<String> = cascaded.iter().map(|id| {
            let tv = net.nodes.get(id).map_or("?", |n| &n.truth_value);
            format!("{} {}", id, tv)
        }).collect();
        println!("  Cascaded: {}", summary.join(", "));
    }
    Ok(())
}

pub fn cmd_assert(
    conn: &Connection,
    node_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if db::load_node(conn, node_id)?.is_none() {
        eprintln!("Node not found: {}", node_id);
        std::process::exit(1);
    }

    let all_nodes = db::load_all_nodes(conn)?;
    let all_justs = db::load_all_justifications(conn)?;
    let mut net = Network::load(all_nodes, all_justs);

    let cascaded = net.assert_node(node_id);

    if let Some(node) = net.nodes.get(node_id) {
        db::save_node(conn, node)?;
        db::log_propagation(conn, "assert", node_id, "IN")?;
    }

    for cid in &cascaded {
        if let Some(n) = net.nodes.get(cid) {
            db::update_node_truth(conn, cid, &n.truth_value)?;
            db::log_propagation(conn, "propagate", cid, &n.truth_value)?;
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    db::set_meta(conn, "updated_at", &now)?;

    println!("Asserted {}", node_id);
    if !cascaded.is_empty() {
        let summary: Vec<String> = cascaded.iter().map(|id| {
            let tv = net.nodes.get(id).map_or("?", |n| &n.truth_value);
            format!("{} {}", id, tv)
        }).collect();
        println!("  Cascaded: {}", summary.join(", "));
    }
    Ok(())
}
