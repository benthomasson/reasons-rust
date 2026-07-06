use rusqlite::Connection;
use crate::db;
use crate::types::{Node, Justification};
use crate::tms::Network;

pub fn cmd_add(
    conn: &Connection,
    node_id: &str,
    text: &str,
    sl: Option<&str>,
    unless: Option<&str>,
    source: Option<&str>,
    source_url: Option<&str>,
    label: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if db::load_node(conn, node_id)?.is_some() {
        eprintln!("Node already exists: {}", node_id);
        std::process::exit(1);
    }

    let mut node = Node::new(node_id.to_string(), text.to_string());
    if let Some(s) = source {
        node.source = s.to_string();
    }
    if let Some(u) = source_url {
        node.source_url = u.to_string();
    }

    if let Some(deps) = sl {
        let antecedents: Vec<String> = deps.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        let outlist: Vec<String> = unless
            .map(|u| u.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();

        let all_nodes = db::load_all_nodes(conn)?;
        let all_justs = db::load_all_justifications(conn)?;
        let mut net = Network::load(all_nodes, all_justs);

        net.nodes.insert(node_id.to_string(), node.clone());
        let j = Justification::new_sl(
            node_id.to_string(),
            antecedents,
            outlist,
            label.unwrap_or("").to_string(),
        );

        let changed = net.add_justification(node_id, j.clone());

        if let Some(n) = net.nodes.get(node_id) {
            node.truth_value = n.truth_value.clone();
        }
        db::save_node(conn, &node)?;
        let rowid = db::save_justification(conn, &j)?;
        let _ = rowid;

        for cid in &changed {
            if cid != node_id {
                if let Some(n) = net.nodes.get(cid) {
                    db::update_node_truth(conn, cid, &n.truth_value)?;
                    db::log_propagation(conn, "propagate", cid, &n.truth_value)?;
                }
            }
        }
        db::log_propagation(conn, "add", node_id, &node.truth_value)?;
    } else {
        db::save_node(conn, &node)?;
        db::log_propagation(conn, "add", node_id, "IN")?;
    }

    db::rebuild_fts(conn)?;
    let now = chrono::Utc::now().to_rfc3339();
    db::set_meta(conn, "updated_at", &now)?;

    println!("Added {} [{}]", node_id, node.truth_value);
    Ok(())
}

pub fn cmd_add_justification(
    conn: &Connection,
    node_id: &str,
    sl: &str,
    unless: Option<&str>,
    label: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if db::load_node(conn, node_id)?.is_none() {
        eprintln!("Node not found: {}", node_id);
        std::process::exit(1);
    }

    let antecedents: Vec<String> = sl.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let outlist: Vec<String> = unless
        .map(|u| u.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();

    let j = Justification::new_sl(
        node_id.to_string(),
        antecedents,
        outlist,
        label.unwrap_or("").to_string(),
    );

    let all_nodes = db::load_all_nodes(conn)?;
    let all_justs = db::load_all_justifications(conn)?;
    let mut net = Network::load(all_nodes, all_justs);

    let changed = net.add_justification(node_id, j.clone());
    db::save_justification(conn, &j)?;

    for cid in &changed {
        if let Some(n) = net.nodes.get(cid) {
            db::update_node_truth(conn, cid, &n.truth_value)?;
            db::log_propagation(conn, "propagate", cid, &n.truth_value)?;
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    db::set_meta(conn, "updated_at", &now)?;
    println!("Added justification to {}", node_id);
    Ok(())
}

pub fn cmd_remove_justification(
    conn: &Connection,
    node_id: &str,
    index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let justs = db::load_justifications(conn, node_id)?;
    if index >= justs.len() {
        eprintln!("Justification index {} out of range (node has {})", index, justs.len());
        std::process::exit(1);
    }

    let rowid = justs[index].rowid;
    db::delete_justification(conn, rowid)?;

    let all_nodes = db::load_all_nodes(conn)?;
    let all_justs = db::load_all_justifications(conn)?;
    let mut net = Network::load(all_nodes, all_justs);

    let new_truth = net.compute_truth(node_id).to_string();
    if let Some(node) = net.nodes.get_mut(node_id) {
        if node.truth_value != new_truth {
            node.truth_value = new_truth.clone();
            db::update_node_truth(conn, node_id, &new_truth)?;
            let cascaded = net.propagate(node_id);
            for cid in &cascaded {
                if let Some(n) = net.nodes.get(cid) {
                    db::update_node_truth(conn, cid, &n.truth_value)?;
                    db::log_propagation(conn, "propagate", cid, &n.truth_value)?;
                }
            }
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    db::set_meta(conn, "updated_at", &now)?;
    println!("Removed justification {} from {}", index, node_id);
    Ok(())
}
