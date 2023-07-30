use std::process::{Output, Stdio};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{tcp::OwnedWriteHalf, TcpListener, TcpStream},
    process::Command as Process,
};

static BIND_ADDR: &str = "127.0.0.1:4086";
static SERVER_NAME: &str = "lads-mc";

pub async fn launch() -> tokio::io::Result<()> {
    let listener = TcpListener::bind(BIND_ADDR).await?;
    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(handle_connection(socket));
    }
}

async fn handle_connection(socket: TcpStream) -> tokio::io::Result<()> {
    let (mut rx, mut tx) = socket.into_split();
    let mut buf = [0; 1];
    if rx.read(&mut buf).await? == 0 {
        return tx.write_all(b"Err: Empty message").await;
    }
    drop(rx);
    match TryInto::<Command>::try_into(&buf[..]) {
        Ok(command) => command.run(tx).await,
        Err(err) => tx.write_all(format!("Err: {err}").as_bytes()).await,
    }
}

enum Command {
    Logs(bool),
    Start,
    Stop,
    Restart,
}

impl TryFrom<&[u8]> for Command {
    type Error = &'static str;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err("Empty message");
        }
        value[0].try_into()
    }
}

impl TryFrom<u8> for Command {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Command::Logs(true)),
            1 => Ok(Command::Logs(false)),
            2 => Ok(Command::Start),
            3 => Ok(Command::Stop),
            4 => Ok(Command::Restart),
            _ => Err("Invalid option"),
        }
    }
}

impl Command {
    pub async fn run(self, mut tx: OwnedWriteHalf) -> tokio::io::Result<()> {
        match self {
            Command::Logs(follow) => {
                let args = ["logs", SERVER_NAME, "-f"];
                if follow {
                    let mut process = Process::new("docker")
                        .args(&args)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn()?;
                    let stdout = process
                        .stdout
                        .take()
                        .ok_or_else(tokio::io::Error::last_os_error)?;
                    let mut reader = BufReader::new(stdout);
                    let mut buf = String::with_capacity(1024);

                    loop {
                        buf.clear();
                        reader.read_line(&mut buf).await?;
                        let bytes_sent = tx.write(buf.as_bytes()).await?;
                        if bytes_sent == 0 {
                            break; // Client tcpstream closed
                        } else {
                            println!("Sent {bytes_sent} bytes to client!");
                        }
                    }

                    process.kill().await
                } else {
                    let output = Self::docker(&args[..2]).await?;
                    tx.write_all(&output.stdout).await
                }
            }
            Command::Start => {
                let output = Self::docker(&["compose", "up", SERVER_NAME, "-d"]).await?;
                tx.write_all(&output.stderr).await
            }
            Command::Stop => {
                let output = Self::docker(&["compose", "stop", SERVER_NAME]).await?;
                tx.write_all(&output.stderr).await
            }
            Command::Restart => {
                let output = Self::docker(&["compose", "restart", SERVER_NAME]).await?;
                tx.write_all(&output.stderr).await
            }
        }
    }

    async fn docker(args: &[&str]) -> tokio::io::Result<Output> {
        Process::new("docker").args(args).output().await
    }
}
