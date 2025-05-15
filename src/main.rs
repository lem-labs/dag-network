mod network;
mod event_loop;
mod dag;
mod api;

use crate::api::Api;
use std::io::Write;
use std::{error::Error, io};
use tokio;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let secret_key_seed = prompt_input("Enter secret key seed: ");
    let p2p_port = prompt_input("Enter p2p port: ");
    let api_port = prompt_input("Enter api port: ");

    let (mut event_loop, api_sender) = network::new(&*secret_key_seed, p2p_port).await?;
    let api = Api::new(api_sender);

    let event_loop_task = tokio::spawn(async move {
        event_loop.run().await;
    });

    let api_task = tokio::spawn(async move {
        api.run(api_port.parse().unwrap()).await;
    });


    api_task.await?;

    event_loop_task.await?;

    Ok(())

}

fn prompt_input(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).expect("Failed to read input");
    input.trim().to_string()
}


