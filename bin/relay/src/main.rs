//! # Odyssey Relay
//!
//! A relay service that sponsors transactions for EIP-7702 accounts.

use alloy_provider::{network::EthereumWallet, Provider, ProviderBuilder};
use alloy_rpc_client::RpcClient;
use alloy_signer_local::PrivateKeySigner;
use clap::Parser;
use eyre::Context;
use hyper::Method;
use jsonrpsee::server::Server;
use odyssey_wallet::{AlloyUpstream, OdysseyWallet, OdysseyWalletApiServer};
use reth_tracing::Tracer;
use std::net::{IpAddr, Ipv4Addr};
use tower_http::cors::{Any, CorsLayer};
use tracing::info;
use url::Url;

/// The Odyssey relayer service sponsors transactions for EIP-7702 accounts.
#[derive(Debug, Parser)]
#[command(author, about = "Relay", long_about = None)]
struct Args {
    /// The address to serve the RPC on.
    #[arg(long = "http.addr", value_name = "ADDR", default_value_t = IpAddr::V4(Ipv4Addr::LOCALHOST))]
    address: IpAddr,
    /// The port to serve the RPC on.
    #[arg(long = "http.port", value_name = "PORT", default_value_t = 9119)]
    port: u16,
    /// The RPC endpoint of the chain to send transactions to.
    /// Must be a valid HTTP or HTTPS URL pointing to an Ethereum JSON-RPC endpoint.
    #[arg(long, value_name = "RPC_ENDPOINT")]
    upstream: Url,
    /// The secret key to sponsor transactions with.
    #[arg(long, value_name = "SECRET_KEY", env = "RELAY_SK")]
    secret_key: String,
}

impl Args {
    /// Run the relayer service.
    async fn run(self) -> eyre::Result<()> {
        let _guard = reth_tracing::RethTracer::new().init()?;

        // construct provider
        let signer: PrivateKeySigner = self.secret_key.parse().wrap_err("Invalid signing key")?;
        let wallet = EthereumWallet::from(signer);
        let rpc_client = RpcClient::new_http(self.upstream);
        let provider = ProviderBuilder::new().wallet(wallet).connect_client(rpc_client);

        // get chain id
        let chain_id = provider.get_chain_id().await?;

        // construct rpc module
        let rpc = OdysseyWallet::new(AlloyUpstream::new(provider), chain_id).into_rpc();

        // start server
        let cors = CorsLayer::new()
            .allow_methods([Method::POST])
            .allow_origin(Any)
            .allow_headers([hyper::header::CONTENT_TYPE]);
        let middleware = tower::ServiceBuilder::new().layer(cors);
        let server = Server::builder()
            .set_http_middleware(middleware)
            .build((self.address, self.port))
            .await?;
        info!(addr = ?server.local_addr()?, "Started relay service");

        let handle = server.start(rpc);
        handle.stopped().await;

        Ok(())
    }
}

#[doc(hidden)]
#[tokio::main]
async fn main() {
    // Enable backtraces unless a RUST_BACKTRACE value has already been explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        unsafe {
            std::env::set_var("RUST_BACKTRACE", "1");
        }
    }

    let args = Args::parse();
    if let Err(err) = args.run().await {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}
