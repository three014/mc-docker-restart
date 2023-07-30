use clap::{Args, Parser, Subcommand};
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    net::TcpStream, time::Duration,
};

static CONN_ADDR: &str = "127.0.0.1:4086";
const DEFAULT_CAPACITY: usize = 1024;

#[derive(Parser)]
#[command(author = "Aaron Perez", version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct McRemoteClientArgs {
    #[command(subcommand)]
    commands: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
struct LogsArgs {
    #[arg(short = 'f', default_value_t = false)]
    follow: bool,
}

impl McRemoteClientArgs {
    pub fn run(self) -> io::Result<()> {
        let mut stream = TcpStream::connect(CONN_ADDR)?;
        match self.commands {
            Commands::Logs(args) => {
                if args.follow {
                    stream.write(&[0])?;
                    let mut reader = BufReader::new(&stream);
                    let mut buf = String::with_capacity(DEFAULT_CAPACITY);
                    let mut stdout = io::stdout().lock();
                    buf.clear();
                    loop {
                        match reader.read_line(&mut buf).map_err(|e| e.kind()) {
                            Ok(n) => {
                                if n == 0 {
                                    break;
                                } else {
                                    write!(stdout, "{buf}")?;
                                }
                            }
                            Err(io::ErrorKind::WouldBlock) => std::thread::sleep(Duration::from_secs(2)),
                            Err(err) => return Err(io::Error::new(err, "Not good :D")),
                        }
                    }
                    writeln!(stdout)?;
                } else {
                    stream.write(&[1])?;
                    let mut buf = String::with_capacity(DEFAULT_CAPACITY);
                    stream.read_to_string(&mut buf)?;
                    println!("{buf}");
                }
            }
            Commands::Start => {
                stream.write(&[2])?;
                let mut buf = String::with_capacity(DEFAULT_CAPACITY);
                stream.read_to_string(&mut buf)?;
                println!("{buf}");
            }
            Commands::Stop => {
                stream.write(&[3])?;
                let mut buf = String::with_capacity(DEFAULT_CAPACITY);
                stream.read_to_string(&mut buf)?;
                println!("{buf}");
            }
            Commands::Restart => {
                stream.write(&[4])?;
                let mut buf = String::with_capacity(DEFAULT_CAPACITY);
                stream.read_to_string(&mut buf)?;
                println!("{buf}");
            }
        }
        Ok(println!("Done"))
    }
}
