use clap::Parser;
use shared::{Action, Command};
use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    net::TcpStream,
};

static CONN_ADDR: &str = "127.0.0.1:4086";
const DEFAULT_CAPACITY: usize = 1024;

#[derive(Parser)]
#[command(author = "Aaron Perez", version, about, long_about = None)]
#[command(propagate_version = true)]
struct Args {
    #[command(subcommand)]
    commands: Action,
}

pub struct McRemoteClient {
    action: Command,
}

impl McRemoteClient {
    pub fn parse() -> Self {
        let args = Args::parse();
        Self {
            action: args.commands.into_command(),
        }
    }

    pub async fn connect(self) -> tokio::io::Result<()> {
        let (mut rx, mut tx) = TcpStream::connect(CONN_ADDR).await?.into_split();
        let mut buf = String::with_capacity(DEFAULT_CAPACITY);
        self.action.write_self(&mut tx).await?;
        match self.action.get() {
            Action::Logs(args) if args.follow() => {
                let reader = BufReader::new(rx);
                let cancel = tokio::spawn(tokio::signal::ctrl_c());
                let recv = tokio::spawn(Self::recv_data(buf, reader));

                cancel.await??;
                recv.abort();
                let _ = self.action.ctrl_c(&mut tx).await?;
                println!();
            }
            _ => {
                rx.read_to_string(&mut buf).await?;
                println!("{buf}");
            }
        }
        Ok(())
    }

    async fn recv_data<R>(mut buf: String, mut reader: R) -> tokio::io::Result<()>
    where
        R: AsyncBufReadExt + AsyncReadExt + Unpin,
    {
        loop {
            buf.clear();
            match reader.read_line(&mut buf).await.map_err(|e| e.kind()) {
                Ok(n) => {
                    if n == 0 {
                        break Ok(());
                    } else {
                        print!("{buf}");
                    }
                }
                Err(tokio::io::ErrorKind::WouldBlock) => {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Err(err) => return Err(tokio::io::Error::new(err, "Not good :D")),
            }
        }
    }
}
