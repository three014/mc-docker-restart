use std::process::Output;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    process::Command as Process,
};

static BIND_ADDR: &str = "127.0.0.1:4086";

pub async fn launch() -> tokio::io::Result<()> {
    let listener = TcpListener::bind(BIND_ADDR).await?;
    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(handle_connection(socket));
    }
}

async fn handle_connection(mut socket: TcpStream) -> tokio::io::Result<()> {
    let mut buf = [0; 1];
    if socket.read(&mut buf).await? == 0 {
        return socket.write_all(b"Err: Empty message").await;
    }
    match TryInto::<Command>::try_into(&buf[..]) {
        Ok(command) => command.run(socket).await,
        Err(err) => socket.write_all(format!("Err: {err}").as_bytes()).await,
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
            0 => Ok(Command::Logs(false)),
            1 => Ok(Command::Logs(true)),
            2 => Ok(Command::Start),
            3 => Ok(Command::Stop),
            4 => Ok(Command::Restart),
            _ => Err("Invalid option"),
        }
    }
}

impl Command {
    pub async fn run(self, mut socket: TcpStream) -> tokio::io::Result<()> {
        match self {
            Command::Logs(follow) => {
                let args = ["logs", "lads-mc", "-f"];
                if follow {
                    let mut process = Process::new("docker").args(&args).spawn()?;
                    let stdout = process.stdout.take().ok_or(tokio::io::Error::last_os_error())?;
                    let mut reader = BufReader::new(stdout);
                    let mut buf = Vec::with_capacity(256);
                    while reader.read_buf(&mut buf).await? != 0 {
                        socket.write(&buf).await?;
                    }
                    process.wait().await?;
                    Ok(())
                } else {
                    let output = Self::docker(&args[..2]).await?;
                    socket.write_all(&output.stdout).await
                }
            }
            Command::Start => {
                let output = Self::docker(&["compose", "up", "lads-mc", "-d"]).await?;
                socket.write_all(&output.stdout).await
            }
            Command::Stop => {
                let output = Self::docker(&["compose", "stop", "lads-mc"]).await?;
                socket.write_all(&output.stdout).await
            }
            Command::Restart => {
                let output = Self::docker(&["compose", "restart", "lads-mc"]).await?;
                socket.write_all(&output.stdout).await
            }
        }
    }

    async fn docker(args: &[&str]) -> tokio::io::Result<Output> {
        Process::new("docker").args(args).output().await
    }
}
