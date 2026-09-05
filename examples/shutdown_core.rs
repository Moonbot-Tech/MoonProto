//! Request guarded, graceful MoonBot core shutdown using the public API.
//!
//! Set MOONPROTO_KEY, then run: cargo run --example shutdown_core -- [host:port]
//! The core refuses shutdown while active take/sell orders exist. There is no
//! command acknowledgement; a deployer must verify process exit separately.

use std::{env, thread, time::Duration};

use moonproto::{ConnectConfig, MoonClient};

mod common;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key =
        env::var("MOONPROTO_KEY").map_err(|_| "set MOONPROTO_KEY before running shutdown_core")?;
    let endpoint = env::args().nth(1);
    let (cfg, _) = common::client_config(key.trim(), endpoint.as_ref())?;
    let client = MoonClient::connect_blocking(
        cfg,
        ConnectConfig::new(common::init_config()).with_connect_timeout(Duration::from_secs(45)),
        Duration::from_secs(60),
    )?;
    client.settings().request_core_shutdown()?;
    thread::sleep(Duration::from_secs(5));
    client.disconnect()?;
    client.wait_finished()?;
    println!("Shutdown requested. Verify that the core exited before replacing its executable.");
    Ok(())
}
