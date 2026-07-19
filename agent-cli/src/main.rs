use std::io::{self, BufRead, IsTerminal};
use std::sync::Arc;

use agent_cli::{
    Cli, CliApplication, CliCommand, CliConfig, EmbeddedAgentClient, HttpAgentClient,
    ProfessionalApplication, TerminalAgentClient, TerminalRenderer,
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

    let config = CliConfig::load(&cli.workspace)?;
    let client: Arc<dyn TerminalAgentClient> = if config.server.mode == "remote" {
        let url = config.server.url.as_deref().ok_or_else(|| {
            agent_cli::CliError::Configuration("remote mode requires server.url".into())
        })?;
        Arc::new(HttpAgentClient::new(url)?)
    } else {
        Arc::new(EmbeddedAgentClient::open(&cli.workspace, &config).await?)
    };
    let color = !cli.no_color && io::stdout().is_terminal();
    let professional = ProfessionalApplication::new(&cli.workspace, client.clone());
    let application =
        CliApplication::new(&cli.workspace, config, client, TerminalRenderer::new(color));
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
        CliCommand::Chat => {
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
