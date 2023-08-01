use server::McRemoteServer;

fn main() -> tokio::io::Result<()> {
    init_logger();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async { McRemoteServer::bind(BIND_ADDR).await?.serve().await })
}

fn init_logger() {
    use std::io::Write;
    env_logger::builder().format(|buf, record| {
        writeln!(
            buf,
            "{} {:5} {:<28} {}",
            chrono::Local::now().format("%m/%d/%Y %H:%M:%S"),
            record.level(),
            record.module_path().unwrap_or("???"),
            record.args()
        )
    }).target(env_logger::Target::Stderr).init()
}

static BIND_ADDR: &str = "127.0.0.1:4086";
