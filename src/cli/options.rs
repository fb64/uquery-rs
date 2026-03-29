use std::env;
use clap::Parser;
use tracing::metadata::LevelFilter;
use tracing::warn;
use tracing_subscriber::EnvFilter;

/// Attached database name
pub const UQ_ATTACHED_DB_NAME: &str = "uquery_attached_db";

/// Enable the provider credential chain for AWS
const UQ_CREATE_AWS_CREDENTIAL_CHAIN: &str =
    "CREATE SECRET aws_secret (TYPE S3, PROVIDER CREDENTIAL_CHAIN);";

/// Configure GCS community extension to use the credential chain provider for GCP and enable gRPC
const UQ_CREATE_GCP_CREDENTIAL_CHAIN: &str = r#"INSTALL gcs from community;
LOAD gcs;
SET gcs_enable_grpc=true;
CREATE SECRET gcp_secret (TYPE gcp, PROVIDER credential_chain);"#;

/// Start DuckDB UI
const UQ_START_UI_SERVER: &str = "CALL start_ui_server();";

/// Cloud allowed directory prefixes
const CLOUD_PREFIXES: &[&str] = &["gcs://", "gs://", "gcss://", "s3://"];

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Options {
    /// Port to listen on
    #[arg(default_value = "8080", short, long, env = "UQ_PORT")]
    pub port: u16,

    /// Address to listen on
    #[arg(default_value = "0.0.0.0", short, long, env = "UQ_ADDR")]
    pub addr: String,

    /// Verbose mode.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Google Cloud Storage Key ID
    #[arg(long, env = "UQ_GCS_KEY_ID")]
    pub gcs_key_id: Option<String>,

    /// Google Cloud Storage Secret
    #[arg(long, env = "UQ_GCS_SECRET")]
    pub gcs_secret: Option<String>,

    /// Enable GCS Credential Chain
    #[arg(long, env = "UQ_GCS_CREDENTIAL_CHAIN")]
    pub gcs_credential_chain: bool,

    /// DuckDB database file to attach in read only mode and use as default
    #[arg(short, long, env = "UQ_DB_FILE")]
    pub db_file: Option<String>,

    /// Enabled permissive CORS
    #[arg(short, long, env = "UQ_CORS_ENABLED")]
    pub cors_enabled: bool,

    /// Enable AWS Credential Chain
    #[arg(long, env = "UQ_AWS_CREDENTIAL_CHAIN")]
    pub aws_credential_chain: bool,

    /// Enable DuckDB UI Proxy
    #[arg(long, env = "UQ_UI_PROXY")]
    pub duckdb_ui: bool,

    /// DuckDB UI Port
    #[arg(default_value = "14213", long, env = "UQ_UI_PORT")]
    pub duckdb_ui_port: u16,

    /// Iceberg Catalog Endpoint
    #[arg(long, env = "UQ_ICEBERG_CATALOG_ENDPOINT")]
    pub ic_catalog_endpoint: Option<String>,

    /// Iceberg Catalog name
    #[arg(long, env = "UQ_ICEBERG_CATALOG_NAME")]
    pub ic_catalog_name: Option<String>,

    /// Iceberg User
    #[arg(long, env = "UQ_ICEBERG_USER")]
    pub ic_user: Option<String>,

    #[arg(long, env = "UQ_ICEBERG_SECRET")]
    pub ic_secret: Option<String>,

    #[arg(long, env = "UQ_ALLOWED_DIRECTORIES")]
    pub allowed_directories: Option<Vec<String>>,

    /// Number of pre-cloned DuckDB connections kept in the pool
    #[arg(default_value = "4", long, env = "UQ_POOL_SIZE")]
    pub pool_size: usize,
}

impl Options {
    pub fn init_script(&self) -> Vec<String> {
        let key_opt = self.gcs_key_id.as_ref();
        let secret_opt = self.gcs_secret.as_ref();
        let db_file_opt = self.db_file.as_ref();
        let ic_catalog_endpoint = self.ic_catalog_endpoint.as_ref();
        let ic_catalog_name = self.ic_catalog_name.as_ref();
        let ic_user = self.ic_user.as_ref();
        let ic_secret = self.ic_secret.as_ref();
        let mut init_script = Vec::new();

        if let (Some(key), Some(secret)) = (key_opt, secret_opt) {
            init_script.push(format!(
                "CREATE SECRET gcs_secret (TYPE GCS, KEY_ID '{key}', SECRET '{secret}');"
            ));
        } else if self.gcs_credential_chain {
            init_script.push(UQ_CREATE_GCP_CREDENTIAL_CHAIN.to_string());
        }

        if self.aws_credential_chain {
            init_script.push(UQ_CREATE_AWS_CREDENTIAL_CHAIN.to_string());
        }

        if let (Some(ic_catalog_endpoint), Some(ic_catalog_name), Some(ic_user), Some(ic_secret)) =
            (ic_catalog_endpoint, ic_catalog_name, ic_user, ic_secret)
        {
            init_script.push("LOAD httpfs;".to_string()); // https://github.com/duckdb/duckdb-iceberg/issues/483
            init_script.push("LOAD iceberg;".to_string());
            init_script.push(format!("CREATE SECRET ic_secret (TYPE iceberg, CLIENT_ID '{ic_user}', CLIENT_SECRET '{ic_secret}', ENDPOINT '{ic_catalog_endpoint}');"));
            init_script.push(format!("ATTACH '{ic_catalog_name}' AS iceberg (TYPE iceberg, ENDPOINT '{ic_catalog_endpoint}');"));
        }

        if let Some(db_file) = db_file_opt {
            init_script.push(format!(
                "ATTACH '{db_file}' as {UQ_ATTACHED_DB_NAME} (READ_ONLY);"
            ));
        }

        if self.duckdb_ui {
            init_script.push(UQ_START_UI_SERVER.to_string());
        }

        let directories = self.get_allowed_directories();
        if !directories.is_empty() {
            init_script.push(format!("SET allowed_directories = [{}];",directories));
            init_script.push("SET enable_external_access=false;".to_string());
        }

        init_script.push("SET lock_configuration = true;".to_string());
        init_script
    }

