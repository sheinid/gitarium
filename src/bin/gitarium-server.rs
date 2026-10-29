#[tokio::main]
async fn main() -> anyhow::Result<()> {
    gitarium::server::run().await
}
