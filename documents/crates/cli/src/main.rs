use std::process::ExitCode;

use clap::Parser;
use documents::{Documents, Viewer};
use documents_cli::{Agents, Command, formats};

/// Reads and edits Word, Excel, `PowerPoint` and PDF files as one more client of a live document.
#[derive(Parser)]
#[command(name = "documents", version)]
struct Arguments {
    /// The name that edits go under.
    #[arg(long, default_value = "Agent")]
    name: String,
    #[command(subcommand)]
    command: Command,
}

#[tokio::main]
async fn main() -> ExitCode {
    let arguments = Arguments::parse();
    let state = std::env::temp_dir().join("documents-cli");
    let documents = Documents::new(state, formats());
    let agents = Agents::new(documents.clone());
    let viewer = Viewer {
        id: arguments.name.clone(),
        name: arguments.name,
        can_edit: true,
    };
    let result = agents.run(&viewer, arguments.command).await;
    documents.close().await;
    match result {
        Ok(text) => {
            let mut out = std::io::stdout();
            drop(std::io::Write::write_all(&mut out, text.as_bytes()));
            ExitCode::SUCCESS
        }
        Err(problem) => {
            let mut out = std::io::stderr();
            drop(std::io::Write::write_all(
                &mut out,
                format!("{problem}\n").as_bytes(),
            ));
            ExitCode::FAILURE
        }
    }
}
