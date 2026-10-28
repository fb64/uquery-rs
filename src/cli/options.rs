use clap::Parser;
use std::env;
use std::path::Path;
use tracing::metadata::LevelFilter;
use tracing::warn;
use tracing_subscriber::EnvFilter;
use uquery::core::duckdb::UQ_ATTACHED_DB_NAME;

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

/// Remote URL prefixes allowed by default
const CLOUD_PREFIXES: &[&str] = &["https://", "gcs://", "gs://", "gcss://", "s3://"];

/// `--allowed-urls` value disabling remote access
const NO_URLS: &str = "none";

/// CA certificate bundles DuckDB looks for to verify HTTPS servers. The sandbox
/// blocks reading them unless they are listed in `allowed_paths`.
const CA_BUNDLES: &[&str] = &[
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/etc/ssl/ca-bundle.pem",
    "/etc/ssl/cert.pem",
];

/// All known DuckDB extensions with their optional source repository.
const ALL_EXTENSIONS: &[(&str, Option<&str>)] = &[
    ("httpfs", None),
    ("iceberg", None),
    ("ui", None),
    ("ducklake", None),
    ("gcs", Some("community")),
];

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Options {
    /// Port to listen on
    #[arg(default_value = "8080", short, long, env = "UQ_PORT")]
    pub port: u16,

    /// Address to listen on. Use 0.0.0.0 to accept connections from other hosts
    #[arg(default_value = "127.0.0.1", short, long, env = "UQ_ADDR")]
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

    /// Comma-separated local directories queries can read (default: the current directory)
    #[arg(long, env = "UQ_ALLOWED_DIRECTORIES", value_delimiter = ',')]
    pub allowed_directories: Option<Vec<String>>,

    /// Comma-separated URL prefixes queries can read, e.g. https://data.example.com/ or
    /// s3://my-bucket/, or `none` to disable remote access (default: https://, s3://,
    /// gs://, gcs://, gcss://)
    #[arg(long, env = "UQ_ALLOWED_URLS", value_delimiter = ',')]
    pub allowed_urls: Option<Vec<String>>,

    /// Allow statements other than SELECT (COPY, CREATE, ATTACH, USE, ...). All
    /// clients share the same database: only enable this for trusted clients.
    #[arg(long, env = "UQ_ALLOW_WRITES")]
    pub allow_writes: bool,

    /// Number of pre-cloned DuckDB connections kept in the pool
    #[arg(default_value = "4", long, env = "UQ_POOL_SIZE")]
    pub pool_size: usize,

    /// Maximum query execution time in seconds (0 = no timeout)
    #[arg(default_value = "30", long, env = "UQ_QUERY_TIMEOUT")]
    pub query_timeout_secs: u64,

    /// Maximum total query time in seconds, including streaming the result. A
    /// response still streaming at that point is aborted (0 = no limit)
    #[arg(default_value = "0", long, env = "UQ_MAX_QUERY_DURATION")]
    pub max_query_duration_secs: u64,

    /// DuckDB memory limit, e.g. 4GB (default: 80% of the system memory)
    #[arg(long, env = "UQ_MEMORY_LIMIT")]
    pub memory_limit: Option<String>,

    /// Number of DuckDB worker threads (default: number of CPU cores)
    #[arg(long, env = "UQ_THREADS")]
    pub threads: Option<usize>,

    /// Require this API key on queries, sent as `Authorization: Bearer <key>` or
    /// `X-API-Key: <key>`. Without a value, a key is generated and printed at
    /// startup. When the option is absent, the API is open.
    #[arg(long, env = "UQ_API_KEY", num_args = 0..=1, default_missing_value = "")]
    pub api_key: Option<String>,

    /// Install all DuckDB extensions and exit. Use this once after installation
    /// to pre-download extensions so the server starts without network access.
    #[arg(long, env = "UQ_INSTALL_EXTENSIONS")]
    pub install_extensions: bool,

    /// Update uQuery to the latest GitHub release and exit.
    #[arg(long, conflicts_with = "check_update")]
    pub self_update: bool,

    /// Check for a newer GitHub release, print its release notes and exit.
    #[arg(long)]
    pub check_update: bool,

    /// Do not ask for confirmation before installing an update.
    #[arg(short = 'y', long, requires = "self_update")]
    pub yes: bool,
}

