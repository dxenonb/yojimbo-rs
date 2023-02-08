use std::{sync::mpsc::channel, thread::sleep, time::Duration};

use rust_game_networking::{
    config::ClientServerConfig, initialize, log_level, server::Server, shutdown, LogLevel,
    PRIVATE_KEY_BYTES,
};

fn main() {
    initialize().unwrap();
    log_level(LogLevel::Debug);

    server_main();

    shutdown();
}

struct Message(u32);

fn server_main() {
    let mut time = 100.0;

    let config = ClientServerConfig::default();
    let max_clients = 16;
    let private_key = [0; PRIVATE_KEY_BYTES];

    let server_address = "127.0.0.1:40000".to_string();
    println!("starting server on port {} (insecure)", &server_address);

    let mut server: Server<Message> = Server::new(&private_key, server_address, config, time);
    server.start(max_clients);

    let (stop_tx, stop_rx) = channel();
    ctrlc::set_handler(move || stop_tx.send(()).unwrap()).expect("Failed to set Ctrl-C handler");
    println!("server started; Ctrl-C to stop");

    let delta_time = 0.01;
    loop {
        if stop_rx.try_recv().is_ok() {
            break;
        }

        server.send_packets();
        server.receive_packets();

        time += delta_time;

        server.advance_time(time);

        if !server.running() {
            println!("server not running");
            break;
        }

        sleep(Duration::from_secs_f64(delta_time));
    }

    println!("stopping server");
    server.stop();
    println!("server stopped");
}
