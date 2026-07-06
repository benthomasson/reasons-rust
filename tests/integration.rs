use std::process::Command;
use tempfile::TempDir;

fn reasons(dir: &TempDir) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_reasons"));
    cmd.current_dir(dir.path());
    cmd
}

fn run(dir: &TempDir, args: &[&str]) -> String {
    let output = reasons(dir)
        .args(args)
        .output()
        .expect("failed to run reasons");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        panic!("reasons {} failed:\nstdout: {}\nstderr: {}", args.join(" "), stdout, stderr);
    }
    stdout
}

#[test]
fn init_creates_db_and_status_reads_it() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    assert!(dir.path().join("reasons.db").exists());

    let status = run(&dir, &["status"]);
    assert!(status.contains("Nodes: 0"));
    assert!(status.contains("Premises: 0"));
}

#[test]
fn add_and_show_round_trip() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "fact-1", "The earth is round", "--source", "science"]);

    let show = run(&dir, &["show", "fact-1"]);
    assert!(show.contains("fact-1 [IN]"));
    assert!(show.contains("The earth is round"));
    assert!(show.contains("Source: science"));
}

#[test]
fn add_derived_with_justification() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "a", "Premise A"]);
    run(&dir, &["add", "b", "Premise B"]);
    run(&dir, &["add", "c", "Derived from A and B", "--sl", "a,b"]);

    let show = run(&dir, &["show", "c"]);
    assert!(show.contains("c [IN]"));
    assert!(show.contains("SL: a, b"));

    let explain = run(&dir, &["explain", "c"]);
    assert!(explain.contains("c is IN because"));
    assert!(explain.contains("a is IN (premise)"));
    assert!(explain.contains("b is IN (premise)"));
}

#[test]
fn retract_cascade_and_restore() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "a", "Premise"]);
    run(&dir, &["add", "b", "Derived from A", "--sl", "a"]);
    run(&dir, &["add", "c", "Derived from B", "--sl", "b"]);

    let retract_out = run(&dir, &["retract", "a", "--reason", "testing"]);
    assert!(retract_out.contains("Retracted a"));
    assert!(retract_out.contains("Cascaded"));

    let list = run(&dir, &["list"]);
    assert!(list.contains("[OUT] a"));
    assert!(list.contains("[OUT] b"));
    assert!(list.contains("[OUT] c"));

    run(&dir, &["assert", "a"]);
    let list2 = run(&dir, &["list"]);
    assert!(list2.contains("[IN] a"));
    assert!(list2.contains("[IN] b"));
    assert!(list2.contains("[IN] c"));
}

#[test]
fn search_finds_nodes() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "cat-fact", "Cats sleep sixteen hours per day"]);
    run(&dir, &["add", "dog-fact", "Dogs are loyal companions"]);

    let results = run(&dir, &["search", "cats sleep"]);
    assert!(results.contains("cat-fact"));
}

#[test]
fn lookup_finds_by_substring() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "x-1", "Quantum entanglement is spooky"]);

    let results = run(&dir, &["lookup", "entanglement"]);
    assert!(results.contains("x-1"));
    assert!(results.contains("Quantum"));
}

#[test]
fn list_with_filters() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "p1", "Premise 1"]);
    run(&dir, &["add", "p2", "Premise 2"]);
    run(&dir, &["add", "d1", "Derived", "--sl", "p1,p2"]);
    run(&dir, &["retract", "p1"]);

    let premises = run(&dir, &["list", "--premises"]);
    assert!(premises.contains("p1") || premises.contains("p2"));
    assert!(!premises.contains("d1"));

    let in_only = run(&dir, &["list", "--status", "IN"]);
    assert!(in_only.contains("p2"));
    assert!(!in_only.contains("[OUT]"));
}

#[test]
fn tree_up_direction() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "root", "Root premise"]);
    run(&dir, &["add", "mid", "Middle node", "--sl", "root"]);
    run(&dir, &["add", "leaf", "Leaf node", "--sl", "mid"]);

    let tree = run(&dir, &["tree", "leaf", "--direction", "up"]);
    assert!(tree.contains("leaf [IN]"));
    assert!(tree.contains("mid [IN]"));
    assert!(tree.contains("root [IN]"));
    assert!(tree.contains("(premise)"));
}

#[test]
fn tree_down_direction() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "root", "Root premise"]);
    run(&dir, &["add", "child", "Child node", "--sl", "root"]);

    let tree = run(&dir, &["tree", "root", "--direction", "down"]);
    assert!(tree.contains("root [IN]"));
    assert!(tree.contains("child [IN]"));
}

#[test]
fn export_import_json_round_trip() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "a", "Fact A", "--source", "src"]);
    run(&dir, &["add", "b", "Fact B"]);
    run(&dir, &["add", "c", "Derived", "--sl", "a,b"]);

    let export_path = dir.path().join("export.json");
    run(&dir, &["export", "-o", export_path.to_str().unwrap()]);
    assert!(export_path.exists());

    // Re-init and import
    std::fs::remove_file(dir.path().join("reasons.db")).unwrap();
    run(&dir, &["init"]);
    run(&dir, &["import-json", export_path.to_str().unwrap()]);

    let list = run(&dir, &["list"]);
    assert!(list.contains("a"));
    assert!(list.contains("b"));
    assert!(list.contains("c"));

    let show = run(&dir, &["show", "a"]);
    assert!(show.contains("Source: src"));
}

