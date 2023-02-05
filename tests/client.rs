use std::mem::size_of;

use rust_game_networking::{
    bindings::netcode_random_bytes, client::Client, config::ClientServerConfig, initialize,
    shutdown, PRIVATE_KEY_BYTES,
};

#[test]
fn client() {
    initialize().unwrap();

    client_main();

    shutdown();
}

fn client_main() {
    let time = 100.0;

    let mut client_id: u64 = 0;
    unsafe { netcode_random_bytes(&mut client_id as *mut u64 as *mut u8, size_of::<u64>() as _) };

    println!("client id is {:x}", client_id);

    let config = ClientServerConfig::default();
    let mut client = Client::new("0.0.0.0".to_string(), config, time);

    let server_address = "127.0.0.1:40000".to_string();

    let private_key = [0; PRIVATE_KEY_BYTES];

    client.insecure_connect(&private_key, client_id, &[&server_address]);
}
