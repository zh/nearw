# nearw

CLI wallet for NEAR Protocol. Local key storage, named accounts, token/NFT management.

## Install

```bash
cargo install --path .
```

## Quick Start

```bash
# Create a wallet (offline, no network needed)
nearw wallet create mywallet --network testnet

# Register a named account (free on testnet via faucet)
nearw wallet register -n mywallet

# Check balance
nearw balance -n mywallet

# Send NEAR
nearw send alice 0.1 -n mywallet

# View tokens
nearw token list -n mywallet

# Transaction history
nearw history -n mywallet
```

## Commands

| Command | Description |
|---------|-------------|
| `wallet create <name>` | Create new wallet (generates seed phrase) |
| `wallet import <name>` | Import wallet from seed phrase |
| `wallet export` | Show seed phrase |
| `wallet info` | Show wallet details + on-chain balance |
| `wallet list` | List all wallets |
| `wallet default <name>` | Set default wallet |
| `wallet delete <name>` | Delete a wallet |
| `wallet register [account]` | Register a named account (.near/.testnet) |
| `wallet unregister` | Clear named account association |
| `balance` | Check NEAR balance |
| `balance --token <contract>` | Check FT balance |
| `balance --json` | Output as JSON |
| `send <recipient> <amount>` | Send NEAR |
| `send-all <recipient>` | Send all NEAR (drain wallet) |
| `receive` | Show receive address + QR code |
| `token list` | List all FT balances (auto-discovered) |
| `token info <contract>` | Show FT contract metadata |
| `token send <contract> <recipient> <amount>` | Send fungible tokens |
| `nft list` | List all NFTs (auto-discovered) |
| `nft list <contract>` | List NFTs from a specific contract |
| `nft info <contract>` | Show NFT contract metadata |
| `nft send <contract> <token-id> <recipient>` | Send an NFT |
| `history` | Transaction history |
| `key list` | List all access keys on the account |
| `key add <contract>` | Add function-call key for a contract |
| `key add-full-access` | Add full-access key (dangerous) |
| `key delete <public-key>` | Delete an access key |
| `key generate` | Generate a new key pair offline |
| `key import <contract>` | Import key from seed phrase |

## Global Options

| Option | Description |
|--------|-------------|
| `-n, --name <wallet>` | Select wallet (accepts wallet name or NEAR account) |
| `--network <net>` | Override network (mainnet/testnet) |
| `--json` | Machine-readable JSON output (balance, send, token list, nft list, history) |
| `--confirmed` | Skip confirmation prompt (send, token send, nft send) |

## Storage

```
~/.nearw/                    # 0700
├── config.toml              # Optional: custom RPC endpoints
├── default                  # Default wallet name
└── wallets/                 # 0700
    ├── mywallet             # Seed phrase (0600)
    ├── mywallet.net         # Network (mainnet/testnet)
    ├── mywallet.account     # Named account (e.g. alice.near)
    └── mywallet.keys        # Function-call keys JSON (0600)
```

Override storage location: `NEARW_HOME=/path/to/dir`

## Networks

- **Mainnet**: default. Named accounts end in `.near`
- **Testnet**: use `--network testnet`. Named accounts end in `.testnet`. Free account creation via faucet.

The network is saved per wallet on creation. No need to pass `--network` on every command.

## Recipient Resolution

Recipients auto-resolve based on the sender's network:

```bash
nearw send alice 1          # → alice.near (mainnet) or alice.testnet (testnet)
nearw send alice.near 1     # → used as-is
nearw send mywallet 1       # → if "mywallet" is a local wallet, sends to its account
```

## Security

- **Key storage**: Plaintext seed phrases with 0600 file permissions. Same model as near-cli-rs.
- **Function-call keys**: Scoped keys for contract interactions. Stored in `.keys` file. Cannot transfer NEAR.
- **Confirmation prompts**: All value transfers require interactive confirmation. Use `--confirmed` for automation.
- **send-all**: Always requires interactive confirmation (no `--confirmed` bypass).
- **Key deletion safety**: Cannot delete the wallet's own signing key.
- **HTTPS required**: Custom RPC endpoints must use HTTPS.

## Dependencies

Built on [near-api](https://crates.io/crates/near-api) (v0.8). Token/NFT discovery via [Nearblocks API](https://nearblocks.io/).

Includes a vendored patch for `slipped10` crate (supply chain fix).

## License

MIT
