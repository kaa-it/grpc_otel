#[tokio::main]
async fn main() -> anyhow::Result<()> {
    client::run().await?;

    Ok(())
}
