mod db;
mod types;
mod tms;
mod format;
mod commands;
mod mcp;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "reasons", about = "Truth Maintenance System for managing justified beliefs", version = concat!("(rust) ", env!("CARGO_PKG_VERSION")))]
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
        #[arg(long)]
        show_out: bool,
    },

    /// Simple substring search
    Lookup {
        query: String,
        #[arg(long)]
        show_out: bool,
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

    // --- Stubs: available in ftl-reasons (Python), require LLM / API keys ---

    #[command(hide = true)]
    Ask { query: String },
    #[command(hide = true)]
    Derive,
    #[command(hide = true)]
    Accept { file: PathBuf },
    #[command(hide = true)]
    Summarize { node_ids: Vec<String> },
    #[command(hide = true)]
    Compact,
    #[command(hide = true)]
    Deduplicate,
    #[command(name = "cluster-list", hide = true)]
    ClusterList,
    #[command(hide = true)]
    Verify { node_ids: Vec<String> },
    #[command(name = "list-negative", hide = true)]
    ListNegative,
    #[command(name = "review-beliefs", hide = true)]
    ReviewBeliefs,
    #[command(name = "review-justifications", hide = true)]
    ReviewJustifications,
    #[command(name = "review-premises", hide = true)]
    ReviewPremises,
    #[command(name = "repair-premises", hide = true)]
    RepairPremises,
    #[command(name = "propose-update", hide = true)]
    ProposeUpdate { node_id: String },
    #[command(name = "repair-smuggled", hide = true)]
    RepairSmuggled,
    #[command(hide = true)]
    Repair,
    #[command(hide = true)]
    Research,
    #[command(hide = true)]
    Contradictions,
    #[command(hide = true)]
    Report { node_id: String },
    #[command(name = "report-gated", hide = true)]
    ReportGated,

    // --- Stubs: external services ---

    #[command(name = "import-api", hide = true)]
    ImportApi,
    #[command(name = "export-api", hide = true)]
    ExportApi,
    #[command(name = "import-hf", hide = true)]
    ImportHf { repo: String },
    #[command(hide = true)]
    Pull { name: String },
    #[command(hide = true)]
    Publish,
    #[command(name = "import-agent", hide = true)]
    ImportAgent { agent: String },
    #[command(name = "sync-agent", hide = true)]
    SyncAgent { agent: String },

    // --- Stubs: source pinning / other ---

    #[command(name = "what-if", hide = true)]
    WhatIf { node_id: String },
    #[command(name = "add-repo", hide = true)]
    AddRepo { name: String, path: PathBuf },
    #[command(hide = true)]
    Repos,
    #[command(name = "trace-access-tags", hide = true)]
    TraceAccessTags { node_id: String },
    #[command(name = "export-card", hide = true)]
    ExportCard,
    #[command(name = "hash-sources", hide = true)]
    HashSources,
    #[command(name = "check-stale", hide = true)]
    CheckStale,
    #[command(name = "pin-sources", hide = true)]
    PinSources { node_ids: Vec<String> },
    #[command(name = "pin-update", hide = true)]
    PinUpdate { node_ids: Vec<String> },
    #[command(name = "pin-lines", hide = true)]
    PinLines { node_id: String },
    #[command(name = "search-sources", hide = true)]
    SearchSources { query: String },
    #[command(name = "list-gated", hide = true)]
    ListGated,
    #[command(hide = true)]
    Namespaces,
    #[command(hide = true)]
    Topics,
    #[command(name = "build-wiki", hide = true)]
    BuildWiki,
}

fn not_available(command: &str) {
    eprintln!("Warning: `reasons {}` is not available in reasons-rust.", command);
    eprintln!("This command requires Claude Code, Gemini CLI, or API keys.");
    eprintln!("Use the full ftl-reasons (Python) instead: pip install ftl-reasons");
}

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Ask { .. } => { not_available("ask"); Ok(()) }
        Commands::Derive => { not_available("derive"); Ok(()) }
        Commands::Accept { .. } => { not_available("accept"); Ok(()) }
        Commands::Summarize { .. } => { not_available("summarize"); Ok(()) }
        Commands::Compact => { not_available("compact"); Ok(()) }
        Commands::Deduplicate => { not_available("deduplicate"); Ok(()) }
        Commands::ClusterList => { not_available("cluster-list"); Ok(()) }
        Commands::Verify { .. } => { not_available("verify"); Ok(()) }
        Commands::ListNegative => { not_available("list-negative"); Ok(()) }
        Commands::ReviewBeliefs => { not_available("review-beliefs"); Ok(()) }
        Commands::ReviewJustifications => { not_available("review-justifications"); Ok(()) }
        Commands::ReviewPremises => { not_available("review-premises"); Ok(()) }
        Commands::RepairPremises => { not_available("repair-premises"); Ok(()) }
        Commands::ProposeUpdate { .. } => { not_available("propose-update"); Ok(()) }
        Commands::RepairSmuggled => { not_available("repair-smuggled"); Ok(()) }
        Commands::Repair => { not_available("repair"); Ok(()) }
        Commands::Research => { not_available("research"); Ok(()) }
        Commands::Contradictions => { not_available("contradictions"); Ok(()) }
        Commands::Report { .. } => { not_available("report"); Ok(()) }
        Commands::ReportGated => { not_available("report-gated"); Ok(()) }
        Commands::ImportApi => { not_available("import-api"); Ok(()) }
        Commands::ExportApi => { not_available("export-api"); Ok(()) }
        Commands::ImportHf { .. } => { not_available("import-hf"); Ok(()) }
        Commands::Pull { .. } => { not_available("pull"); Ok(()) }
        Commands::Publish => { not_available("publish"); Ok(()) }
        Commands::ImportAgent { .. } => { not_available("import-agent"); Ok(()) }
        Commands::SyncAgent { .. } => { not_available("sync-agent"); Ok(()) }
        Commands::WhatIf { .. } => { not_available("what-if"); Ok(()) }
        Commands::AddRepo { .. } => { not_available("add-repo"); Ok(()) }
        Commands::Repos => { not_available("repos"); Ok(()) }
        Commands::TraceAccessTags { .. } => { not_available("trace-access-tags"); Ok(()) }
        Commands::ExportCard => { not_available("export-card"); Ok(()) }
        Commands::HashSources => { not_available("hash-sources"); Ok(()) }
        Commands::CheckStale => { not_available("check-stale"); Ok(()) }
        Commands::PinSources { .. } => { not_available("pin-sources"); Ok(()) }
        Commands::PinUpdate { .. } => { not_available("pin-update"); Ok(()) }
        Commands::PinLines { .. } => { not_available("pin-lines"); Ok(()) }
        Commands::SearchSources { .. } => { not_available("search-sources"); Ok(()) }
        Commands::ListGated => { not_available("list-gated"); Ok(()) }
        Commands::Namespaces => { not_available("namespaces"); Ok(()) }
        Commands::Topics => { not_available("topics"); Ok(()) }
        Commands::BuildWiki => { not_available("build-wiki"); Ok(()) }

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
                Commands::Search { query, format, depth, show_out } => {
                    commands::query::cmd_search(&conn, query, format, *depth, *show_out)
                }
                Commands::Lookup { query, show_out } => commands::query::cmd_lookup(&conn, query, *show_out),
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
                _ => unreachable!(),
            }
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
