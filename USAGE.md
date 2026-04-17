# nearw

CLI wallet for NEAR Protocol. Local key storage, named accounts, fungible tokens, NFTs, function-call keys, generic contract calls, and NEAR Intents swaps via the 1Click API.

## Features

- **Multi-wallet** — create, import, and switch between named wallets
- **BIP39 seed** — 12-word mnemonic, ed25519 derivation compatible with near-cli / MyNearWallet
- **Implicit and named accounts** — 64-char hex implicit accounts out of the box; register a `.near` / `.testnet` account when you want one
- **NEAR send/receive** — with QR codes, `send-all` drain (keeps a small storage reserve)
- **Fungible tokens (NEP-141)** — list, info, send; auto-discovered via the Nearblocks indexer
- **NFTs (NEP-171)** — list, info, send; auto-discovered via the Nearblocks indexer
- **Access keys** — list, add function-call keys scoped to a contract/methods, add full-access keys, delete, generate offline
- **Generic contract calls** — `call` (state-changing) and `view` (read-only) with JSON args, deposits, and gas overrides
- **NEAR Intents swap** — `quote`, `execute`, `status`, `tokens` via https://1click.chaindefuser.com (mainnet only; native NEAR auto-wraps to wNEAR)
- **Transaction history** — recent signatures with block time, status, and explorer links
- **Mainnet / testnet** — network auto-detected per wallet
- **Local key management** — private keys never leave your machine
- **Agent-friendly** — `--json` output and `--confirmed` flags on every value-moving command

## Install

```bash
cargo install --path .
```

## Agent Skill

If you want an AI agent (Claude Code, etc.) to drive `nearw`, install the packaged skill at `skills/nearw/SKILL.md`. It documents commands, decision flow, required user-approval gates, and example transcripts. Point your agent harness at that file, or copy it into your skills directory.

## Quick Start

```bash
# Create a wallet (testnet is safe for trying things out)
nearw wallet create mywallet --network testnet

# Register a named account (free on testnet via faucet)
nearw wallet register -n mywallet

# Check balance
nearw balance

# Receive address + QR
nearw receive

# Send NEAR
nearw send <recipient> 0.1

# List fungible tokens
nearw token list

# Quote a swap (no funds move; mainnet only)
nearw swap quote NEAR USDC 1

# Transaction history
nearw history
```

## Wallet Management

```bash
# Create a new wallet with a 12-word seed phrase
nearw wallet create <name> [--network <mainnet|testnet>]

# Import an existing wallet (prompts for seed phrase)
nearw wallet import <name> [--network <mainnet|testnet>]

# List all wallets
nearw wallet list

# Show the selected wallet's info + on-chain balance
nearw wallet info

# Switch default wallet
nearw wallet default <name>

# Export seed phrase
nearw wallet export

# Delete a wallet
nearw wallet delete <name>

# Register a named account (suffix added automatically per network)
nearw wallet register [account]

# Clear the named-account association (revert to implicit account)
nearw wallet unregister
```

Use `-n <name>` with any command to target a specific wallet without changing the default:

```bash
nearw -n mywallet balance
nearw -n devtest token list
```

`-n` also accepts a fully qualified NEAR account id (e.g. `alice.near`) — nearw will match it against your stored wallets.

## Networks

`nearw` supports two clusters:

- **mainnet** — https://rpc.mainnet.near.org; named accounts end in `.near`
- **testnet** — https://rpc.testnet.near.org; named accounts end in `.testnet` (free registration via faucet)

The network is stored per wallet in `~/.nearw/wallets/<name>.net` at creation and auto-used thereafter. Override on a single command with `--network <name>`:

```bash
nearw wallet create prod                         # defaults to mainnet
nearw wallet create devtest --network testnet
nearw -n devtest balance                         # auto-detects testnet
nearw --network testnet balance                  # one-off override
```

`swap` subcommands are hard-restricted to mainnet (the NEAR Intents `intents.near` contract has no testnet deployment).

### Custom RPC endpoints (config.toml)

The public `rpc.mainnet.near.org` / `rpc.testnet.near.org` endpoints are rate-limited and sometimes unreliable for indexer-style reads. To use private RPC providers (FastNear, Lava, Pagoda, etc.), drop a TOML file at `~/.nearw/config.toml`:

```toml
[rpc]
mainnet = ["https://near.lava.build", "https://rpc.mainnet.fastnear.com"]
testnet = ["https://rpc.testnet.fastnear.com"]
```

Each list is tried in order; the first healthy endpoint serves the request. Unset keys fall through to the built-in default.

The same file holds optional 1Click swap settings:

```toml
# Override the 1Click API base URL (HTTPS required on mainnet)
oneclick_api = "https://1click.chaindefuser.com"
# JWT for 0% platform fee (alternatively, set ONECLICK_JWT env var)
oneclick_jwt = "eyJ..."
```

Precedence for `oneclick_jwt`: `ONECLICK_JWT` env var > `config.toml` > unset.

