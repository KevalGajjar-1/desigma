use desigma_mcp::server::DesigmaServer;
use rmcp::{transport::stdio, ServiceExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = match (args.next(), args.next(), args.next()) {
        (Some(flag), Some(path), None) if flag == "--workspace" => std::path::PathBuf::from(path),
        (Some(flag), None, None) if flag == "--help" => {
            eprintln!("desigma-mcp --workspace DIRECTORY\nLocal MCP server over stdio; edits are isolated until explicitly saved to a new file.");
            return Ok(());
        }
        _ => return Err("Usage: desigma-mcp --workspace DIRECTORY".into()),
    };
    let service = DesigmaServer::new(&root)?.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
