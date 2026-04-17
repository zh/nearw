# nearw

CLI wallet for NEAR Protocol. Local key storage, named accounts, fungible tokens, NFTs, function-call keys, contract calls, and NEAR Intents swaps via the 1Click API.

## Install

```bash
cargo install --path .
```

## Quick Start

```bash
# Create a wallet (testnet is safe for trying things out)
nearw wallet create mywallet --network testnet

# Register a named account (free on testnet via faucet)
nearw wallet register -n mywallet

# Check balance
nearw balance -n mywallet

# Send NEAR
nearw send alice 0.1 -n mywallet

# Quote a swap (mainnet only)
nearw swap quote NEAR USDC 1

# Transaction history
nearw history -n mywallet
```

## Documentation

- **[USAGE.md](USAGE.md)** — full command reference: wallet management, FT/NFT, access keys, contract calls, NEAR Intents swaps, config, security, storage layout.
- **[skills/nearw/SKILL.md](skills/nearw/SKILL.md)** — agent-ready skill file. Drop this into Claude Code (or any skill-aware agent harness) to teach the agent how to drive `nearw` safely, including the required user-approval gates before any fund-spending command.
- **[PRD.md](PRD.md)** / **[research.md](research.md)** — design notes and background research.

## Command Groups

| Group | What it does |
|-------|--------------|
| `wallet` | create, import, export, list, default, delete, info, register, unregister |
| `balance` / `receive` / `send` / `send-all` | NEAR balance, address + QR, transfer, drain |
| `token` | FT list, info, send (NEP-141) |
| `nft` | NFT list, info, send (NEP-171) |
| `key` | Access key list/add/delete/generate (function-call + full-access) |
| `call` / `view` | Generic contract calls with JSON args, deposit, gas |
| `swap` | NEAR Intents swap (quote, execute, status, tokens — mainnet only) |
| `history` | Recent transactions |

Run `nearw <group> --help` for subcommands, or see [USAGE.md](USAGE.md) for full documentation with examples.

## Global Options

| Option | Description |
|--------|-------------|
| `-n, --name <wallet>` | Select wallet (accepts wallet name or NEAR account) |
| `--network <net>` | Override network (`mainnet` or `testnet`) |
| `--json` | Machine-readable JSON output |
| `--confirmed` | Skip confirmation prompt (for automation) |

## Storage

```
~/.nearw/                    # 0700
├── config.toml              # Optional: custom RPC + 1Click settings
├── default                  # Default wallet name
└── wallets/                 # 0700
    ├── mywallet             # Seed phrase (0600)
    ├── mywallet.net         # Network
    ├── mywallet.account     # Named account (e.g. alice.near)
    └── mywallet.keys        # Function-call keys JSON (0600)
```

Override location with `NEARW_HOME=/path/to/dir`. Full details in [USAGE.md](USAGE.md#storage).

## Security

Plaintext seed phrases with `0600` perms (same model as `near-cli-rs`). All value-moving commands require interactive confirmation; swaps are mainnet-only and validate 1Click deposit addresses. See [USAGE.md](USAGE.md#security) for the full list.

## Dependencies

Built on [near-api](https://crates.io/crates/near-api) (v0.8). Token/NFT discovery via [Nearblocks API](https://nearblocks.io/). Swaps via the 1Click API at https://1click.chaindefuser.com.

Includes a vendored patch for `slipped10` (supply-chain fix).

## License

MIT
