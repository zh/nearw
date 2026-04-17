---
name: nearw
description: NEAR Protocol CLI wallet for balance queries, token transfers, contract calls, and token swaps via NEAR Intents. Use when interacting with NEAR blockchain or swapping tokens on NEAR.
---

# nearw — NEAR Protocol CLI Wallet

Rust CLI wallet for NEAR Protocol. Supports wallet management, NEAR/FT/NFT transfers, generic contract calls, and token swaps via the NEAR Intents protocol (1Click API).

## Commands

### Wallet & Balance

```bash
# Create a new wallet
nearw wallet create <name> [--network mainnet|testnet]

# Import from seed phrase
nearw wallet import <name> --mnemonic "word1 word2 ..."

# List wallets
nearw wallet list

# Check balance (NEAR + all fungible tokens)
nearw balance --json
nearw balance --token usdt.tether-token.near --json
```

### Send NEAR

```bash
# Send NEAR (will prompt for confirmation)
nearw send <receiver> <amount>

# Non-interactive (for AI agents)
nearw send <receiver> <amount> --confirmed --json

# Send all (always interactive, reserves 0.005 NEAR for gas)
nearw send-all <receiver>
```

### Send Fungible Tokens

```bash
# Send FT
nearw token send <receiver> <amount> <contract> --confirmed --json

# List FT holdings
nearw token list --json

# Token metadata
nearw token info <contract>
```

### Contract Calls

```bash
# Read-only view call (no wallet/signer needed)
nearw view <contract> <method> [json_args]

# State-changing call (requires wallet)
nearw call <contract> <method> <json_args> [--deposit 0.1] [--gas 100] --confirmed --json
```

Examples:
```bash
# Query wNEAR metadata
nearw view wrap.near ft_metadata

# Check FT balance for any account
nearw view usdt.tether-token.near ft_balance_of '{"account_id": "alice.near"}'

# Wrap NEAR to wNEAR
nearw call wrap.near near_deposit '{}' --deposit 1 --confirmed --json

# Storage deposit on a token contract
nearw call usdt.tether-token.near storage_deposit '{"account_id": "bob.near"}' --deposit 0.00125 --confirmed --json
```

### Token Swaps (NEAR Intents — mainnet only)

```bash
# List supported tokens
nearw swap tokens --json

# Get a quote (read-only, no funds spent)
nearw swap quote <from> <to> <amount> --json

# Execute a swap (spends funds!)
nearw swap execute <from> <to> <amount> --confirmed --json

# Check status of an existing swap
nearw swap status <deposit_address> --json
```

Token aliases: `NEAR`, `USDT`, `USDC`, `WETH`, `WBTC`, `AURORA`. Unknown tokens: use the contract ID directly.

Examples:
```bash
# Quote: how much USDC for 10 NEAR?
nearw swap quote NEAR USDC 10 --json

# Execute swap
nearw swap execute NEAR USDC 10 --confirmed --json

# Swap USDT to NEAR
nearw swap execute USDT NEAR 100 --confirmed --json

# Check swap status
nearw swap status d54c51921f25c80eac5ca6016b4423042c247dd621c32b042e24a029f77ceeea --json
```

## Decision Flow

When an agent needs to interact with NEAR:

1. **Read-only query** (balance, view, swap quote/tokens):
   - Execute directly — no approval needed, no funds at risk
   - Use `--json` for machine-readable output

2. **Token swap**:
   - First: `nearw swap quote <from> <to> <amount> --json` to check the rate
   - Inform user of the expected swap (amount in, expected out)
   - **Wait for explicit user approval**
   - Then: `nearw swap execute <from> <to> <amount> --confirmed --json`

3. **Send NEAR or tokens**:
   - Inform user of the amount and recipient
   - **Wait for explicit user approval**
   - Then: `nearw send <receiver> <amount> --confirmed --json`