#[test]
fn export_import_markdown_round_trip() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "p1", "A premise"]);
    run(&dir, &["add", "d1", "A derived belief", "--sl", "p1"]);

    let md_path = dir.path().join("beliefs.md");
    run(&dir, &["export-markdown", "-o", md_path.to_str().unwrap()]);
    assert!(md_path.exists());

    std::fs::remove_file(dir.path().join("reasons.db")).unwrap();
    run(&dir, &["init"]);
    run(&dir, &["import-beliefs", md_path.to_str().unwrap()]);

    let list = run(&dir, &["list"]);
    assert!(list.contains("p1"));
    assert!(list.contains("d1"));
}

#[test]
fn challenge_and_defend_cycle() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "claim", "The sky is blue"]);

    run(&dir, &["challenge", "claim", "--reason", "It might be grey"]);
    let list1 = run(&dir, &["list"]);
    assert!(list1.contains("[OUT] claim"));

    run(&dir, &["defend", "claim", "--challenge-id", "challenge-claim",
        "--reason", "It is usually blue"]);
    let list2 = run(&dir, &["list"]);
    assert!(list2.contains("[IN] claim"));
}

#[test]
fn supersede_makes_old_out() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "old-claim", "Version 1 of the claim"]);
    run(&dir, &["add", "new-claim", "Version 2 of the claim"]);

    run(&dir, &["supersede", "--old-id", "old-claim", "--new-id", "new-claim"]);
    let list = run(&dir, &["list"]);
    assert!(list.contains("[OUT] old-claim"));
    assert!(list.contains("[IN] new-claim"));
}

#[test]
fn nogood_retracts_least_entrenched() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "weak", "Weakly held belief"]);
    run(&dir, &["add", "strong", "Strongly held belief", "--source", "authoritative-doc"]);

    run(&dir, &["nogood", "weak", "strong"]);
    let list = run(&dir, &["list"]);
    assert!(list.contains("[OUT] weak"));
    assert!(list.contains("[IN] strong"));
}

#[test]
fn find_culprits_lists_candidates() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "a", "Claim A"]);
    run(&dir, &["add", "b", "Claim B", "--source", "doc"]);

    let output = run(&dir, &["find-culprits", "a", "b"]);
    assert!(output.contains("Culprit premises"));
    assert!(output.contains("a")); // less entrenched, should appear first
}

#[test]
fn update_changes_fields() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "x", "Original text"]);

    run(&dir, &["update", "x", "--text", "Updated text", "--source", "new-source"]);
    let show = run(&dir, &["show", "x"]);
    assert!(show.contains("Updated text"));
    assert!(show.contains("Source: new-source"));
}

#[test]
fn set_and_get_metadata() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "x", "A node"]);

    run(&dir, &["set-metadata", "x", "custom_key", "custom_value"]);
    let meta = run(&dir, &["get-metadata", "x", "custom_key"]);
    assert!(meta.contains("custom_value"));
}

#[test]
fn propagate_fixes_truth_values() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "a", "Premise"]);
    run(&dir, &["add", "b", "Derived", "--sl", "a"]);

    // Propagate should report no changes when already correct
    let output = run(&dir, &["propagate"]);
    assert!(output.contains("No changes"));
}

#[test]
fn trace_finds_premises() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "p1", "First premise"]);
    run(&dir, &["add", "p2", "Second premise"]);
    run(&dir, &["add", "d1", "Mid-level", "--sl", "p1,p2"]);
    run(&dir, &["add", "d2", "Top-level", "--sl", "d1"]);

    let trace = run(&dir, &["trace", "d2"]);
    assert!(trace.contains("p1"));
    assert!(trace.contains("p2"));
}

#[test]
fn convert_to_premise_strips_justifications() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "a", "Premise"]);
    run(&dir, &["add", "b", "Derived", "--sl", "a"]);

    run(&dir, &["convert-to-premise", "b"]);
    let show = run(&dir, &["show", "b"]);
    assert!(!show.contains("Justifications:"));
}

#[test]
fn log_shows_entries() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "x", "Test"]);
    run(&dir, &["retract", "x"]);

    let log = run(&dir, &["log"]);
    assert!(log.contains("retract"));
    assert!(log.contains("x"));
}

#[test]
fn status_after_operations() {
    let dir = TempDir::new().unwrap();
    run(&dir, &["init"]);
    run(&dir, &["add", "a", "A"]);
    run(&dir, &["add", "b", "B"]);
    run(&dir, &["add", "c", "C", "--sl", "a,b"]);
    run(&dir, &["retract", "a"]);

    let status = run(&dir, &["status"]);
    assert!(status.contains("Nodes: 3"));
    assert!(status.contains("Premises: 2"));
    assert!(status.contains("Derived: 1"));
}
