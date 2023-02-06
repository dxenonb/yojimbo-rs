use std::{mem::size_of, sync::mpsc::channel, thread::sleep, time::Duration};

use rust_game_networking::{
    bindings::netcode_random_bytes, client::Client, config::ClientServerConfig, initialize,
    log_level, shutdown, LogLevel, PRIVATE_KEY_BYTES,
};

fn main() {
    initialize().unwrap();
    log_level(LogLevel::Debug);

    client_main();

    shutdown();
}

fn client_main() {
    println!("connecting client (insecure)");

    let mut time = 100.0;

    let mut client_id: u64 = 0;
    unsafe { netcode_random_bytes(&mut client_id as *mut u64 as *mut u8, size_of::<u64>() as _) };

    println!("client id is {:x}", client_id);

    let config = ClientServerConfig::default();
    let mut client = Client::new("0.0.0.0".to_string(), config, time);

    let server_address = "127.0.0.1:40000".to_string();

    let private_key = [0; PRIVATE_KEY_BYTES];

    client.insecure_connect(&private_key, client_id, &[&server_address]);

    let (stop_tx, stop_rx) = channel();
    ctrlc::set_handler(move || stop_tx.send(()).unwrap()).expect("Failed to set Ctrl-C handler");
    println!("client connecting; Ctrl-C to stop");

    let delta_time = 0.01;

    loop {
        if stop_rx.try_recv().is_ok() {
            break;
        }

        client.send_packets();
        client.receive_packets();

        if client.is_disconnected() {
            break;
        }

        time += delta_time;

        client.advance_time(delta_time);

        if client.connection_failed() {
            break;
        }

        sleep(Duration::from_secs_f64(delta_time));
    }

    client.disconnect()
}