4. **Contract call with deposit**:
   - Inform user of the contract, method, and deposit amount
   - **Wait for explicit user approval**
   - Then: `nearw call <contract> <method> <args> --deposit <amount> --confirmed --json`

## User Approval Required Before Any Transaction

**CRITICAL**: The agent MUST NOT execute fund-spending commands without explicit user approval. These commands spend real NEAR or tokens from the user's wallet:

- `nearw send` / `nearw send-all`
- `nearw token send`
- `nearw nft send`
- `nearw call` (with `--deposit`)
- `nearw swap execute`

Always:
1. Show the user what will happen (amount, recipient/swap details)
2. Wait for explicit confirmation ("yes", "go ahead", "do it")
3. Only then execute with `--confirmed --json`

Safe commands that need no approval:
- `nearw balance`, `nearw view`, `nearw swap quote`, `nearw swap tokens`, `nearw swap status`
- `nearw token list`, `nearw token info`, `nearw nft list`, `nearw history`
- `nearw wallet list`, `nearw wallet info`, `nearw receive`
- `nearw call` (without `--deposit` — still a transaction but zero cost)

## AI Agent Workflow

### Example: Swap NEAR for USDC

```
Agent: nearw swap quote NEAR USDC 10 --json
  -> {"amount_in": "10000000000000000000000000", "amount_out": "13250000", "amount_out_formatted": "13.25", ...}

Agent: "Swapping 10 NEAR for ~13.25 USDC. Approve?"
User: "yes"

Agent: nearw swap execute NEAR USDC 10 --confirmed --json
  -> {"status": "COMPLETED", "amount_out": "13250000", "deposit_tx_hash": "...", ...}
```

### Example: Check balance and send tokens

```
Agent: nearw balance --json
  -> {"near": "45.23", "tokens": [{"symbol": "USDT", "balance": "100.5", ...}]}

Agent: "You have 45.23 NEAR and 100.5 USDT. Send 50 USDT to bob.near?"
User: "yes"

Agent: nearw token send bob.near 50 usdt.tether-token.near --confirmed --json
  -> {"tx_hash": "...", "from": "...", "to": "bob.near", ...}
```

### Example: Query a contract

```
Agent: nearw view wrap.near ft_balance_of '{"account_id": "alice.near"}'
  -> "500000000000000000000000000"
  (= 500 wNEAR, 24 decimals)
```

## Key Options

| Option | Description |
|--------|-------------|
| `--json` | Machine-readable output (recommended for AI agents) |
| `--confirmed` | Skip confirmation prompt (only after user approval) |
| `--network mainnet\|testnet` | Override network (default: mainnet) |
| `-n <wallet>` | Target a specific wallet by name |
| `--deposit <amount>` | NEAR deposit for contract calls (e.g. "0.1" or "1yocto") |
| `--gas <tgas>` | Gas in TGas for contract calls (default: 100) |

## Token Aliases (for swaps)

| Alias | Contract | Decimals |
|-------|----------|----------|
| NEAR / WNEAR | wrap.near | 24 |
| USDT | usdt.tether-token.near | 6 |
| USDC | 17208628f...36133a1 | 6 |
| WETH / ETH | aurora | 18 |
| WBTC | 2260fac5e...c2c599.factory.bridge.near | 8 |
| AURORA | aaaaaa20d...07961.factory.bridge.near | 18 |

For tokens not in this list, pass the full contract ID: `nearw swap quote blackdragon.tkn.near NEAR 1000`

## Notes

- Wallet stored locally at `~/.nearw/` (credentials never leave the machine)
- Mainnet by default; testnet with `--network testnet`
- **Swaps are mainnet only** — `intents.near` has no testnet deployment
- Swap uses 1Click API (https://1click.chaindefuser.com) — handles quoting and settlement
- Native NEAR swaps auto-wrap to wNEAR before depositing
- 151+ tokens available for swaps (check with `nearw swap tokens`)
- Payment is per-swap, no batching
