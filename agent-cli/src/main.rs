use std::io::{self, BufRead, IsTerminal};
use std::sync::Arc;

use agent_cli::{
    run_tui, tui_approval_channel, Cli, CliApplication, CliCommand, CliConfig, EmbeddedAgentClient,
    HttpAgentClient, ProfessionalApplication, TerminalAgentClient, TerminalRenderer, TuiOptions,
};
use clap::Parser;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

async fn run() -> agent_cli::CliResult<()> {
    let cli = Cli::parse();
    if matches!(cli.command, CliCommand::Init) {
        CliConfig::initialize(&cli.workspace)?;
        println!("Initialized {}", cli.workspace.join(".agent").display());
        return Ok(());
    }

    let config = CliConfig::load(&cli.workspace).await?;
    let use_tui = matches!(&cli.command, CliCommand::Chat)
        && !cli.no_color
        && io::stdin().is_terminal()
        && io::stdout().is_terminal();
    let (tui_approval, mut tui_receiver) = if use_tui {
        let (approval, receiver) = tui_approval_channel();
        (Some(approval), Some(receiver))
    } else {
        (None, None)
    };
    let client: Arc<dyn TerminalAgentClient> = if config.server.mode == "remote" {
        let url = config.server.url.as_deref().ok_or_else(|| {
            agent_cli::CliError::Configuration("remote mode requires server.url".into())
        })?;
        Arc::new(HttpAgentClient::new(url)?)
    } else {
        Arc::new(if let Some(approval) = tui_approval {
            EmbeddedAgentClient::open_with_approval(&cli.workspace, &config, approval).await?
        } else {
            EmbeddedAgentClient::open(&cli.workspace, &config).await?
        })
    };
    let color = !cli.no_color && io::stdout().is_terminal();
    let tui_options = TuiOptions::new(
        &cli.workspace,
        cli.workspace
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("workspace"),
        config.model.name.clone(),
        config.permissions.mode.clone(),
    );
    let professional: Arc<ProfessionalApplication<dyn TerminalAgentClient>> =
        Arc::new(ProfessionalApplication::new(&cli.workspace, client.clone()));
    let application: Arc<CliApplication<dyn TerminalAgentClient, TerminalRenderer>> = Arc::new(
        CliApplication::new(&cli.workspace, config, client, TerminalRenderer::new(color)),
    );
    match cli.command {
        CliCommand::Init => unreachable!(),
        CliCommand::Run { goal } => print_output(application.run(goal).await?),
        CliCommand::Status { session_id } => print_output(application.status(session_id).await?),
        CliCommand::Sessions => print_output(application.sessions().await?),
        CliCommand::Config => print_output(application.config()?),
        CliCommand::Resume { session_id } => print_output(application.resume(session_id).await?),
        CliCommand::Cancel { session_id } => print_output(application.cancel(session_id).await?),
        CliCommand::Project => print_lines(professional.execute_line("/project").await?),
        CliCommand::Profile { name } => {
            print_lines(professional.execute_line(&slash("profile", name)).await?)
        }
        CliCommand::Tasks => print_lines(professional.execute_line("/tasks").await?),
        CliCommand::History { query } => {
            print_lines(professional.execute_line(&slash("history", query)).await?)
        }
        CliCommand::Review => print_lines(professional.execute_line("/review").await?),
        CliCommand::Plan { goal } => {
            print_lines(professional.execute_line(&slash("plan", [goal])).await?)
        }
        CliCommand::Explain { target } => print_lines(
            professional
                .execute_line(&slash("explain", [target]))
                .await?,
        ),
        CliCommand::Test { target } => {
            print_lines(professional.execute_line(&slash("test", target)).await?)
        }
        CliCommand::Fix { target } => {
            print_lines(professional.execute_line(&slash("fix", target)).await?)
        }
        CliCommand::Refactor { target } => print_lines(
            professional
                .execute_line(&slash("refactor", [target]))
                .await?,
        ),
        CliCommand::Commit => print_lines(professional.execute_line("/commit").await?),
        CliCommand::Pr => print_lines(professional.execute_line("/pr").await?),
        CliCommand::Tools => print_lines(professional.execute_line("/tools").await?),
        CliCommand::Memory => print_lines(professional.execute_line("/memory").await?),
        CliCommand::Compact => print_lines(professional.execute_line("/compact").await?),
        CliCommand::Checkpoint { subcommand, name_or_id } => {
            let line = match (subcommand, name_or_id) {
                (Some(cmd), Some(arg)) => format!("/checkpoint {cmd} {arg}"),
                (Some(cmd), None) => format!("/checkpoint {cmd}"),
                (None, _) => "/checkpoint".into(),
            };
            print_lines(professional.execute_line(&line).await?)
        }
        CliCommand::Search { query } => {
            let line = format!("/search {}", query.join(" "));
            print_lines(professional.execute_line(&line).await?)
        }
        CliCommand::Trace { function, depth } => {
            let line = match depth {
                Some(d) => format!("/trace {function} --depth {d}"),
                None => format!("/trace {function}"),
            };
            print_lines(professional.execute_line(&line).await?)
        }
        CliCommand::Architecture => {
            print_lines(professional.execute_line("/architecture").await?)
        }
        CliCommand::Permissions => {
            print_lines(professional.execute_line("/permissions").await?)
        }
        CliCommand::Approve { arg } => {
            print_lines(professional.execute_line(&format!("/approve {arg}")).await?)
        }
        CliCommand::MemoryShow { scope } => {
            let line = match scope {
                Some(s) => format!("/memory-show {s}"),
                None => "/memory-show".into(),
            };
            print_lines(professional.execute_line(&line).await?)
        }
        CliCommand::MemorySave { content } => {
            let line = format!("/memory-save {}", content.join(" "));
            print_lines(professional.execute_line(&line).await?)
        }
        CliCommand::MemoryClear { scope, confirm } => {
            let line = if confirm {
                format!("/memory-clear {scope} --confirm")
            } else {
                format!("/memory-clear {scope}")
            };
            print_lines(professional.execute_line(&line).await?)
        }
        CliCommand::Knowledge => {
            print_lines(professional.execute_line("/knowledge").await?)
        }
        CliCommand::Learn { path, recursive } => {
            let line = if recursive {
                format!("/learn {path} --recursive")
            } else {
                format!("/learn {path}")
            };
            print_lines(professional.execute_line(&line).await?)
        }
        CliCommand::TraceAgent { trace_id } => {
            let line = match trace_id {
                Some(id) => format!("/trace-agent {id}"),
                None => "/trace-agent".into(),
            };
            print_lines(professional.execute_line(&line).await?)
        }
        CliCommand::Evaluate { trace_id } => {
            print_lines(professional.execute_line(&format!("/evaluate {trace_id}")).await?)
        }
        CliCommand::Benchmark { agent_id } => {
            let line = match agent_id {
                Some(id) => format!("/benchmark {id}"),
                None => "/benchmark".into(),
            };
            print_lines(professional.execute_line(&line).await?)
        }
        CliCommand::DebugCmd { trace_id } => {
            print_lines(professional.execute_line(&format!("/debug {trace_id}")).await?)
        }
        CliCommand::Replay { trace_id } => {
            print_lines(professional.execute_line(&format!("/replay {trace_id}")).await?)
        }
        CliCommand::Score { agent_id } => {
            let line = match agent_id {
                Some(id) => format!("/score {id}"),
                None => "/score".into(),
            };
            print_lines(professional.execute_line(&line).await?)
        }
        CliCommand::Chat => {
            application.begin_chat()?;
            if use_tui {
                run_tui(
                    tui_options,
                    application,
                    professional,
                    tui_receiver
                        .take()
                        .expect("TUI approval receiver must be initialized"),
                )
                .await?;
                return Ok(());
            }
            print_lines(professional.execute_line("/project").await?);
            for line in application.header() {
                println!("{line}");
            }
            println!("Type /exit to leave the session.");
            for line in io::stdin().lock().lines() {
                let line = line?;
                if line.trim() == "/exit" {
                    break;
                }
                if line.trim().is_empty() {
                    continue;
                }
                if line.starts_with('/') {
                    print_lines(professional.execute_line(&line).await?);
                } else {
                    print_output(application.chat(line).await?);
                }
            }
        }
    }
    Ok(())
}

fn print_output(output: agent_cli::CommandOutput) {
    print_lines(output.lines);
}

fn print_lines(lines: Vec<String>) {
    for line in lines {
        println!("{line}");
    }
}

fn slash(name: &str, arguments: impl IntoIterator<Item = String>) -> String {
    let mut line = format!("/{name}");
    for argument in arguments {
        line.push(' ');
        line.push('"');
        for character in argument.chars() {
            if matches!(character, '\\' | '"') {
                line.push('\\');
            }
            line.push(character);
        }
        line.push('"');
    }
    line
}
