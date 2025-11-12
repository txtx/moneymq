<div align="center">
  <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/txtx/moneymq/main/docs/assets/moneymq-github-hero-dark.png">
      <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/txtx/moneymq/main/docs/assets/moneymq-github-hero-light.png">
      <img alt="Stablecoin Payment Engine" style="max-width: 60%;">
  </picture>
</div>

## ⚡️ Open payment engine for stablecoins

Build, test and deploy stablecoin payments right from your laptop.
Simulate invoices, subscriptions, or pay-per-API calls — then deploy anywhere in one click.
Your cashflow earns yield by default. Open-source, portable, and built on modern standards (x402-ready).

## MoneyMQ in action: 101 Series
<a href="https://www.youtube.com/playlist?list=PL0FMgRjJMRzPXNmWJsnOzpoPTPMuSlL2j">
  <picture>
    <source srcset="https://raw.githubusercontent.com/txtx/moneymq/main/docs/assets/youtube.png">
    <img alt="MoneyMQ 101 series" style="max-width: 100%;">
  </picture>
</a>

## 💡 Key Features

### 🛠️ Integrated Local Environment
Spin up your entire payment stack locally — product catalog, billing rules and test accounts — without touching external APIs.
Simulate checkouts, test 402-gated endpoints, and preview your logic in a real sandbox before deployment.
MoneyMQ runs offline, integrates seamlessly with your existing tools, and treats **billing as declarative YAML code** — ensuring your production setup is reproducible and predictable.

### 🌎 The Easiest Way to Build with x402
**MoneyMQ** comes with native support for x402 payment flows.
It automatically provisions a local sandbox with all required components — letting you test, iterate, and deploy x402 integrations in minutes.

### 🧠 MCP Embedded and Agent-Ready
With an embedded MCP runtime, MoneyMQ connects your agent directly to your payment stack and codebase.
This enables autonomous agents to design, test, and optimize payment strategies — using real context from your codebase and environment.

### 💰 Embedded Yield (Coming soon)
Your balances don’t just sit — they earn.
Idle stablecoin liquidity is automatically routed to yield strategies, so your cashflow grows by default.

---

## 🧩 Installation

Install pre-built binaries:

```console
# macOS (Homebrew)
brew install txtx/taps/moneymq

# Updating MoneyMQ for Homebrew users
brew tap txtx/taps
brew reinstall moneymq
```

Install from source:

```console
# Clone repo
git clone https://github.com/txtx/moneymq.git

# Enter repo
cd moneymq

# Build
cargo moneymq-install
```

Or use Docker:

```console
docker run moneymq/moneymq --version
```

Verify installation:

```console
moneymq --version
```

---

## 🚀 Usage

Start a local payment environment:

```console
moneymq sandbox
```

From there, you can:
- Define products, prices, and plans locally (or import from Stripe).
- Simulate checkout sessions and API metering.
- Test x402-gated endpoints before going live.

## ⭐️ How to Contribute

Start by starring the repository!

We are actively developing MoneyMQ and we welcome contributions from the community. If you'd like to get involved, here’s how:

- Join the discussion on [Discord](https://discord.gg/rqXmWsn2ja)

- Explore and contribute to open issues: [GitHub Issues](https://github.com/txtx/moneymq/issues?q=is%3Aissue%20state%3Aopen%20label%3A%22help%20wanted%22)

- Get releases updates via [X](https://x.com/txtx_sol)

Your contributions help shape the future of MoneyMQ.