## Balance & Addresses

```bash
# NEAR balance + storage reservation
nearw balance
nearw balance --json

# Single-token balance
nearw balance --token <ft-contract>

# Receive address + QR code
nearw receive
nearw receive --no-qr
```

## Sending NEAR

```bash
# Send NEAR (amount in NEAR unless --unit yocto)
nearw send <recipient> <amount>

# Amounts in yoctoNEAR (1 NEAR = 10^24 yocto)
nearw send <recipient> 1000000000000000000000000 --unit yocto

# Drain wallet to an address (keeps ~0.005 NEAR storage reserve)
nearw send-all <recipient>

# Skip interactive confirmation (for automation)
nearw send <recipient> 0.1 --confirmed

# Machine-readable output
nearw send <recipient> 0.1 --confirmed --json
```

`send-all` withholds ~0.005 NEAR so the source account stays above its storage reserve. It always prompts interactively and does not accept `--confirmed`.

Recipients auto-resolve against the sender's network:

```bash
nearw send alice 1          # -> alice.near (mainnet) or alice.testnet (testnet)
nearw send alice.near 1     # -> used as-is
nearw send mywallet 1       # -> if "mywallet" is a local wallet, sends to its account
```

## Fungible Tokens (NEP-141)

```bash
# List non-empty FT balances (auto-discovered via Nearblocks indexer)
nearw token list

# Show FT contract metadata (name, symbol, decimals, icon)
nearw token info <contract>

# Send tokens (amount in UI units; e.g. 1.5 for 1.5 USDT)
nearw token send <contract> <recipient> <amount>

# Automation + JSON
nearw token send <contract> <recipient> 1.5 --confirmed
```

`token send` automatically handles the recipient's `storage_deposit` when required by the token contract.

## NFTs (NEP-171)

```bash
# List NFTs across all contracts (auto-discovered via Nearblocks indexer)
nearw nft list

# List NFTs from a specific contract
nearw nft list <contract>

# Show NFT contract metadata
nearw nft info <contract>

# Send an NFT by token id
nearw nft send <contract> <token-id> <recipient>
nearw nft send <contract> <token-id> <recipient> --confirmed
```

## Access Keys

NEAR splits signing authority across multiple access keys per account. Each key is either full-access (can do anything) or function-call (restricted to a contract and optional method list).

```bash
# List all keys on the signing account
nearw key list

# Add a function-call key (default: 0.25 NEAR gas allowance, all methods)
nearw key add <contract>
nearw key add <contract> --methods "ft_transfer,ft_transfer_call" --allowance 0.5

# Reuse an existing public key instead of generating a fresh one
nearw key add <contract> --public-key ed25519:...

# Add a full-access key (grants complete control — be careful)
nearw key add-full-access
nearw key add-full-access --public-key ed25519:...

# Delete a key by public key
nearw key delete <public-key>

# Generate a keypair offline (does not touch the account)
nearw key generate

# Import a function-call key from a seed phrase and store it locally
nearw key import <contract>
```

Function-call keys generated by `key add` are stored in `~/.nearw/wallets/<name>.keys`. They can be used by dApps/tools that read that file but cannot transfer NEAR. `key delete` refuses to remove the wallet's own signing key.

## Contract Calls

```bash
# Read-only view call — no signer required, no gas cost
nearw view <contract> <method> [json_args]

# State-changing call — requires the wallet to sign
nearw call <contract> <method> <json_args> \
    [--deposit <amount>] [--gas <tgas>] [--confirmed] [--json]
```

- `<json_args>` is a JSON object passed to the method (e.g. `'{"account_id":"alice.near"}'`). `view` accepts it as optional; omitting defaults to `{}`.
- `--deposit` accepts NEAR units (`0.1`) or yocto (`1yocto`). Defaults to zero.
- `--gas` is in TGas (default: 100).

Examples:

```bash
# Query an FT balance (view, no wallet state change)
nearw view usdt.tether-token.near ft_balance_of '{"account_id":"alice.near"}'

# Wrap 1 NEAR to wNEAR
nearw call wrap.near near_deposit '{}' --deposit 1 --confirmed

# Storage-deposit for a recipient on a token contract
nearw call usdt.tether-token.near storage_deposit \
    '{"account_id":"bob.near"}' --deposit 0.00125 --confirmed

# Unwrap 1 wNEAR
nearw call wrap.near near_withdraw \
    '{"amount":"1000000000000000000000000"}' --deposit 1yocto --confirmed
```

## Transaction History

```bash
# Recent transactions (default limit 20)
nearw history

# Custom limit
nearw history --limit 50

# JSON output
nearw history --json
```

History is fetched from the Nearblocks indexer. Each entry shows the tx hash, block time, method called, status, and explorer URL.

## NEAR Intents Swap (mainnet only)

The hero feature. `swap quote` and `swap tokens` are read-only. `swap execute` moves real funds and requires explicit approval.

