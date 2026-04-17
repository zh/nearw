use anyhow::Result;
use clap::{Parser, Subcommand};

mod cli;
mod config;
mod constants;
mod network;
mod oneclick;
mod storage;
mod wallet;

#[derive(Parser)]
#[command(name = "nearw", version, about = "nearw -- NEAR Protocol wallet CLI")]
struct Cli {
    /// Wallet name (uses default wallet if omitted)
    #[arg(short = 'n', long = "name", global = true)]
    name: Option<String>,

    /// Network: mainnet or testnet (overrides stored network)
    #[arg(long = "network", global = true)]
    network: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Wallet management (create, import, info, export, list, delete, default, register)
    Wallet {
        #[command(subcommand)]
        command: WalletCommand,
    },
    /// Check NEAR or token balance
    Balance {
        /// FT contract address for token balance
        #[arg(long)]
        token: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Send NEAR to an address
    Send {
        /// Recipient account ID
        receiver: String,
        /// Amount to send (in NEAR unless --unit is set)
        amount: String,
        /// Unit: near or yocto
        #[arg(long, default_value = "near")]
        unit: String,
        /// Skip confirmation prompt (for automation/agents)
        #[arg(long)]
        confirmed: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Send all NEAR to an address (drain wallet)
    SendAll {
        /// Recipient account ID
        receiver: String,
    },
    /// Show receive address and QR code
    Receive {
        /// Suppress QR code display
        #[arg(long)]
        no_qr: bool,
    },
    /// Fungible token operations (list, info, send)
    Token {
        #[command(subcommand)]
        command: TokenCommand,
    },
    /// NFT operations (list, info, send)
    Nft {
        #[command(subcommand)]
        command: NftCommand,
    },
    /// Transaction history
    History {
        /// Number of transactions to show
        #[arg(long, default_value = "20")]
        limit: u32,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Access key management (list, add, delete, generate)
    Key {
        #[command(subcommand)]
        command: KeyCommand,
    },
    /// Call a contract method (state-changing transaction)
    Call {
        /// Contract account ID
        contract: String,
        /// Method name
        method: String,
        /// JSON arguments (e.g. '{"account_id": "alice.near"}')
        args: String,
        /// NEAR deposit amount (e.g. "0.1" for NEAR, "1yocto" for yoctoNEAR)
        #[arg(long)]
        deposit: Option<String>,
        /// Gas in TGas (default: 100)
        #[arg(long)]
        gas: Option<u64>,
        /// Skip confirmation prompt
        #[arg(long)]
        confirmed: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// View a contract method (read-only, no transaction)
    View {
        /// Contract account ID
        contract: String,
        /// Method name
        method: String,
        /// JSON arguments (optional, defaults to '{}')
        args: Option<String>,
    },
    /// Token swap via NEAR Intents protocol (mainnet only)
    Swap {
        #[command(subcommand)]
        command: SwapCommand,
    },
}

#[derive(Subcommand)]
enum WalletCommand {
    /// Create a new wallet
    Create {
        /// Wallet name
        name: String,
    },
    /// Import an existing wallet from seed phrase
    Import {
        /// Wallet name
        name: String,
    },
    /// Show wallet info
    Info,
    /// Export wallet seed phrase
    Export,
    /// Delete a wallet
    Delete {
        /// Wallet name to delete
        name: String,
    },
    /// Set a wallet as the default
    Default {
        /// Wallet name to set as default
        name: String,
    },
    /// List all stored wallets
    List,
    /// Register a named account (suffix added automatically based on network)
    Register {
        /// Account name (defaults to wallet name if omitted)
        account: Option<String>,
    },
    /// Clear the named account association for a wallet
    Unregister,
}

#[derive(Subcommand)]
enum TokenCommand {
    /// List FT balances (auto-discovers tokens via indexer)
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show FT contract metadata
    Info {
        /// FT contract address
        contract: String,
    },
    /// Send fungible tokens
    Send {
        /// FT contract address
        contract: String,
        /// Recipient account ID
        receiver: String,
        /// Amount to send
        amount: String,
        /// Skip confirmation prompt (for automation/agents)
        #[arg(long)]
        confirmed: bool,
    },
}

#[derive(Subcommand)]
enum NftCommand {
    /// List NFTs (auto-discovers via indexer, or specify a contract)
    List {
        /// Optional: NFT contract address to query
        contract: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show NFT contract metadata
    Info {
        /// NFT contract address
        contract: String,
    },
    /// Send an NFT
    Send {
        /// NFT contract address
        contract: String,
        /// Token ID
        token_id: String,
        /// Recipient account ID
        receiver: String,
        /// Skip confirmation prompt (for automation/agents)
        #[arg(long)]
        confirmed: bool,
    },
}

#[derive(Subcommand)]
enum KeyCommand {
    /// List all access keys on the account
    List,
    /// Add a function-call access key for a contract
    Add {
        /// Contract to grant access to
        contract: String,
        /// Restrict to specific methods (comma-separated). Omit for all methods.
        #[arg(long)]
        methods: Option<String>,
        /// Gas allowance in NEAR (default: 0.25)
        #[arg(long, default_value = "0.25")]
        allowance: String,
        /// Use an existing public key instead of generating a new one
        #[arg(long)]
        public_key: Option<String>,
    },
    /// Add a full-access key (dangerous — grants complete control)
    AddFullAccess {
        /// Use an existing public key instead of generating a new one
        #[arg(long)]
        public_key: Option<String>,
    },
    /// Delete an access key by public key
    Delete {
        /// Public key to delete (ed25519:...)
        public_key: String,
    },
    /// Generate a new key pair (does not add to account)
    Generate,
    /// Import a key from seed phrase and store locally for a contract
    Import {
        /// Contract this key is for
        contract: String,
    },
}

#[derive(Subcommand)]
enum SwapCommand {
    /// Execute a token swap via NEAR Intents (mainnet only)
    Execute {
        /// Source token (alias like USDT, NEAR, or contract ID)
        from: String,
        /// Destination token (alias like USDT, NEAR, or contract ID)
        to: String,
        /// Amount to swap (in human-readable units, e.g. "100" for 100 USDT)
        amount: String,
        /// Skip confirmation prompt
        #[arg(long)]
        confirmed: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show a swap quote without executing
    Quote {
        /// Source token
        from: String,
        /// Destination token
        to: String,
        /// Amount to swap
        amount: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Check the status of an existing swap
    Status {
        /// Deposit address from a previous swap
        deposit_address: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// List supported tokens for swaps
    Tokens {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}

#[tokio::main]
async fn run() -> Result<()> {
    let cli = Cli::parse();
    let wallet_name = cli.name.as_deref();
    let cli_network = cli.network.as_deref();

    match cli.command {
        Commands::Wallet { command } => match command {
            WalletCommand::Create { name } => {
                cli::wallet::create(&name, cli_network)?;
            }
            WalletCommand::Import { name } => {
                cli::wallet::import(&name, cli_network)?;
            }
            WalletCommand::Info => {
                cli::wallet::info(wallet_name, cli_network).await?;
            }
            WalletCommand::Export => {
                cli::wallet::export(wallet_name)?;
            }
            WalletCommand::Delete { name } => {
                cli::wallet::delete(&name)?;
            }
            WalletCommand::Default { name } => {
                cli::wallet::set_default(&name)?;
            }
            WalletCommand::List => {
                cli::wallet::list()?;
            }
            WalletCommand::Register { account } => {
                cli::wallet::register(wallet_name, account.as_deref(), cli_network).await?;
            }
            WalletCommand::Unregister => {
                cli::wallet::unregister(wallet_name)?;
            }
        },
        Commands::Balance { token, json } => {
            cli::balance::run(wallet_name, cli_network, token.as_deref(), json).await?;
        }
        Commands::Send {
            receiver,
            amount,
            unit,
            confirmed,
            json,
        } => {
            cli::send::run(wallet_name, &receiver, &amount, &unit, cli_network, confirmed, json).await?;
        }
        Commands::SendAll { receiver } => {
            cli::send::run_send_all(wallet_name, &receiver, cli_network).await?;
        }
        Commands::Receive { no_qr } => {
            cli::receive::run(wallet_name, cli_network, no_qr).await?;
        }
        Commands::History { limit, json } => {
            cli::history::run(wallet_name, cli_network, limit, json).await?;
        }
        Commands::Token { command } => match command {
            TokenCommand::List { json } => {
                cli::token::list(wallet_name, cli_network, json).await?;
            }
            TokenCommand::Info { contract } => {
                cli::token::info(wallet_name, &contract, cli_network).await?;
            }
            TokenCommand::Send {
                contract,
                receiver,
                amount,
                confirmed,
            } => {
                cli::token::send(wallet_name, &receiver, &amount, &contract, cli_network, confirmed).await?;
            }
        },
        Commands::Nft { command } => match command {
            NftCommand::List { contract, json } => {
                cli::nft::list(wallet_name, contract.as_deref(), cli_network, json).await?;
            }
            NftCommand::Info { contract } => {
                cli::nft::info(wallet_name, &contract, cli_network).await?;
            }
            NftCommand::Send {
                contract,
                token_id,
                receiver,
                confirmed,
            } => {
                cli::nft::send(wallet_name, &receiver, &contract, &token_id, cli_network, confirmed).await?;
            }
        },
        Commands::Key { command } => match command {
            KeyCommand::List => {
                cli::key::list(wallet_name, cli_network).await?;
            }
            KeyCommand::Add {
                contract,
                methods,
                allowance,
                public_key,
            } => {
                cli::key::add_function_call(
                    wallet_name,
                    &contract,
                    methods.as_deref(),
                    &allowance,
                    public_key.as_deref(),
                    cli_network,
                )
                .await?;
            }
            KeyCommand::AddFullAccess { public_key } => {
                cli::key::add_full_access(wallet_name, public_key.as_deref(), cli_network).await?;
            }
            KeyCommand::Delete { public_key } => {
                cli::key::delete(wallet_name, &public_key, cli_network).await?;
            }
            KeyCommand::Generate => {
                cli::key::generate()?;
            }
            KeyCommand::Import { contract } => {
                cli::key::import(wallet_name, &contract)?;
            }
        },
        Commands::Call {
            contract,
            method,
            args,
            deposit,
            gas,
            confirmed,
            json,
        } => {
            cli::call::run(
                wallet_name,
                cli_network,
                &contract,
                &method,
                &args,
                deposit.as_deref(),
                gas,
                confirmed,
                json,
            )
            .await?;
        }
        Commands::View {
            contract,
            method,
            args,
        } => {
            cli::call::view(cli_network, &contract, &method, args.as_deref()).await?;
        }
        Commands::Swap { command } => match command {
            SwapCommand::Execute {
                from,
                to,
                amount,
                confirmed,
                json,
            } => {
                cli::swap::execute(wallet_name, cli_network, &from, &to, &amount, confirmed, json)
                    .await?;
            }
            SwapCommand::Quote {
                from,
                to,
                amount,
                json,
            } => {
                cli::swap::quote(wallet_name, cli_network, &from, &to, &amount, json).await?;
            }
            SwapCommand::Status {
                deposit_address,
                json,
            } => {
                cli::swap::status(&deposit_address, json).await?;
            }
            SwapCommand::Tokens { json } => {
                cli::swap::tokens(json).await?;
            }
        },
    }

    Ok(())
}
