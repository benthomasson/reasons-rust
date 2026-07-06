use rusqlite::Connection;
use crate::db;
use crate::types::Nogood;
use crate::tms::Network;

pub fn cmd_nogood(
    conn: &Connection,
    node_ids: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    for id in node_ids {
        if db::load_node(conn, id)?.is_none() {
            eprintln!("Node not found: {}", id);
            std::process::exit(1);
        }
    }

    let existing = db::load_nogoods(conn)?;
    let next_id = existing.len() + 1;
    let nogood_id = format!("nogood-{:03}", next_id);

    let now = chrono::Utc::now().to_rfc3339();
    let nogood = Nogood {
        id: nogood_id.clone(),
        nodes: node_ids.to_vec(),
        discovered: now.clone(),
        resolution: String::new(),
    };
    db::save_nogood(conn, &nogood)?;

    println!("Recorded contradiction: {}", nogood_id);
    println!("  Nodes: {}", node_ids.join(", "));

    let all_nodes = db::load_all_nodes(conn)?;
    let all_justs = db::load_all_justifications(conn)?;
    let mut net = Network::load(all_nodes, all_justs);

    let all_in = node_ids.iter().all(|id| {
        net.nodes.get(id).map_or(false, |n| n.truth_value == "IN")
    });

    if all_in {
        println!("  All nodes are IN — contradiction is active. Running backtracking...");
        let culprits = net.find_culprits(node_ids);

        if let Some((least_entrenched, score)) = culprits.first() {
            let cascaded = net.retract(least_entrenched, Some("Retracted by dependency-directed backtracking"));

            if let Some(node) = net.nodes.get(least_entrenched) {
                db::save_node(conn, node)?;
                db::log_propagation(conn, "backtrack-retract", least_entrenched, "OUT")?;
            }

            for cid in &cascaded {
                if let Some(n) = net.nodes.get(cid) {
                    db::update_node_truth(conn, cid, &n.truth_value)?;
                    db::log_propagation(conn, "propagate", cid, &n.truth_value)?;
                }
            }

            let resolution = format!("Retracted {} (entrenchment: {})", least_entrenched, score);
            let resolved_nogood = Nogood {
                resolution: resolution.clone(),
                ..nogood
            };
            db::save_nogood(conn, &resolved_nogood)?;

            println!("  Retracted {} (entrenchment: {})", least_entrenched, score);
            if !cascaded.is_empty() {
                println!("  Cascaded: {}", cascaded.join(", "));
            }
        } else {
            println!("  No culprit found for backtracking.");
        }
    } else {
        println!("  Not all nodes are IN — contradiction is not currently active.");
    }

    db::set_meta(conn, "updated_at", &now)?;
    Ok(())
}

pub fn cmd_find_culprits(
    conn: &Connection,
    node_ids: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    for id in node_ids {
        if db::load_node(conn, id)?.is_none() {
            eprintln!("Node not found: {}", id);
            std::process::exit(1);
        }
    }

    let all_nodes = db::load_all_nodes(conn)?;
    let all_justs = db::load_all_justifications(conn)?;
    let net = Network::load(all_nodes, all_justs);

    let culprits = net.find_culprits(node_ids);

    if culprits.is_empty() {
        println!("No culprit premises found.");
    } else {
        println!("Culprit premises (least entrenched first):");
        for (id, score) in &culprits {
            if let Some(node) = net.nodes.get(id) {
                println!("  {} (entrenchment: {}): {}", id, score, crate::format::truncate(&node.text, 60));
            }
        }
    }
    Ok(())
}
