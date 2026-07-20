use std::path::PathBuf;

use clap::{Parser, Subcommand};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "agent", version, about = "AgentOS terminal client")]
pub struct Cli {
    #[arg(long, global = true, default_value = ".")]
    pub workspace: PathBuf,
    #[arg(long, global = true)]
    pub no_color: bool,
    #[command(subcommand)]
    pub command: CliCommand,
}

#[derive(Debug, Subcommand)]
pub enum CliCommand {
    Init,
    Chat,
    Run { goal: String },
    Status { session_id: Option<Uuid> },
    Sessions,
    Config,
    Resume { session_id: Option<Uuid> },
    Cancel { session_id: Option<Uuid> },
    Project,
    Profile { name: Option<String> },
    Tasks,
    History { query: Option<String> },
    Review,
    Plan { goal: String },
    Explain { target: String },
    Test { target: Option<String> },
    Fix { target: Option<String> },
    Refactor { target: String },
    Compact,
    Checkpoint { subcommand: Option<String>, name_or_id: Option<String> },
    Search { query: Vec<String> },
    Trace { function: String, depth: Option<usize> },
    Architecture,
    Permissions,
    Approve { arg: String },
    MemoryShow { scope: Option<String> },
    MemorySave { content: Vec<String> },
    MemoryClear { scope: String, confirm: bool },
    Knowledge,
    Learn { path: String, recursive: bool },
    TraceAgent { trace_id: Option<String> },
    Evaluate { trace_id: String },
    Benchmark { agent_id: Option<String> },
    DebugCmd { trace_id: String },
    Replay { trace_id: String },
    Score { agent_id: Option<String> },
    Commit,
    Pr,
    Tools,
    Memory,
}
