use clap::{Args, Subcommand};

pub const STOP_CODE: u8 = 255;

pub struct Command {
    command: Action,
}

#[derive(Subcommand)]
pub enum Action {
    /// Checks the logs of the minecraft server.
    Logs(LogsArgs),

    /// Starts the minecraft server.
    Start,

    /// Stops the minecraft server.
    Stop,

    /// Restarts the minecraft server.
    Restart,
}

#[derive(Args)]
pub struct LogsArgs {
    #[arg(short = 'f', long, default_value_t = false)]
    follow: bool,
}

impl LogsArgs {
    pub fn follow(&self) -> bool {
        self.follow
    }
}

impl TryFrom<&[u8]> for Action {
    type Error = &'static str;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err("Empty message");
        }
        value[0].try_into()
    }
}

impl TryFrom<u8> for Action {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Action::Logs(LogsArgs { follow: true })),
            1 => Ok(Action::Logs(LogsArgs { follow: false })),
            2 => Ok(Action::Start),
            3 => Ok(Action::Stop),
            4 => Ok(Action::Restart),
            _ => Err("Invalid option"),
        }
    }
}

impl Action {
    fn as_u8(&self) -> u8 {
        match self {
            Self::Logs(args) => !args.follow as u8,
            Self::Start => 2,
            Self::Stop => 3,
            Self::Restart => 4,
        }
    }

    pub fn into_command(self) -> Command {
        Command { command: self }
    }
}

impl Command {
    pub async fn write_self<'a, W>(&self, writer: &mut W) -> tokio::io::Result<usize>
    where
        W: tokio::io::AsyncWriteExt + Unpin + 'a,
    {
        writer.write(&[self.command.as_u8()]).await
    }

    pub fn get(&self) -> &Action {
        &self.command
    }

    pub async fn ctrl_c<'a, W>(&self, writer: &mut W) -> tokio::io::Result<usize>
    where
        W: tokio::io::AsyncWriteExt + Unpin + 'a
    {
        writer.write(&[STOP_CODE]).await
    }

    pub async fn try_from_reader<'a, R>(mut reader: R) -> Result<Self, String>
    where
        R: tokio::io::AsyncReadExt + Unpin + 'a,
    {
        let mut buf = [0; 1];
        if reader.read(&mut buf).await.map_err(|e| e.to_string())? == 0 {
            Err("empty message".to_owned())
        } else {
            buf[..]
                .try_into()
                .map_err(|err: &'static str| err.to_string())
        }
    }
}

impl TryFrom<&[u8]> for Command {
    type Error = &'static str;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Ok(Command {
            command: value.try_into()?,
        })
    }
}
