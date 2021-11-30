use henix::run;
use tracing::{info, error};

#[tokio::main]
async fn main() {
    // Initialize logging.
    {
        let mut env_var_exists = false;
        // If environment var is empty or does not exist, set it to INFO by default.
        if std::env::var("RUST_LOG").map_or(true, |x| x.is_empty()) {
            std::env::set_var("RUST_LOG", "INFO");
        } else {
            env_var_exists = true;
        }
        tracing_subscriber::fmt::init();
        if env_var_exists {
            info!("Picked up $RUST_LOG");
        }
    }

    // Run and process any errors.
    if let Err(e) = run(std::env::args_os()).await {
        error!("{:?}", e);
        std::process::exit(1);
    }
}