```bash
# List supported tokens (151+ including cross-chain assets)
nearw swap tokens [--json]

# Read-only quote
nearw swap quote <from> <to> <amount> [--json]

# Execute on mainnet (wallet required)
nearw swap execute <from> <to> <amount> [--confirmed] [--json]

# Poll an existing swap by deposit address
nearw swap status <deposit_address> [--json]
```

`<amount>` is the input in **UI units** (e.g. `10` for 10 NEAR, `100` for 100 USDT). Decimals are taken from the token alias map or fetched from `ft_metadata` on-chain.

Default slippage is 100 bps (1%). Quote example:

```bash
nearw swap quote NEAR USDC 10
# From:     10 NEAR
# Expected: 13.25 USDC
# Deposit:  <64-char hex implicit account>
# Est swap: ~20s
# Valid:    2026-04-10T05:50Z (14:50 JST, in 28min)
```

Execute example (wallet must hold the input token; native NEAR is auto-wrapped to wNEAR first):

```bash
nearw -n mainnet swap execute NEAR USDC 10
# interactive confirm prompt unless --confirmed
# wraps 10 NEAR → wNEAR, then ft_transfer_call to the 1Click deposit address
# polls status every 2-10s up to 5 minutes
```

If `nearw` wraps successfully but the deposit transfer fails, it prints exact recovery commands — your wNEAR stays safe in your account and can be unwrapped or retried.

### Built-in Token Aliases

| Alias | Contract | Decimals |
|-------|----------|----------|
| `NEAR`, `WNEAR`, `WRAP` | `wrap.near` | 24 |
| `USDT` | `usdt.tether-token.near` | 6 |
| `USDC` | `17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1` | 6 |
| `WETH`, `ETH` | `aurora` | 18 |
| `WBTC` | `2260fac5e5542a773aa44fbcfedf7c193bc2c599.factory.bridge.near` | 8 |
| `AURORA`, `AOA` | `aaaaaa20d9e0e2461697782ef11675f668207961.factory.bridge.near` | 18 |

Any other string is treated as a literal NEP-141 contract id and its decimals are pulled from on-chain metadata. Run `nearw swap tokens` for the full list.

## Global Options

| Option | Applies to | Description |
|--------|------------|-------------|
| `-n, --name <wallet>` | all commands | Select wallet by name or by NEAR account id (falls back to default) |
| `--network <net>` | all commands | Override stored network: `mainnet` or `testnet` |
| `--confirmed` | `send`, `token send`, `nft send`, `call`, `swap execute` | Skip the interactive confirmation prompt |
| `--json` | most commands | Machine-readable JSON output |
| `--unit <near\|yocto>` | `send` | Interpret amount as NEAR (default) or yoctoNEAR |
| `--deposit <amount>` | `call` | Attach NEAR as deposit (e.g. `0.1` or `1yocto`) |
| `--gas <tgas>` | `call` | Gas in TGas (default: 100) |

## Storage

```
~/.nearw/                       # 0700
├── config.toml                 # Optional: custom RPC + 1Click settings
├── default                     # Name of the default wallet
└── wallets/                    # 0700
    ├── <name>                  # BIP39 mnemonic (0600)
    ├── <name>.net              # Network: mainnet | testnet
    ├── <name>.account          # Named account, if registered (e.g. alice.near)
    └── <name>.keys             # Function-call keys JSON (0600)
```

Override the storage root with `NEARW_HOME=/path/to/dir`.

## Security

- **Key storage** — plaintext seed phrases with `0600` file permissions; private keys never leave disk. Same model as `near-cli-rs` file-system wallets.
- **Confirmation prompts** — all value-moving commands (`send`, `token send`, `nft send`, `call` with `--deposit`, `swap execute`) require interactive confirmation. Use `--confirmed` for automation.
- **`send-all` is always interactive** — no `--confirmed` bypass.
- **Mainnet guard on swaps** — `swap execute` / `quote` refuse to run on any network other than mainnet (NEAR Intents only deploys there).
- **Deposit-address validation** — `swap execute` rejects 1Click responses whose `deposit_address` is not a valid 64-char hex implicit account.
- **HTTPS-only** — built-in RPC endpoints and 1Click overrides must use HTTPS.
- **Key-deletion safety** — `key delete` refuses to remove the wallet's own signing key.
- **Reserved names** — wallet names `default` and `config` are rejected to prevent collisions with metadata files.

## Dependencies

Built on [`near-api`](https://crates.io/crates/near-api) (v0.8) for RPC, signing, and transaction assembly. Token/NFT discovery uses the [Nearblocks](https://nearblocks.io/) indexer. Swaps go through the 1Click API at https://1click.chaindefuser.com — `/quote` for route pricing and `/deposit/submit` to notify after the on-chain `ft_transfer_call`; nearw then polls `/status/<deposit_address>` until terminal.

Includes a vendored patch for the `slipped10` crate (supply-chain fix).

## License

MIT
