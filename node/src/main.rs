//! Substrate Node Template CLI library.
#![warn(missing_docs)]
#![allow(clippy::result_large_err)]

mod benchmarking;
mod chain_spec;
mod cli;
mod command;
mod rpc;
mod service;

fn main() -> sc_cli::Result<()> {
	command::run()
}
