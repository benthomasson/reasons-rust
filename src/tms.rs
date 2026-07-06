use std::collections::{HashMap, HashSet, VecDeque};
use crate::types::{Node, Justification};

pub struct Network {
    pub nodes: HashMap<String, Node>,
    pub justifications: HashMap<String, Vec<Justification>>,
    pub dependents: HashMap<String, HashSet<String>>,
}

impl Network {
    pub fn new() -> Self {
        Network {
            nodes: HashMap::new(),
            justifications: HashMap::new(),
            dependents: HashMap::new(),
        }
    }

    pub fn load(nodes: Vec<Node>, justifications: Vec<Justification>) -> Self {
        let mut net = Network::new();
        for node in nodes {
            net.nodes.insert(node.id.clone(), node);
        }
        for j in justifications {
            net.justifications
                .entry(j.node_id.clone())
                .or_default()
                .push(j);
        }
        net.rebuild_dependents();
        net
    }

    pub fn rebuild_dependents(&mut self) {
        self.dependents.clear();
        for justs in self.justifications.values() {
            for j in justs {
                for ant_id in &j.antecedents {
                    self.dependents
                        .entry(ant_id.clone())
                        .or_default()
                        .insert(j.node_id.clone());
                }
                for out_id in &j.outlist {
                    self.dependents
                        .entry(out_id.clone())
                        .or_default()
                        .insert(j.node_id.clone());
                }
            }
        }
    }

    pub fn justification_valid(&self, j: &Justification) -> bool {
        let inlist_ok = j.antecedents.iter().all(|a| {
            self.nodes.get(a).map_or(false, |n| n.truth_value == "IN")
        });
        let outlist_ok = j.outlist.iter().all(|o| {
            self.nodes.get(o).map_or(true, |n| n.truth_value == "OUT")
        });
        inlist_ok && outlist_ok
    }

    pub fn compute_truth(&self, node_id: &str) -> &str {
        match self.justifications.get(node_id) {
            None => self.nodes.get(node_id).map_or("OUT", |n| &n.truth_value),
            Some(justs) if justs.is_empty() => {
                self.nodes.get(node_id).map_or("OUT", |n| &n.truth_value)
            }
            Some(justs) => {
                if justs.iter().any(|j| self.justification_valid(j)) {
                    "IN"
                } else {
                    "OUT"
                }
            }
        }
    }

    pub fn propagate(&mut self, changed_id: &str) -> Vec<String> {
        let mut changed = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(changed_id.to_string());

        while let Some(current) = queue.pop_front() {
            if let Some(deps) = self.dependents.get(&current).cloned() {
                for dep_id in deps {
                    if let Some(node) = self.nodes.get(&dep_id) {
                        let is_retracted = node.metadata
                            .get("_retracted")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if is_retracted {
                            continue;
                        }
                    }

                    let new_truth = self.compute_truth(&dep_id).to_string();
                    if let Some(node) = self.nodes.get_mut(&dep_id) {
                        if node.truth_value != new_truth {
                            node.truth_value = new_truth;
                            changed.push(dep_id.clone());
                            queue.push_back(dep_id);
                        }
                    }
                }
            }
        }
        changed
    }

