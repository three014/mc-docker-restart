use std::io;

use clap::Parser;
use client::McRemoteClientArgs;

fn main() -> io::Result<()> {
    McRemoteClientArgs::parse().run()
}
