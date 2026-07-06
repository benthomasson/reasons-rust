use rusqlite::Connection;
use crate::db;
use crate::types::{Node, Justification};
use crate::tms::Network;

pub fn cmd_challenge(
    conn: &Connection,
    target_id: &str,
    reason: &str,
    challenge_id: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if db::load_node(conn, target_id)?.is_none() {
        eprintln!("Node not found: {}", target_id);
        std::process::exit(1);
    }

    let cid = challenge_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("challenge-{}", target_id));

    let challenge_node = Node::new(cid.clone(), reason.to_string());
    db::save_node(conn, &challenge_node)?;

    let all_nodes = db::load_all_nodes(conn)?;
    let all_justs = db::load_all_justifications(conn)?;
    let mut net = Network::load(all_nodes, all_justs);

    let target_justs = net.justifications.get(target_id).cloned().unwrap_or_default();

    let mut all_changed = Vec::new();

    if target_justs.is_empty() {
        let j = Justification::new_sl(
            target_id.to_string(),
            Vec::new(),
            vec![cid.clone()],
            String::new(),
        );
        db::save_justification(conn, &j)?;
        all_changed.extend(net.add_justification(target_id, j));
    } else {
        conn.execute(
            "DELETE FROM justifications WHERE node_id = ?1",
            rusqlite::params![target_id],
        )?;

        for mut j in target_justs {
            if !j.outlist.contains(&cid) {
                j.outlist.push(cid.clone());
            }
            db::save_justification(conn, &j)?;
        }

        let reloaded_justs = db::load_justifications(conn, target_id)?;
        net.justifications.insert(target_id.to_string(), reloaded_justs);
        net.rebuild_dependents();

        let new_truth = net.compute_truth(target_id).to_string();
        if let Some(target) = net.nodes.get_mut(target_id) {
            if target.truth_value != new_truth {
                target.truth_value = new_truth;
                all_changed.push(target_id.to_string());
                all_changed.extend(net.propagate(target_id));
            }
        }
    }

    if let Some(target) = net.nodes.get_mut(target_id) {
        if let Some(obj) = target.metadata.as_object_mut() {
            let challenges = obj.entry("challenges".to_string())
                .or_insert_with(|| serde_json::json!([]));
            if let Some(arr) = challenges.as_array_mut() {
                arr.push(serde_json::json!(cid));
            }
        }
        db::update_node_metadata(conn, target_id, &target.metadata)?;
    }

    for changed_id in &all_changed {
        if let Some(n) = net.nodes.get(changed_id) {
            db::update_node_truth(conn, changed_id, &n.truth_value)?;
            db::log_propagation(conn, "propagate", changed_id, &n.truth_value)?;
        }
    }

    db::log_propagation(conn, "challenge", target_id, &net.nodes.get(target_id).map_or("OUT", |n| &n.truth_value).to_string())?;
    db::rebuild_fts(conn)?;
    let now = chrono::Utc::now().to_rfc3339();
    db::set_meta(conn, "updated_at", &now)?;

    println!("Challenged {} with {} -> target is now {}",
        target_id, cid,
        net.nodes.get(target_id).map_or("?", |n| &n.truth_value));
    Ok(())
}

pub fn cmd_defend(
    conn: &Connection,
    target_id: &str,
    challenge_id: &str,
    reason: &str,
    defense_id: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let did = defense_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("defense-{}", challenge_id));

    cmd_challenge(conn, challenge_id, reason, Some(&did))?;

    if let Some(mut defense) = db::load_node(conn, &did)? {
        if let Some(obj) = defense.metadata.as_object_mut() {
            obj.insert("defense_target".to_string(), serde_json::json!(target_id));
            obj.insert("defends".to_string(), serde_json::json!(challenge_id));
        }
        db::update_node_metadata(conn, &did, &defense.metadata)?;
    }

    println!("Defended {} against {} with {}",
        target_id, challenge_id, did);
    Ok(())
}

pub fn cmd_supersede(
    conn: &Connection,
    old_id: &str,
    new_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if db::load_node(conn, old_id)?.is_none() {
        eprintln!("Node not found: {}", old_id);
        std::process::exit(1);
    }
    if db::load_node(conn, new_id)?.is_none() {
        eprintln!("Node not found: {}", new_id);
        std::process::exit(1);
    }

    let old_justs = db::load_justifications(conn, old_id)?;

    if old_justs.is_empty() {
        let j = Justification::new_sl(
            old_id.to_string(),
            Vec::new(),
            vec![new_id.to_string()],
            String::new(),
        );
        db::save_justification(conn, &j)?;
    } else {
        conn.execute(
            "DELETE FROM justifications WHERE node_id = ?1",
            rusqlite::params![old_id],
        )?;
        for mut j in old_justs {
            if !j.outlist.contains(&new_id.to_string()) {
                j.outlist.push(new_id.to_string());
            }
            db::save_justification(conn, &j)?;
        }
    }

    if let Some(mut old_node) = db::load_node(conn, old_id)? {
        if let Some(obj) = old_node.metadata.as_object_mut() {
            obj.insert("superseded_by".to_string(), serde_json::json!(new_id));
        }
        db::update_node_metadata(conn, old_id, &old_node.metadata)?;
    }

    if let Some(mut new_node) = db::load_node(conn, new_id)? {
        if let Some(obj) = new_node.metadata.as_object_mut() {
            let supersedes = obj.entry("supersedes".to_string())
                .or_insert_with(|| serde_json::json!([]));
            if let Some(arr) = supersedes.as_array_mut() {
                arr.push(serde_json::json!(old_id));
            }
        }
        db::update_node_metadata(conn, new_id, &new_node.metadata)?;
    }

    let all_nodes = db::load_all_nodes(conn)?;
    let all_justs = db::load_all_justifications(conn)?;
    let mut net = Network::load(all_nodes, all_justs);

    let new_truth = net.compute_truth(old_id).to_string();
    if let Some(old_node) = net.nodes.get_mut(old_id) {
        if old_node.truth_value != new_truth {
            old_node.truth_value = new_truth;
            db::update_node_truth(conn, old_id, &old_node.truth_value)?;
            let cascaded = net.propagate(old_id);
            for cid in &cascaded {
                if let Some(n) = net.nodes.get(cid) {
                    db::update_node_truth(conn, cid, &n.truth_value)?;
                }
            }
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    db::set_meta(conn, "updated_at", &now)?;

    println!("Superseded {} with {} -> {} is now {}",
        old_id, new_id, old_id,
        net.nodes.get(old_id).map_or("?", |n| &n.truth_value));
    Ok(())
}
