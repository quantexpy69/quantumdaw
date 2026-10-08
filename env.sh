# Uso: source env.sh  — activa el toolchain local de Rust del proyecto.
export RUSTUP_HOME="$(pwd)/.toolchain/rustup"
export CARGO_HOME="$(pwd)/.toolchain/cargo"
export PATH="$CARGO_HOME/bin:$PATH"
