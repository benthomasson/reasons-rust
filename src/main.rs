mod db;
mod types;
mod tms;
mod format;
mod commands;
mod mcp;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "reasons", about = "Truth Maintenance System for managing justified beliefs", version)]
struct Cli {
    #[arg(long, default_value = "reasons.db", global = true)]
    db: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new reasons database
    Init,

    /// Show database summary
    Status,

    /// Show node details
    Show {
        node_id: String,
    },

    /// Explain why a node is IN or OUT
    Explain {
        node_id: String,
    },

    /// Full-text search with neighbor expansion
    Search {
        query: String,
        #[arg(long, default_value = "markdown")]
        format: String,
        #[arg(long, default_value_t = 1)]
        depth: usize,
    },

    /// Simple substring search
    Lookup {
        query: String,
    },

    /// List nodes with filters
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        premises: bool,
        #[arg(long)]
        has_dependents: bool,
        #[arg(long)]
        by_impact: bool,
    },

    /// Tree visualization of dependencies
    Tree {
        node_id: String,
        #[arg(long, default_value = "up")]
        direction: String,
        #[arg(long)]
        depth: Option<usize>,
    },

    /// Export as JSON
    Export {
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Export as markdown (beliefs.md format)
    ExportMarkdown {
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Add a belief node
    Add {
        node_id: String,
        text: String,
        #[arg(long)]
        sl: Option<String>,
        #[arg(long)]
        unless: Option<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        source_url: Option<String>,
        #[arg(long)]
        label: Option<String>,
    },

    /// Add a justification to an existing node
    AddJustification {
        node_id: String,
        #[arg(long)]
        sl: String,
        #[arg(long)]
        unless: Option<String>,
        #[arg(long)]
        label: Option<String>,
    },

    /// Remove a justification by index
    RemoveJustification {
        node_id: String,
        #[arg(long)]
        index: usize,
    },

    /// Retract a node (mark OUT with cascade)
    Retract {
        node_id: String,
        #[arg(long)]
        reason: Option<String>,
    },

    /// Re-assert a retracted node
    Assert {
        node_id: String,
    },

    /// Update node fields
    Update {
        node_id: String,
        #[arg(long)]
        text: Option<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        source_url: Option<String>,
    },

    /// Set metadata on a node
    SetMetadata {
        node_id: String,
        key: String,
        value: String,
    },

    /// Get metadata from a node
    GetMetadata {
        node_id: String,
        key: Option<String>,
    },

    /// Challenge a belief
    Challenge {
        target_id: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        challenge_id: Option<String>,
    },

    /// Defend a belief against a challenge
    Defend {
        target_id: String,
        #[arg(long)]
        challenge_id: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        defense_id: Option<String>,
    },

    /// Mark a belief as superseded by another
    Supersede {
        #[arg(long)]
        old_id: String,
        #[arg(long)]
        new_id: String,
    },

    /// Record a contradiction between nodes
    Nogood {
        node_ids: Vec<String>,
    },

    /// Find culprit premises for a set of contradicting nodes
    FindCulprits {
        node_ids: Vec<String>,
    },

    /// Import beliefs from markdown file
    ImportBeliefs {
        file: PathBuf,
    },

    /// Import from JSON file
    ImportJson {
        file: PathBuf,
    },

    /// Force full truth-value recomputation
    Propagate,

    /// Trace backward to find supporting premises
    Trace {
        node_id: String,
    },

    /// Convert a derived node to a premise
    ConvertToPremise {
        node_id: String,
    },

    /// Show propagation log
    Log {
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },

    /// Run as MCP server over stdio transport
    Mcp,
}

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Init => commands::manage::cmd_init(&cli.db),

        Commands::Mcp => {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(mcp::run_server(&cli.db))
        }

        _ => {
            let conn = if cli.db.exists() {
                match db::open_db(&cli.db) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed to open database: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                eprintln!("Database not found: {}. Run 'reasons init' first.", cli.db.display());
                std::process::exit(1);
            };

            match &cli.command {
                Commands::Init | Commands::Mcp => unreachable!(),
                Commands::Status => commands::manage::cmd_status(&conn),
                Commands::Show { node_id } => commands::query::cmd_show(&conn, node_id),
                Commands::Explain { node_id } => commands::query::cmd_explain(&conn, node_id),
                Commands::Search { query, format, depth } => {
                    commands::query::cmd_search(&conn, query, format, *depth)
                }
                Commands::Lookup { query } => commands::query::cmd_lookup(&conn, query),
                Commands::List { status, premises, has_dependents, by_impact } => {
                    commands::query::cmd_list(&conn, status.as_deref(), *premises, *has_dependents, *by_impact)
                }
                Commands::Tree { node_id, direction, depth } => {
                    commands::tree::cmd_tree(&conn, node_id, direction, *depth)
                }
                Commands::Export { output } => {
                    commands::export::cmd_export(&conn, output.as_deref())
                }
                Commands::ExportMarkdown { output } => {
                    commands::export::cmd_export_markdown(&conn, output.as_deref())
                }
                Commands::Add { node_id, text, sl, unless, source, source_url, label } => {
                    commands::add::cmd_add(&conn, node_id, text, sl.as_deref(), unless.as_deref(),
                        source.as_deref(), source_url.as_deref(), label.as_deref())
                }
                Commands::AddJustification { node_id, sl, unless, label } => {
                    commands::add::cmd_add_justification(&conn, node_id, sl, unless.as_deref(), label.as_deref())
                }
                Commands::RemoveJustification { node_id, index } => {
                    commands::add::cmd_remove_justification(&conn, node_id, *index)
                }
                Commands::Retract { node_id, reason } => {
                    commands::retract::cmd_retract(&conn, node_id, reason.as_deref())
                }
                Commands::Assert { node_id } => commands::retract::cmd_assert(&conn, node_id),
                Commands::Update { node_id, text, source, source_url } => {
                    commands::manage::cmd_update(&conn, node_id, text.as_deref(),
                        source.as_deref(), source_url.as_deref())
                }
                Commands::SetMetadata { node_id, key, value } => {
                    commands::manage::cmd_set_metadata(&conn, node_id, key, value)
                }
                Commands::GetMetadata { node_id, key } => {
                    commands::manage::cmd_get_metadata(&conn, node_id, key.as_deref())
                }
                Commands::Challenge { target_id, reason, challenge_id } => {
                    commands::challenge::cmd_challenge(&conn, target_id, reason, challenge_id.as_deref())
                }
                Commands::Defend { target_id, challenge_id, reason, defense_id } => {
                    commands::challenge::cmd_defend(&conn, target_id, challenge_id, reason, defense_id.as_deref())
                }
                Commands::Supersede { old_id, new_id } => {
                    commands::challenge::cmd_supersede(&conn, old_id, new_id)
                }
                Commands::Nogood { node_ids } => commands::nogood::cmd_nogood(&conn, node_ids),
                Commands::FindCulprits { node_ids } => {
                    commands::nogood::cmd_find_culprits(&conn, node_ids)
                }
                Commands::ImportBeliefs { file } => {
                    commands::import::cmd_import_beliefs(&conn, file)
                }
                Commands::ImportJson { file } => {
                    commands::import::cmd_import_json(&conn, file)
                }
                Commands::Propagate => commands::manage::cmd_propagate(&conn),
                Commands::Trace { node_id } => commands::manage::cmd_trace(&conn, node_id),
                Commands::ConvertToPremise { node_id } => {
                    commands::manage::cmd_convert_to_premise(&conn, node_id)
                }
                Commands::Log { limit } => commands::manage::cmd_log(&conn, *limit),
            }
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
