mod musync;

use std::{io, path::PathBuf, time::Instant};

use clap::Parser;
use colored::Colorize;
use musync::musync;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, help = "Directory to sync from")]
    src: PathBuf,
    #[arg(short, help = "Directory to sync to")]
    dst: PathBuf,
    #[arg(short, value_name = "JOBS", help = "Number of jobs to run in parallel")]
    #[clap(default_value = "16")]
    j: usize,
}

fn main() -> io::Result<()> {
    let instant = Instant::now();
    let cli = Cli::parse();
    eprintln!(
        "{}",
        r#" ___ ___  __ __  _____ __ __  ____     __ 
|   |   ||  |  |/ ___/|  |  ||    \   /  ]
| _   _ ||  |  (   \_ |  |  ||  _  | /  / 
|  \_/  ||  |  |\__  ||  ~  ||  |  |/  /  
|   |   ||  :  |/  \ ||___, ||  |  /   \_ 
|   |   ||     |\    ||     ||  |  \     |
|___|___| \__,_| \___||____/ |__|__|\____|"#
            .cyan()
    );
    musync(cli.src, cli.dst)?;
    eprintln!(
        "Finished in {}",
        instant.elapsed().as_secs_f32().to_string().green().bold()
    );
    Ok(())
}