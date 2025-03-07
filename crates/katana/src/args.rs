use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;
use katana_core::backend::config::{Environment, StarknetConfig};
use katana_core::constants::{
    DEFAULT_GAS_PRICE, DEFAULT_INVOKE_MAX_STEPS, DEFAULT_VALIDATE_MAX_STEPS,
};
use katana_core::db::serde::state::SerializableState;
use katana_core::sequencer::SequencerConfig;
use katana_rpc::config::ServerConfig;
use tracing::Subscriber;
use tracing_subscriber::{fmt, EnvFilter};
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct KatanaArgs {
    #[arg(long)]
    #[arg(help = "Don't print anything on startup.")]
    pub silent: bool,

    #[arg(long)]
    #[arg(conflicts_with = "block_time")]
    #[arg(help = "Disable auto and interval mining, and mine on demand instead via an endpoint.")]
    pub no_mining: bool,

    #[arg(short, long)]
    #[arg(value_name = "MILLISECONDS")]
    #[arg(help = "Block time in milliseconds for interval mining.")]
    pub block_time: Option<u64>,

    #[arg(long)]
    #[arg(value_name = "PATH")]
    #[arg(help = "Dump the state of chain on exit to the given file.")]
    #[arg(long_help = "Dump the state of chain on exit to the given file. If the value is a \
                       directory, the state will be written to `<PATH>/state.bin`.")]
    pub dump_state: Option<PathBuf>,

    #[arg(long)]
    #[arg(value_name = "URL")]
    #[arg(help = "The Starknet RPC provider to fork the network from.")]
    pub rpc_url: Option<Url>,

    #[arg(long)]
    #[arg(help = "Output logs in JSON format.")]
    pub json_log: bool,

    #[arg(long)]
    #[arg(requires = "rpc_url")]
    #[arg(value_name = "BLOCK_NUMBER")]
    #[arg(help = "Fork the network at a specific block.")]
    pub fork_block_number: Option<u64>,

    #[arg(long)]
    #[arg(value_name = "PATH")]
    #[arg(value_parser = SerializableState::parse)]
    #[arg(help = "Initialize the chain from a previously saved state snapshot.")]
    pub load_state: Option<SerializableState>,

    #[cfg(feature = "messaging")]
    #[arg(long)]
    #[arg(value_name = "PATH")]
    #[arg(value_parser = katana_core::service::messaging::MessagingConfig::parse)]
    #[arg(help = "Configure the messaging with an other chain.")]
    #[arg(long_help = "Configure the messaging to allow Katana listening/sending messages on a \
                       settlement chain that can be Ethereum or an other Starknet sequencer. \
                       The configuration file details and examples can be found here: TODO.")]
    pub messaging: Option<katana_core::service::messaging::MessagingConfig>,

    #[command(flatten)]
    #[command(next_help_heading = "Server options")]
    pub server: ServerOptions,

    #[command(flatten)]
    #[command(next_help_heading = "Starknet options")]
    pub starknet: StarknetOptions,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(about = "Generate shell completion file for specified shell")]
    Completions { shell: Shell },
}

#[derive(Debug, Args, Clone)]
pub struct ServerOptions {
    #[arg(short, long)]
    #[arg(default_value = "5050")]
    #[arg(help = "Port number to listen on.")]
    pub port: u16,

    #[arg(long)]
    #[arg(help = "The IP address the server will listen on.")]
    pub host: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct StarknetOptions {
    #[arg(long)]
    #[arg(default_value = "0")]
    #[arg(help = "Specify the seed for randomness of accounts to be predeployed.")]
    pub seed: String,

    #[arg(long = "accounts")]
    #[arg(value_name = "NUM")]
    #[arg(default_value = "10")]
    #[arg(help = "Number of pre-funded accounts to generate.")]
    pub total_accounts: u8,

    #[arg(long)]
    #[arg(help = "Disable charging fee for transactions.")]
    pub disable_fee: bool,

    #[command(flatten)]
    #[command(next_help_heading = "Environment options")]
    pub environment: EnvironmentOptions,
}

#[derive(Debug, Args, Clone)]
pub struct EnvironmentOptions {
    #[arg(long)]
    #[arg(help = "The chain ID.")]
    #[arg(default_value = "KATANA")]
    pub chain_id: String,

