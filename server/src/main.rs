use server::McRemoteServer;

fn main() -> tokio::io::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async { McRemoteServer::bind(BIND_ADDR).await?.serve().await })
}

static BIND_ADDR: &str = "127.0.0.1:4086";
