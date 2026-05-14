# Validators Clock

Web dashboard for Everscale, Tycho, and TON validator rounds, elections,
stakes, rewards, wallet types, and recent validator history.

![Validators Clock screenshot](docs/validators-clock-screenshot.png)

## Run Locally

```bash
cd ~
git clone https://github.com/jouliene/validators_clock.git validators_clock
cd validators_clock
cargo run
```

Open:

```text
http://127.0.0.1:8787
```

If Rust is missing:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

## Install On Ubuntu Server

Point DNS to the server first. Ports `80` and `443` must be open.

Install packages:

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev curl git
```

Clone and install:

```bash
cd ~
git clone https://github.com/jouliene/validators_clock.git validators_clock
cd validators_clock
./install.sh
```

For another domain:

```bash
VALIDATORS_CLOCK_PUBLIC_URL=https://your-domain.example ./install.sh
```

`install.sh` checks Rust. If Rust is missing, it installs Rust with `rustup`.
If Rust is already managed by `rustup`, it updates Rust before building.

The script asks for `sudo` only for systemd work: installing the service file,
reloading systemd, enabling the service, and restarting the service.

## Update Production

```bash
cd ~/validators_clock
./update.sh
```

`update.sh` checks/updates Rust, runs:

```bash
git pull --ff-only origin main
```

and then runs `./install.sh`.

`--ff-only` is intentional. It updates production only when Git can move
straight to the GitHub version. Plain `git pull` can create a merge commit on
the server if there are local changes.

## Check Production

```bash
systemctl status validators-clock.service --no-pager
curl -sS https://validatorsclock.xyz/api/status
```

Logs:

```bash
sudo journalctl -u validators-clock.service -n 100 --no-pager
sudo journalctl -u validators-clock.service -f
```

## Files

Installed binary:

```text
~/.cargo/bin/validators_clock
```

Production data:

```text
~/.validators_clock
```

Important data files:

```text
validators_clock.production.json
validators_clock_history_everscale.json
validators_clock_history_tycho-testnet.json
validators_clock_history_ton.json
validators_clock_validator_types.json
acme/
```