    fn get_allowed_directories(&self) -> String {
        let local_dirs: Vec<String> = self.allowed_directories.clone().unwrap_or_else(|| {
            env::current_dir()
                .map(|dir| vec![dir.to_string_lossy().into_owned()])
                .unwrap_or_else(|e| {
                    warn!("Failed to get current directory: {}", e);
                    vec![]
                })
        });

        CLOUD_PREFIXES
            .iter()
            .map(|s| format!("'{s}'"))
            .chain(local_dirs.iter().map(|s| format!("'{s}'")))
            .collect::<Vec<_>>()
            .join(",")
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
        .with_env_filter(
            EnvFilter::new("pingora_core=off,pingora_pool=off,pingora_proxy=off")
                .add_directive(LevelFilter::from(debug_level).into()),
        )
        .init();
    opts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_opts() -> Options {
        Options {
            port: 8080,
            addr: "0.0.0.0".into(),
            verbose: 0,
            gcs_key_id: None,
            gcs_secret: None,
            gcs_credential_chain: false,
            db_file: None,
            cors_enabled: false,
            aws_credential_chain: false,
            duckdb_ui: false,
            duckdb_ui_port: 14213,
            ic_catalog_endpoint: None,
            ic_catalog_name: None,
            ic_user: None,
            ic_secret: None,
            allowed_directories: None,
            pool_size: 4,
        }
    }

    #[test]
    fn init_query_empty() {
        let options: Options = test_opts();
        assert_eq!(options.init_script().last().unwrap(),"SET lock_configuration = true;")
    }

    #[test]
    fn init_query_gcs() {
        let options: Options = Options {
            gcs_key_id: Some("key_id".to_string()),
            gcs_secret: Some("secret".to_string()),
            ..test_opts()
        };
        assert_eq!(
            options.init_script()[0],
            "CREATE SECRET gcs_secret (TYPE GCS, KEY_ID 'key_id', SECRET 'secret');"
        );
    }

    #[test]
    fn init_query_aws() {
        let options: Options = Options {
            aws_credential_chain: true,
            ..test_opts()
        };
        assert_eq!(options.init_script()[0], UQ_CREATE_AWS_CREDENTIAL_CHAIN);
    }

    #[test]
    fn init_query_secret_gcs() {
        let options: Options = Options {
            gcs_key_id: Some("key_id2".to_string()),
            gcs_secret: Some("secret2".to_string()),
            ..test_opts()
        };
        assert_eq!(
            options.init_script()[0],
            "CREATE SECRET gcs_secret (TYPE GCS, KEY_ID 'key_id2', SECRET 'secret2');"
        );
    }

    #[test]
    fn init_query_chain_gcs() {
        let options: Options = Options {
            gcs_credential_chain: true,
            ..test_opts()
        };
        assert_eq!(options.init_script()[0], UQ_CREATE_GCP_CREDENTIAL_CHAIN);
    }

    #[test]
    fn init_duckdb_ui() {
        let options: Options = Options {
            duckdb_ui: true,
            duckdb_ui_port: 14213,
            ..test_opts()
        };
        assert_eq!(options.init_script()[0], UQ_START_UI_SERVER);
    }

    #[test]
    fn init_iceberg_init() {
        let options: Options = Options {
            ic_catalog_endpoint: Some("https://anycatalog.com/api/catalog".to_string()),
            ic_catalog_name: Some("my_catalog".to_string()),
            ic_user: Some("ic_user".to_string()),
            ic_secret: Some("ic_secret".to_string()),
            ..test_opts()
        };
        assert_eq!(options.init_script()[0], "LOAD httpfs;");
        assert_eq!(options.init_script()[1], "LOAD iceberg;");
        assert_eq!(
            options.init_script()[2],
            "CREATE SECRET ic_secret (TYPE iceberg, CLIENT_ID 'ic_user', CLIENT_SECRET 'ic_secret', ENDPOINT 'https://anycatalog.com/api/catalog');"
        );
        assert_eq!(
            options.init_script()[3],
            "ATTACH 'my_catalog' AS iceberg (TYPE iceberg, ENDPOINT 'https://anycatalog.com/api/catalog');"
        );
    }

    #[test]
    fn init_allowed_directories() {
        let options: Options = Options {
            allowed_directories: Some(vec!["/home/test".to_string(), "/tmp".to_string()]),
            ..test_opts()
        };
        assert!(options.init_script()[0].contains("'/home/test'"));
        assert!(options.init_script()[0].contains("'/tmp'"));
        assert_eq!(options.init_script()[1], "SET enable_external_access=false;");
    }
}