impl Options {
    /// Rejects option combinations that would bypass the API key.
    pub fn validate(&self) -> Result<(), String> {
        if self.api_key.is_some() && self.duckdb_ui {
            return Err(
                "--api-key cannot be used with --duckdb-ui: the DuckDB UI proxy does not check the API key"
                    .to_string(),
            );
        }
        if let Some(urls) = &self.allowed_urls
            && let Some(url) = urls
                .iter()
                .find(|url| *url != NO_URLS && !url.contains("://"))
        {
            return Err(format!(
                "--allowed-urls: '{url}' is not a URL prefix such as https:// or s3://my-bucket/ (use --allowed-directories for local paths)"
            ));
        }
        Ok(())
    }

    /// Returns SQL statements to INSTALL the DuckDB extensions required by the
    /// current configuration. Safe to run on every start — DuckDB is a no-op
    /// for extensions that are already installed.
    pub fn install_script(&self) -> Vec<String> {
        let mut needed: Vec<(&str, Option<&str>)> = Vec::new();

        needed.push(("httpfs", None));
        if self.ic_catalog_endpoint.is_some() {
            needed.push(("iceberg", None));
        }
        if self.gcs_credential_chain || (self.gcs_key_id.is_some() && self.gcs_secret.is_some()) {
            needed.push(("gcs", Some("community")));
        }
        if self.duckdb_ui {
            needed.push(("ui", None));
        }

        Self::build_install_sql(&needed)
    }

    /// Returns SQL statements to INSTALL all known DuckDB extensions.
    /// Used by the `--install-extensions` flag for one-time pre-warming.
    pub fn all_extensions_script() -> Vec<String> {
        Self::build_install_sql(ALL_EXTENSIONS)
    }

    fn build_install_sql(extensions: &[(&str, Option<&str>)]) -> Vec<String> {
        extensions
            .iter()
            .map(|(name, source)| match source {
                Some(src) => format!("INSTALL {name} FROM {src};"),
                None => format!("INSTALL {name};"),
            })
            .collect()
    }

