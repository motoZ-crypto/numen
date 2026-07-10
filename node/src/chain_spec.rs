use sc_service::ChainType;
use serde_json::json;
use solochain_template_runtime::{
	configs::SS58Prefix, genesis_config_presets::{INTEGRATION_RUNTIME_PRESET, TESTNET_RUNTIME_PRESET, MAINNET_RUNTIME_PRESET}, WASM_BINARY,
};

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec;

fn chain_properties() -> sc_service::Properties {
	serde_json::from_value(json!({
		"ss58Format": SS58Prefix::get(),
		"tokenDecimals": 18,
		"tokenSymbol": "NUMN"
	}))
	.expect("valid properties")
}

pub fn development_chain_spec() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Numen Development")
	.with_id("dev")
	.with_protocol_id("numen")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
	.with_properties(chain_properties())
	.build())
}

pub fn integration_chain_spec() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Numen Integration Testnet")
	.with_id("integration")
	.with_protocol_id("numen")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name(INTEGRATION_RUNTIME_PRESET)
	.with_properties(chain_properties())
	.build())
}

pub fn local_chain_spec() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Numen Local Testnet")
	.with_id("local_testnet")
	.with_protocol_id("numen")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
	.with_properties(chain_properties())
	.build())
}

pub fn testnet_chain_spec() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Numen Testnet")
	.with_id("testnet")
	.with_protocol_id("numen")
	.with_chain_type(ChainType::Live)
	.with_genesis_config_preset_name(TESTNET_RUNTIME_PRESET)
	.with_properties(chain_properties())
	.build())
}

pub fn mainnet_chain_spec() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Numen Mainnet")
	.with_id("mainnet")
	.with_protocol_id("numen")
	.with_chain_type(ChainType::Live)
	.with_genesis_config_preset_name(MAINNET_RUNTIME_PRESET)
	.with_properties(chain_properties())
	.build())
}