    #[arg(long)]
    #[arg(help = "The gas price.")]
    pub gas_price: Option<u128>,

    #[arg(long)]
    #[arg(help = "The maximum number of steps available for the account validation logic.")]
    pub validate_max_steps: Option<u32>,

    #[arg(long)]
    #[arg(help = "The maximum number of steps available for the account execution logic.")]
    pub invoke_max_steps: Option<u32>,
}

impl KatanaArgs {
    pub fn init_logging(&self) -> Result<(), Box<dyn std::error::Error>> {
        const DEFAULT_LOG_FILTER: &str = "info,executor=trace,server=debug,katana_core=trace,\
                                          blockifier=off,jsonrpsee_server=off,hyper=off,\
                                          messaging=debug";

        let builder = fmt::Subscriber::builder().with_env_filter(
            EnvFilter::try_from_default_env().or(EnvFilter::try_new(DEFAULT_LOG_FILTER))?,
        );

        let subscriber: Box<dyn Subscriber + Send + Sync> = if self.json_log {
            Box::new(builder.json().finish())
        } else {
            Box::new(builder.finish())
        };

        Ok(tracing::subscriber::set_global_default(subscriber)?)
    }

    pub fn sequencer_config(&self) -> SequencerConfig {
        SequencerConfig {
            block_time: self.block_time,
            no_mining: self.no_mining,
            #[cfg(feature = "messaging")]
            messaging: self.messaging.clone(),
        }
    }

    pub fn server_config(&self) -> ServerConfig {
        ServerConfig {
            port: self.server.port,
            host: self.server.host.clone().unwrap_or("0.0.0.0".into()),
        }
    }

    pub fn starknet_config(&self) -> StarknetConfig {
        StarknetConfig {
            total_accounts: self.starknet.total_accounts,
            seed: parse_seed(&self.starknet.seed),
            disable_fee: self.starknet.disable_fee,
            init_state: self.load_state.clone(),
            fork_rpc_url: self.rpc_url.clone(),
            fork_block_number: self.fork_block_number,
            env: Environment {
                chain_id: self.starknet.environment.chain_id.clone(),
                gas_price: self.starknet.environment.gas_price.unwrap_or(DEFAULT_GAS_PRICE),
                invoke_max_steps: self
                    .starknet
                    .environment
                    .invoke_max_steps
                    .unwrap_or(DEFAULT_INVOKE_MAX_STEPS),
                validate_max_steps: self
                    .starknet
                    .environment
                    .validate_max_steps
                    .unwrap_or(DEFAULT_VALIDATE_MAX_STEPS),
            },
        }
    }
}

fn parse_seed(seed: &str) -> [u8; 32] {
    let seed = seed.as_bytes();

    if seed.len() >= 32 {
        unsafe { *(seed[..32].as_ptr() as *const [u8; 32]) }
    } else {
        let mut actual_seed = [0u8; 32];
        seed.iter().enumerate().for_each(|(i, b)| actual_seed[i] = *b);
        actual_seed
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn default_block_context_from_args() {
        let args = KatanaArgs::parse_from(["katana"]);
        let block_context = args.starknet_config().block_context();
        assert_eq!(block_context.gas_price, DEFAULT_GAS_PRICE);
        assert_eq!(block_context.chain_id.0, "KATANA".to_string());
        assert_eq!(block_context.validate_max_n_steps, DEFAULT_VALIDATE_MAX_STEPS);
        assert_eq!(block_context.invoke_tx_max_n_steps, DEFAULT_INVOKE_MAX_STEPS);
    }

    #[test]
    fn custom_block_context_from_args() {
        let args = KatanaArgs::parse_from([
            "katana",
            "--gas-price",
            "10",
            "--chain-id",
            "SN_GOERLI",
            "--validate-max-steps",
            "100",
            "--invoke-max-steps",
            "200",
        ]);

        let block_context = args.starknet_config().block_context();

        assert_eq!(block_context.gas_price, 10);
        assert_eq!(block_context.chain_id.0, "SN_GOERLI".to_string());
        assert_eq!(block_context.validate_max_n_steps, 100);
        assert_eq!(block_context.invoke_tx_max_n_steps, 200);
    }
}
