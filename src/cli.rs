use clap::Parser;

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Options {

    /// Port to listen on
    #[arg(default_value="8080", short, long,env="UQ_PORT")]
    pub port: u16,

    /// Address to listen on
    #[arg(default_value="0.0.0.0", short, long,env="UQ_ADDR")]
    pub addr: String,

    /// Verbose mode.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}


pub fn parse() -> Options {
    let opts = Options::parse();
    let debug_level = match opts.verbose {
        0 => tracing::Level::INFO,
        1 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    };

    tracing_subscriber::fmt().with_max_level(debug_level).init();
    opts
}