    pub fn retract(&mut self, node_id: &str, reason: Option<&str>) -> Vec<String> {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.truth_value = "OUT".to_string();
            if let Some(obj) = node.metadata.as_object_mut() {
                obj.insert("_retracted".to_string(), serde_json::json!(true));
                if let Some(r) = reason {
                    obj.insert("retract_reason".to_string(), serde_json::json!(r));
                }
            }
            node.retracted_at = chrono::Utc::now().to_rfc3339();
        }
        self.propagate(node_id)
    }

    pub fn assert_node(&mut self, node_id: &str) -> Vec<String> {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.truth_value = "IN".to_string();
            if let Some(obj) = node.metadata.as_object_mut() {
                obj.remove("_retracted");
                obj.remove("retract_reason");
            }
            node.retracted_at = String::new();
        }
        self.propagate(node_id)
    }

    pub fn add_justification(&mut self, node_id: &str, j: Justification) -> Vec<String> {
        for ant_id in &j.antecedents {
            self.dependents
                .entry(ant_id.clone())
                .or_default()
                .insert(node_id.to_string());
        }
        for out_id in &j.outlist {
            self.dependents
                .entry(out_id.clone())
                .or_default()
                .insert(node_id.to_string());
        }

        self.justifications
            .entry(node_id.to_string())
            .or_default()
            .push(j);

        let new_truth = self.compute_truth(node_id).to_string();
        let mut changed = Vec::new();
        if let Some(node) = self.nodes.get_mut(node_id) {
            if node.truth_value != new_truth {
                node.truth_value = new_truth;
                changed.push(node_id.to_string());
                changed.extend(self.propagate(node_id));
            }
        }
        changed
    }

    pub fn remove_justification(&mut self, node_id: &str, index: usize) -> Vec<String> {
        if let Some(justs) = self.justifications.get_mut(node_id) {
            if index < justs.len() {
                justs.remove(index);
            }
        }
        self.rebuild_dependents();

        let new_truth = self.compute_truth(node_id).to_string();
        let mut changed = Vec::new();
        if let Some(node) = self.nodes.get_mut(node_id) {
            if node.truth_value != new_truth {
                node.truth_value = new_truth;
                changed.push(node_id.to_string());
                changed.extend(self.propagate(node_id));
            }
        }
        changed
    }

    pub fn recompute_all(&mut self) -> Vec<String> {
        let max_iters = self.nodes.len() + 1;
        let mut all_changed = Vec::new();

        for _ in 0..max_iters {
            let mut changed_this_pass = false;
            let node_ids: Vec<String> = self.nodes.keys().cloned().collect();

            for node_id in &node_ids {
                let justs = self.justifications.get(node_id);
                let is_premise = justs.map_or(true, |v| v.is_empty());
                if is_premise {
                    continue;
                }

                let is_retracted = self.nodes.get(node_id)
                    .and_then(|n| n.metadata.get("_retracted"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if is_retracted {
                    continue;
                }

                let new_truth = self.compute_truth(node_id).to_string();
                if let Some(node) = self.nodes.get_mut(node_id) {
                    if node.truth_value != new_truth {
                        node.truth_value = new_truth;
                        all_changed.push(node_id.clone());
                        changed_this_pass = true;
                    }
                }
            }

            if !changed_this_pass {
                break;
            }
        }
        all_changed
    }

    pub fn entrenchment(&self, node_id: &str) -> i64 {
        let mut score: i64 = 0;
        let node = match self.nodes.get(node_id) {
            Some(n) => n,
            None => return 0,
        };

        let justs = self.justifications.get(node_id);
        let is_premise = justs.map_or(true, |v| v.is_empty());
        if is_premise {
            score += 100;
        }

        if !node.source.is_empty() {
            score += 50;
        }
        if !node.source_hash.is_empty() {
            score += 25;
        }

        let dep_count = self.dependents.get(node_id).map_or(0, |d| d.len());
        score += (dep_count as i64) * 10;

        let beliefs_type = node.metadata
            .get("beliefs_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        score += match beliefs_type {
            "AXIOM" | "WARNING" => 90,
            "OBSERVATION" => 80,
            "DERIVED" => 40,
            "PREDICTED" => 30,
            "NOTE" => 10,
            _ => 20,
        };

        score
    }

    pub fn find_premises(&self, node_id: &str) -> Vec<String> {
        let mut premises = Vec::new();
        let mut visited = HashSet::new();
        self.trace_premises(node_id, &mut premises, &mut visited);
        premises
    }

    fn trace_premises(&self, node_id: &str, premises: &mut Vec<String>, visited: &mut HashSet<String>) {
        if !visited.insert(node_id.to_string()) {
            return;
        }
        let justs = self.justifications.get(node_id);
        let is_premise = justs.map_or(true, |v| v.is_empty());
        if is_premise {
            premises.push(node_id.to_string());
            return;
        }
        if let Some(justs) = justs {
            for j in justs {
                for ant_id in &j.antecedents {
                    self.trace_premises(ant_id, premises, visited);
                }
            }
        }
    }

    pub fn find_culprits(&self, nogood_ids: &[String]) -> Vec<(String, i64)> {
        let mut all_premises = HashSet::new();
        for nid in nogood_ids {
            if let Some(node) = self.nodes.get(nid) {
                if node.truth_value == "IN" {
                    for p in self.find_premises(nid) {
                        all_premises.insert(p);
                    }
                }
            }
        }

        let mut candidates: Vec<(String, i64)> = all_premises
            .into_iter()
            .map(|p| {
                let score = self.entrenchment(&p);
                (p, score)
            })
            .collect();

        candidates.sort_by_key(|(_, score)| *score);
        candidates
    }
}

