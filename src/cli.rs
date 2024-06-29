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

    #[arg(long, env="UQ_GCS_KEY_ID")]
    pub gcs_key_id: Option<String>,

    #[arg(long, env="UQ_GCS_SECRET")]
    pub gcs_secret: Option<String>,
}

impl Options{
    pub fn init_query(&self) -> Option<String>{
        let key_opt = self.gcs_key_id.as_ref();
        let secret_opt = self.gcs_secret.as_ref();
        return match key_opt.zip(secret_opt) {
            Some((key,secret)) => {
                Some(format!("CREATE SECRET( TYPE GCS, KEY_ID '{key}', SECRET '{secret}');"))
            }
            _ => {None}
        }
    }
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


#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn init_query_empty() {
        let options : Options = Options{
            port: 8080, addr: "".to_string(),
            verbose: 3,
            gcs_key_id: None,
            gcs_secret: None,
        };
        assert!(options.init_query().is_none())
    }

    #[test]
    fn init_query_gcs() {
        let options : Options = Options{
            port: 8080, addr: "".to_string(),
            verbose: 3,
            gcs_key_id: Some("key_id".to_string()),
            gcs_secret:Some("secret".to_string())
        };
        assert_eq!(options.init_query().unwrap(), "CREATE SECRET( TYPE GCS, KEY_ID 'key_id', SECRET 'secret');");
    }
}