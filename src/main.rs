use kurosabi::Kurosabi;
use observer::context::ObserverContext;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    env_logger::try_init_from_env(env_logger::Env::default().default_filter_or("debug")).unwrap_or_else(|_| ());

    let hello: &'static str = "Hello, Observer Bot!";

    let ob_ctx = ObserverContext::new().await;

    let kurosabi = Kurosabi::with_context(ob_ctx.clone());

    let server = kurosabi.server().build();

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
}