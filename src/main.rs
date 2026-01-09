use kurosabi::Kurosabi;
use observer::context::ObserverContext;
use std::{net::{Ipv4Addr, TcpListener}, process::ExitCode};

#[tokio::main]
async fn main() -> ExitCode {
    dotenv::dotenv().ok();
    env_logger::try_init_from_env(env_logger::Env::default().default_filter_or("debug")).ok();

    // コンテキスト初期化
    let ob_ctx = ObserverContext::new().await;

    let config = ob_ctx.config.clone();

    let bind_ip = Ipv4Addr::from(config.web_server_host);
    if let Err(e) = TcpListener::bind((bind_ip, config.web_server_port)) {
        eprintln!(
            "failed to bind web server {}.{}.{}.{}:{} ({})\n\
hint: use an unprivileged port (>= 1024), or set WEB_SERVER_PORT / config.json web_server_port",
            config.web_server_host[0],
            config.web_server_host[1],
            config.web_server_host[2],
            config.web_server_host[3],
            config.web_server_port,
            e
        );
        let _ = ob_ctx.shutdown().await;
        return ExitCode::FAILURE;
    }

    let mut kurosabi = Kurosabi::with_context(ob_ctx.clone());

    kurosabi.get("/latex_expr_render", |mut c| async move {
        c.res.html(include_str!("../data/latex_render.html"));
        c
    });

    let server = kurosabi
        .server()
        .thread(16)
        .host(config.web_server_host)
        .port(config.web_server_port)
        .build();

    println!("server started. Press Ctrl-C to shutdown...");

    tokio::select! {
        _ = server.run_async() => {
            println!("server stopped (run_async returned)");
        }
        _ = tokio::signal::ctrl_c() => {
            println!("received Ctrl-C, shutting down server and browser engine...");
        }
    }

    // サーバ停止後にエンジンもshutdown
    if let Err(e) = ob_ctx.shutdown().await {
        eprintln!("engine shutdown error: {}", e);
    }
    println!("shutdown complete. Exiting.");

    ExitCode::SUCCESS
}