    pub fn init_script(&self) -> Vec<String> {
        let key_opt = self.gcs_key_id.as_ref();
        let secret_opt = self.gcs_secret.as_ref();
        let db_file_opt = self.db_file.as_ref();
        let ic_catalog_endpoint = self.ic_catalog_endpoint.as_ref();
        let ic_catalog_name = self.ic_catalog_name.as_ref();
        let ic_user = self.ic_user.as_ref();
        let ic_secret = self.ic_secret.as_ref();
        let mut init_script = Vec::new();

        // Secrets saved in ~/.duckdb are outside the sandbox: loading them on the first
        // remote read fails. This must run before anything uses the secret manager.
        init_script.push("SET allow_persistent_secrets = false;".to_string());
        init_script.push("LOAD httpfs;".to_string());

        if let (Some(key), Some(secret)) = (key_opt, secret_opt) {
            init_script.push(format!(
                "CREATE SECRET gcs_secret (TYPE GCS, KEY_ID {}, SECRET {});",
                sql_string(key),
                sql_string(secret)
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
            init_script.push("LOAD iceberg;".to_string());
            let endpoint = sql_string(ic_catalog_endpoint);
            init_script.push(format!(
                "CREATE SECRET ic_secret (TYPE iceberg, CLIENT_ID {}, CLIENT_SECRET {}, ENDPOINT {endpoint});",
                sql_string(ic_user),
                sql_string(ic_secret)
            ));
            init_script.push(format!(
                "ATTACH {} AS iceberg (TYPE iceberg, ENDPOINT {endpoint});",
                sql_string(ic_catalog_name)
            ));
        }

        if let Some(db_file) = db_file_opt {
            init_script.push(format!(
                "ATTACH {} as {UQ_ATTACHED_DB_NAME} (READ_ONLY);",
                sql_string(db_file)
            ));
        }

        if self.duckdb_ui {
            init_script.push(UQ_START_UI_SERVER.to_string());
        }

        // Always set, even when empty: an empty list blocks all file and URL access.
        init_script.push(format!(
            "SET allowed_directories = [{}];",
            self.get_allowed_directories()
        ));
        // Must be set before external access is disabled.
        init_script.push(format!(
            "SET allowed_paths = [{}];",
            self.get_allowed_paths()
        ));
        init_script.push("SET enable_external_access=false;".to_string());

        if let Some(memory_limit) = &self.memory_limit {
            init_script.push(format!("SET memory_limit = {};", sql_string(memory_limit)));
        }
        if let Some(threads) = self.threads {
            init_script.push(format!("SET threads = {threads};"));
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

        self.allowed_url_prefixes()
            .iter()
            .map(String::as_str)
            .chain(local_dirs.iter().map(String::as_str))
            .filter(|path| !path.is_empty())
            .map(sql_string)
            .collect::<Vec<_>>()
            .join(",")
    }

    /// The CA bundles present on this system, needed for HTTPS when remote access is allowed.
    fn get_allowed_paths(&self) -> String {
        if self.allowed_url_prefixes().is_empty() {
            return String::new();
        }
        CA_BUNDLES
            .iter()
            .filter(|path| Path::new(path).is_file())
            .map(|path| sql_string(path))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// URL prefixes from `--allowed-urls`, or the cloud prefixes by default. A prefix
    /// naming a host or bucket gets a trailing `/`, so that `s3://bucket` does not also
    /// allow `s3://bucket-other`.
    fn allowed_url_prefixes(&self) -> Vec<String> {
        let Some(urls) = &self.allowed_urls else {
            return CLOUD_PREFIXES.iter().map(|s| s.to_string()).collect();
        };
        urls.iter()
            .filter(|url| !url.is_empty() && *url != NO_URLS)
            .map(|url| match url.split_once("://") {
                Some((_, rest)) if !rest.is_empty() && !url.ends_with('/') => format!("{url}/"),
                _ => url.clone(),
            })
            .collect()
    }
}

/// Quote `value` as a SQL string literal.
fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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
            allowed_urls: None,
            allow_writes: false,
            pool_size: 4,
            query_timeout_secs: 30,
            max_query_duration_secs: 0,
            memory_limit: None,
            threads: None,
            install_extensions: false,
            api_key: None,
            self_update: false,
            check_update: false,
            yes: false,
        }
    }

    #[test]
    fn validate_api_key_with_duckdb_ui() {
        assert!(test_opts().validate().is_ok());
        let api_key_only = Options {
            api_key: Some(String::new()),
            ..test_opts()
        };
        assert!(api_key_only.validate().is_ok());
        let ui_only = Options {
            duckdb_ui: true,
            ..test_opts()
        };
        assert!(ui_only.validate().is_ok());
        let both = Options {
            api_key: Some("key".to_string()),
            duckdb_ui: true,
            ..test_opts()
        };
        assert!(both.validate().is_err());
    }

    #[test]
    fn parse_api_key() {
        let parse = |args: &[&str]| Options::try_parse_from(args).unwrap().api_key;
        assert_eq!(parse(&["uquery"]), None);
        assert_eq!(parse(&["uquery", "--api-key"]), Some(String::new()));
        assert_eq!(
            parse(&["uquery", "--api-key", "secret"]),
            Some("secret".to_string())
        );
    }

    #[test]
    fn parse_update_flags() {
        let opts = Options::try_parse_from(["uquery", "--self-update", "-y"]).unwrap();
        assert!(opts.self_update && opts.yes && !opts.check_update);
        assert!(
            Options::try_parse_from(["uquery", "--check-update"])
                .unwrap()
                .check_update
        );
        assert!(Options::try_parse_from(["uquery", "--yes"]).is_err());
        assert!(Options::try_parse_from(["uquery", "--self-update", "--check-update"]).is_err());
    }

    #[test]
    fn parse_allowed_directories() {
        let parse = |args: &[&str]| Options::try_parse_from(args).unwrap().allowed_directories;
        let expected = Some(vec!["/a".to_string(), "/b".to_string()]);
        assert_eq!(
            parse(&["uquery", "--allowed-directories", "/a,/b"]),
            expected
        );
        assert_eq!(
            parse(&[
                "uquery",
                "--allowed-directories",
                "/a",
                "--allowed-directories",
                "/b"
            ]),
            expected
        );
    }

    #[test]
    fn init_query_empty() {
        let options: Options = test_opts();
        assert_eq!(
            options.init_script()[0],
            "SET allow_persistent_secrets = false;"
        );
        assert_eq!(options.init_script()[1], "LOAD httpfs;");
        assert_eq!(
            options.init_script().last().unwrap(),
            "SET lock_configuration = true;"
        )
    }

    #[test]
    fn init_query_gcs() {
        let options: Options = Options {
            gcs_key_id: Some("key_id".to_string()),
            gcs_secret: Some("secret".to_string()),
            ..test_opts()
        };
        assert_eq!(
            options.init_script()[2],
            "CREATE SECRET gcs_secret (TYPE GCS, KEY_ID 'key_id', SECRET 'secret');"
        );
    }

    #[test]
    fn init_query_escapes_quotes() {
        let options = Options {
            gcs_key_id: Some("key".to_string()),
            gcs_secret: Some("it's".to_string()),
            db_file: Some("/data/o'brien.db".to_string()),
            allowed_directories: Some(vec!["/data/o'brien".to_string()]),
            ..test_opts()
        };
        let script = options.init_script();
        assert_eq!(
            script[2],
            "CREATE SECRET gcs_secret (TYPE GCS, KEY_ID 'key', SECRET 'it''s');"
        );
        assert_eq!(
            script[3],
            format!("ATTACH '/data/o''brien.db' as {UQ_ATTACHED_DB_NAME} (READ_ONLY);")
        );
        assert!(script[4].ends_with(",'/data/o''brien'];"), "{}", script[4]);
    }

    #[test]
    fn init_resource_limits() {
        let options = Options {
            memory_limit: Some("4GB".to_string()),
            threads: Some(2),
            allowed_directories: Some(vec!["/data".to_string()]),
            ..test_opts()
        };
        let script = options.init_script();
        let n = script.len();
        assert_eq!(script[n - 3], "SET memory_limit = '4GB';");
        assert_eq!(script[n - 2], "SET threads = 2;");
        assert_eq!(script[n - 1], "SET lock_configuration = true;");
    }

    #[test]
    fn init_query_aws() {
        let options: Options = Options {
            aws_credential_chain: true,
            ..test_opts()
        };
        assert_eq!(options.init_script()[2], UQ_CREATE_AWS_CREDENTIAL_CHAIN);
    }

    #[test]
    fn init_query_secret_gcs() {
        let options: Options = Options {
            gcs_key_id: Some("key_id2".to_string()),
            gcs_secret: Some("secret2".to_string()),
            ..test_opts()
        };
        assert_eq!(
            options.init_script()[2],
            "CREATE SECRET gcs_secret (TYPE GCS, KEY_ID 'key_id2', SECRET 'secret2');"
        );
    }

    #[test]
    fn init_query_chain_gcs() {
        let options: Options = Options {
            gcs_credential_chain: true,
            ..test_opts()
        };
        assert_eq!(options.init_script()[2], UQ_CREATE_GCP_CREDENTIAL_CHAIN);
    }

    #[test]
    fn init_duckdb_ui() {
        let options: Options = Options {
            duckdb_ui: true,
            duckdb_ui_port: 14213,
            ..test_opts()
        };
        assert_eq!(options.init_script()[2], UQ_START_UI_SERVER);
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
        assert_eq!(options.init_script()[1], "LOAD httpfs;");
        assert_eq!(options.init_script()[2], "LOAD iceberg;");
        assert_eq!(
            options.init_script()[3],
            "CREATE SECRET ic_secret (TYPE iceberg, CLIENT_ID 'ic_user', CLIENT_SECRET 'ic_secret', ENDPOINT 'https://anycatalog.com/api/catalog');"
        );
        assert_eq!(
            options.init_script()[4],
            "ATTACH 'my_catalog' AS iceberg (TYPE iceberg, ENDPOINT 'https://anycatalog.com/api/catalog');"
        );
    }

    #[test]
    fn init_allowed_directories() {
        let options: Options = Options {
            allowed_directories: Some(vec!["/home/test".to_string(), "/tmp".to_string()]),
            ..test_opts()
        };
        assert!(options.init_script()[2].contains("'/home/test'"));
        assert!(options.init_script()[2].contains("'/tmp'"));
        assert_eq!(
            options.init_script()[4],
            "SET enable_external_access=false;"
        );
    }

    #[test]
    fn init_default_allowed_urls() {
        let options = Options {
            allowed_directories: Some(vec!["/data".to_string()]),
            ..test_opts()
        };
        assert_eq!(
            options.init_script()[2],
            "SET allowed_directories = ['https://','gcs://','gs://','gcss://','s3://','/data'];"
        );
    }

    #[test]
    fn init_allowed_urls() {
        let options = Options {
            allowed_directories: Some(vec!["/data".to_string()]),
            allowed_urls: Some(vec![
                "https://data.example.com".to_string(),
                "s3://bucket/reports/".to_string(),
                "https://".to_string(),
            ]),
            ..test_opts()
        };
        assert_eq!(
            options.init_script()[2],
            "SET allowed_directories = ['https://data.example.com/','s3://bucket/reports/','https://','/data'];"
        );
    }

    #[test]
    fn init_no_access() {
        let options = Options {
            allowed_directories: Some(vec![String::new()]),
            allowed_urls: Some(vec![NO_URLS.to_string()]),
            ..test_opts()
        };
        assert_eq!(options.init_script()[2], "SET allowed_directories = [];");
        assert_eq!(options.init_script()[3], "SET allowed_paths = [];");
        assert_eq!(
            options.init_script()[4],
            "SET enable_external_access=false;"
        );
    }

    #[test]
    fn validate_allowed_urls() {
        let with_urls = |urls: &[&str]| Options {
            allowed_urls: Some(urls.iter().map(|s| s.to_string()).collect()),
            ..test_opts()
        };
        assert!(with_urls(&["https://", "s3://bucket"]).validate().is_ok());
        assert!(with_urls(&["none"]).validate().is_ok());
        assert!(with_urls(&["/data"]).validate().is_err());
    }

    /// Runs the startup SQL on DuckDB (minus `LOAD httpfs`, which needs network
    /// access) and checks the resulting sandbox.
    fn sandboxed_connection(options: &Options) -> uquery::duckdb::Connection {
        let conn = uquery::duckdb::Connection::open_in_memory().unwrap();
        for sql in options
            .init_script()
            .iter()
            .filter(|sql| *sql != "LOAD httpfs;")
        {
            conn.execute_batch(sql)
                .unwrap_or_else(|e| panic!("{sql}: {e}"));
        }
        conn
    }

    fn can_read(conn: &uquery::duckdb::Connection, path: &str) -> bool {
        conn.execute_batch(&format!("SELECT * FROM read_text({})", sql_string(path)))
            .is_ok()
    }

    #[test]
    fn init_script_sandbox_on_duckdb() {
        let tests_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests");
        let options = Options {
            allowed_directories: Some(vec![tests_dir.to_string()]),
            allowed_urls: Some(vec![NO_URLS.to_string()]),
            db_file: Some(format!("{tests_dir}/test.db")),
            memory_limit: Some("1GB".to_string()),
            threads: Some(2),
            ..test_opts()
        };
        let conn = sandboxed_connection(&options);
        assert!(can_read(&conn, &format!("{tests_dir}/test.csv")));
        assert!(!can_read(
            &conn,
            concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")
        ));
        assert!(!can_read(&conn, "/etc/passwd"));
        assert!(
            conn.execute_batch("SET threads = 4").is_err(),
            "configuration must be locked"
        );
    }

    #[test]
    fn init_script_remote_access_on_duckdb() {
        let options = Options {
            allowed_directories: Some(vec![String::new()]),
            ..test_opts()
        };
        let conn = sandboxed_connection(&options);
        // Remote reads check secrets, including those saved in ~/.duckdb.
        assert!(conn.execute_batch("FROM duckdb_secrets()").is_ok());
        // HTTPS needs the CA bundle, but nothing else from its directory.
        for bundle in CA_BUNDLES.iter().filter(|p| Path::new(p).is_file()) {
            assert!(can_read(&conn, bundle), "{bundle}");
        }
        assert!(conn.execute_batch("FROM glob('/etc/ssl/certs/*')").is_err());
    }

    #[test]
    fn init_script_without_any_access_on_duckdb() {
        let options = Options {
            allowed_directories: Some(vec![String::new()]),
            allowed_urls: Some(vec![NO_URLS.to_string()]),
            ..test_opts()
        };
        let conn = sandboxed_connection(&options);
        let csv = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test.csv");
        assert!(!can_read(&conn, csv));
        for bundle in CA_BUNDLES {
            assert!(!can_read(&conn, bundle), "{bundle}");
        }
    }

    #[test]
    fn install_script_empty() {
        let options = test_opts();
        assert_eq!(options.install_script(), vec!["INSTALL httpfs;"]);
    }

    #[test]
    fn install_script_aws() {
        let options = Options {
            aws_credential_chain: true,
            ..test_opts()
        };
        assert_eq!(options.install_script(), vec!["INSTALL httpfs;"]);
    }

    #[test]
    fn install_script_iceberg() {
        let options = Options {
            ic_catalog_endpoint: Some("https://catalog.example.com".into()),
            ic_catalog_name: Some("cat".into()),
            ic_user: Some("u".into()),
            ic_secret: Some("s".into()),
            ..test_opts()
        };
        let script = options.install_script();
        assert_eq!(script, vec!["INSTALL httpfs;", "INSTALL iceberg;"]);
    }

    #[test]
    fn install_script_gcs_key() {
        let options = Options {
            gcs_key_id: Some("key".into()),
            gcs_secret: Some("secret".into()),
            ..test_opts()
        };
        assert_eq!(
            options.install_script(),
            vec!["INSTALL httpfs;", "INSTALL gcs FROM community;"]
        );
    }

    #[test]
    fn install_script_ui() {
        let options = Options {
            duckdb_ui: true,
            ..test_opts()
        };
        assert_eq!(
            options.install_script(),
            vec!["INSTALL httpfs;", "INSTALL ui;"]
        );
    }

    #[test]
    fn all_extensions_script_contains_all() {
        let script = Options::all_extensions_script();
        assert!(script.contains(&"INSTALL httpfs;".to_string()));
        assert!(script.contains(&"INSTALL iceberg;".to_string()));
        assert!(script.contains(&"INSTALL ui;".to_string()));
        assert!(script.contains(&"INSTALL ducklake;".to_string()));
        assert!(script.contains(&"INSTALL gcs FROM community;".to_string()));
        assert_eq!(script.len(), ALL_EXTENSIONS.len());
    }
}
