use clap::Parser;
use tracing::metadata::LevelFilter;
use tracing_subscriber::EnvFilter;

pub const UQ_ATTACHED_DB_NAME: &str = "uquery_attached_db";
const UQ_CREATE_AWS_CREDENTIAL_CHAIN: &str = "CREATE SECRET aws_secret ( TYPE S3, PROVIDER CREDENTIAL_CHAIN);";
const UQ_START_UI_SERVER: &str = "CALL start_ui_server();";


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

    /// Google Clous Storage Key ID
    #[arg(long, env="UQ_GCS_KEY_ID")]
    pub gcs_key_id: Option<String>,

    /// Google Clous Storage Secret
    #[arg(long, env="UQ_GCS_SECRET")]
    pub gcs_secret: Option<String>,

    /// DuckDB database file to attach in read only mode and use as default
    #[arg(short, long, env="UQ_DB_FILE")]
    pub db_file: Option<String>,

    /// Enabled permissive CORS
    #[arg(short, long, env="UQ_CORS_ENABLED")]
    pub cors_enabled: bool,

    /// Enable AWS Credential Chain
    #[arg(long, env="UQ_AWS_CREDENTIAL_CHAIN")]
    pub aws_credential_chain: bool,

    /// Enable DuckDB UI Proxy
    #[arg(long, env="UQ_UI_PROXY")]
    pub duckdb_ui: bool,

    /// DuckDB UI Port
    #[arg(default_value="14213",long, env="UQ_UI_PORT")]
    pub duckdb_ui_port: u16,
}

impl Options{
    pub fn init_script(&self) -> Vec<String>{
        let key_opt = self.gcs_key_id.as_ref();
        let secret_opt = self.gcs_secret.as_ref();
        let db_file_opt = self.db_file.as_ref();
        let mut init_script = Vec::new();

        if let (Some(key), Some(secret)) = (key_opt, secret_opt){
            init_script.push(format!("CREATE SECRET gcs_secret ( TYPE GCS, KEY_ID '{key}', SECRET '{secret}');"));
        }

        if self.aws_credential_chain{
            init_script.push(UQ_CREATE_AWS_CREDENTIAL_CHAIN.to_string());
        }

        if let Some(db_file) = db_file_opt{
            init_script.push(format!("ATTACH '{db_file}' as {UQ_ATTACHED_DB_NAME} (READ_ONLY);"));
        }

        if self.duckdb_ui{
            init_script.push(UQ_START_UI_SERVER.to_string());
        }

        init_script
    }
}


pub fn parse() -> Options {
    let opts = Options::parse();
    let debug_level = match opts.verbose {
        0 => tracing::Level::INFO,
        1 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("pingora_core=off,pingora_pool=off,pingora_proxy=off")
            .add_directive(LevelFilter::from(debug_level).into()))
        .init();
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
            db_file: None,
            cors_enabled: false,
            aws_credential_chain: false,
            duckdb_ui: false,
            duckdb_ui_port: 14213,
        };
        assert!(options.init_script().is_empty())
    }

    #[test]
    fn init_query_gcs() {
        let options : Options = Options{
            port: 8080, addr: "".to_string(),
            verbose: 3,
            gcs_key_id: Some("key_id".to_string()),
            gcs_secret:Some("secret".to_string()),
            db_file: None,
            cors_enabled: false,
            aws_credential_chain: false,
            duckdb_ui: false,
            duckdb_ui_port: 14213,
        };
        assert_eq!(options.init_script()[0], "CREATE SECRET gcs_secret ( TYPE GCS, KEY_ID 'key_id', SECRET 'secret');");
    }

    #[test]
    fn init_query_aws() {
        let options : Options = Options{
            port: 8080, addr: "".to_string(),
            verbose: 3,
            gcs_key_id: None,
            gcs_secret:None,
            db_file: None,
            cors_enabled: false,
            aws_credential_chain: true,
            duckdb_ui: false,
            duckdb_ui_port: 14213,
        };
        assert_eq!(options.init_script()[0], UQ_CREATE_AWS_CREDENTIAL_CHAIN);
    }

    #[test]
    fn init_query_was_gcs() {
        let options : Options = Options{
            port: 8080, addr: "".to_string(),
            verbose: 3,
            gcs_key_id: Some("key_id2".to_string()),
            gcs_secret:Some("secret2".to_string()),
            db_file: None,
            cors_enabled: false,
            aws_credential_chain: true,
            duckdb_ui: false,
            duckdb_ui_port: 14213,
        };
        assert_eq!(options.init_script()[0], "CREATE SECRET gcs_secret ( TYPE GCS, KEY_ID 'key_id2', SECRET 'secret2');");
        assert_eq!(options.init_script()[1], UQ_CREATE_AWS_CREDENTIAL_CHAIN);
    }

    #[test]
    fn init_duckdb_ui() {
        let options : Options = Options{
            port: 8080, addr: "".to_string(),
            verbose: 3,
            gcs_key_id: None,
            gcs_secret: None,
            db_file: None,
            cors_enabled: false,
            aws_credential_chain: false,
            duckdb_ui: true,
            duckdb_ui_port: 14213,
        };
        assert_eq!(options.init_script()[0], UQ_START_UI_SERVER);
    }
}