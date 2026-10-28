use crate::cli::options::Options;
use crate::cli::proxy::UIProxyService;
use pingora::prelude::{Server, http_proxy_service};
use std::sync::Arc;
use tokio::signal;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};
use uquery::core::duckdb::DuckDbEngine;
use uquery::core::engine::UQueryEngine;
use uquery::duckdb::Connection;
use uquery::web;

mod cli;

fn main() {
    let cli_options = cli::options::parse();
    if let Err(e) = cli_options.validate() {
        error!("{e}");
        std::process::exit(1);
    }
    if cli_options.self_update || cli_options.check_update {
        if let Err(e) = cli::update::run(cli_options.check_update, cli_options.yes) {
            error!("Update failed: {e}");
            std::process::exit(1);
        }
        return;
    }
    let start = Instant::now();
    let addr = format!("{}:{}", cli_options.addr, cli_options.port);
    let conn = Connection::open_in_memory().unwrap();

    if cli_options.install_extensions {
        for sql in Options::all_extensions_script() {
            conn.execute(sql.as_str(), []).unwrap();
        }
        info!("All DuckDB extensions installed successfully.");
        return;
    }

    for sql in cli_options.install_script() {
        conn.execute(sql.as_str(), []).unwrap();
    }
    for init_query in cli_options.init_script() {
        conn.execute(init_query.as_str(), []).unwrap();
    }
    if cli_options.allow_writes {
        warn!(
            "Write statements are allowed: every client can modify the shared database and write files in the allowed directories"
        );
    }
    let engine: Arc<dyn UQueryEngine> = Arc::new(
        DuckDbEngine::new(conn, cli_options.db_file.is_some(), cli_options.pool_size)
            .unwrap()
            .allow_writes(cli_options.allow_writes),
    );

    let tk_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .max_blocking_threads(cli_options.pool_size * 2)
        .build()
        .unwrap();

    if cli_options.duckdb_ui {
        let ui_addr = format!("{}:{}", cli_options.addr, cli_options.duckdb_ui_port);
        tk_runtime.spawn_blocking(move || {
            start_duckdb_ui_proxy(&ui_addr);
        });
    }

    tk_runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
        info!("uQuery server started in {:?}", start.elapsed());
        debug!("listening on {}", addr);
        let query_timeout = match cli_options.query_timeout_secs {
            0 => None,
            secs => Some(std::time::Duration::from_secs(secs)),
        };
        let api_key = cli_options.api_key.map(|key| {
            if key.is_empty() {
                let key = web::auth::generate_api_key();
                info!("API key required, generated key: {key}");
                key
            } else {
                info!("API key required");
                key
            }
        });
        let router =
            web::routers::create_router(engine, cli_options.cors_enabled, query_timeout, api_key);
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap();
    });
}

fn start_duckdb_ui_proxy(ui_addr: &str) {
    let mut server = Server::new(None).unwrap();
    server.bootstrap();
    let service: UIProxyService = UIProxyService;
    let mut app = http_proxy_service(&server.configuration, service);
    app.add_tcp(ui_addr);
    server.add_service(app);
    info!("DuckDB UI Proxy server started on {ui_addr}");
    server.run_forever()
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    debug!("Shutting down uQuery server");
}
