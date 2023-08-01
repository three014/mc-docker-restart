use client::McRemoteClient;

fn main() -> tokio::io::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(McRemoteClient::parse().connect())
}
