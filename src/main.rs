use dotenv::dotenv;
use ethers::{
    abi::{encode_packed, Abi, Token},
    prelude::Contract,
    providers::{Http, Middleware, Provider},
    types::Chain,
    types::{Address, U256},
    utils::{self, to_checksum},
};
use ethers_etherscan::Client;
use eyre::Result;
use std::env;
use std::sync::Arc;

const ROCKET_STORAGE_ADDRESS: &str = "0x1d8f8f00cfa6758d7bE78336684788Fb0ee0Fa46";
const RPL_CHECKPOINT_BLOCKS: u64 = 5760;
const ETH_SECONDS_PER_BLOCK: i64 = 12;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let rocket_storage_address: Address = ROCKET_STORAGE_ADDRESS.parse::<Address>()?;

    let rocket_storage_abi: Abi = get_contract_abi(rocket_storage_address).await?;

    let provider = Provider::<Http>::try_from(format!(
        "https://eth-mainnet.gateway.pokt.network/v1/lb/{}",
        env::var("PORTAL_ID")?
    ))?;
    let client = Arc::new(provider);

    let rocket_storage_contract =
        Contract::new(rocket_storage_address, rocket_storage_abi, client.clone());

    let rocket_network_prices_abi: Vec<u8> = encode_packed(&[
        Token::String(String::from("contract.address")),
        Token::String(String::from("rocketNetworkPrices")),
    ])?;
    let salt = utils::keccak256(rocket_network_prices_abi);

    let rocket_network_prices: Address = rocket_storage_contract
        .method::<_, Address>("getAddress", salt)?
        .call()
        .await?;
    let rocket_network_prices_abi: Abi = get_contract_abi(rocket_network_prices).await?;

    let rocket_network_prices_contract = Contract::new(
        rocket_network_prices,
        rocket_network_prices_abi,
        client.clone(),
    );
    let rpl_price = rocket_network_prices_contract
        .method::<_, U256>("getRPLPrice", ())?
        .call()
        .await?;
    let rpl_price: f64 = rpl_price.as_usize() as f64 / 1e18;
    println!("Current RPL checkpoint price: {0:.6}", rpl_price);

    let prices_block = rocket_network_prices_contract
        .method::<_, U256>("getPricesBlock", ())?
        .call()
        .await?;

    let blocks_until_next_price_update: i64 = (RPL_CHECKPOINT_BLOCKS
        - (client.get_block_number().await?.as_u64() - prices_block.as_u64()))
    .try_into()
    .unwrap();

    let hours_until_next_price_update =
        blocks_until_next_price_update * ETH_SECONDS_PER_BLOCK / 60 / 60;
    let minutes_until_next_price_update = blocks_until_next_price_update * ETH_SECONDS_PER_BLOCK
        / 60
        - hours_until_next_price_update * 60;

    let hours = if hours_until_next_price_update == 1 {
        "hour"
    } else {
        "hours"
    };

    let minutes = if minutes_until_next_price_update == 1 {
        "minute"
    } else {
        "minutes"
    };

    println!("Next price update in {blocks_until_next_price_update} blocks, or {hours_until_next_price_update} {hours} and {minutes_until_next_price_update} {minutes}");

    Ok(())
}

async fn get_contract_abi(contract_address: Address) -> Result<Abi> {
    let contract_address_str = to_checksum(&contract_address, None);

    let file = std::fs::File::open(format!("./.{contract_address_str}.json"));

    let contract_abi: Abi = if file.is_ok() {
        println!("Found cached ABI for {contract_address_str}");
        serde_json::from_reader(file?)?
    } else {
        let client_etherscan = Client::new_from_env(Chain::Mainnet)?;

        let metadata = client_etherscan
            .contract_source_code(contract_address)
            .await?;
        let contract_abi = metadata.items[0].abi.as_str();

        let contract_abi: Abi = serde_json::from_str(contract_abi)?;
        serde_json::to_writer_pretty(
            std::fs::File::create(format!("./.{contract_address_str}.json"))?,
            &contract_abi,
        )?;
        contract_abi
    };
    Ok(contract_abi)
}

fn _print_type_of<T>(_: &T) {
    println!("{}", std::any::type_name::<T>())
}
