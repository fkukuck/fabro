use std::net::Ipv4Addr;

use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 7777)).await?;
    axum::serve(listener, fabro_sandboxd::build_router()).await?;
    Ok(())
}
