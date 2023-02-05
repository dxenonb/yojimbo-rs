use rust_game_networking::{
    config::ClientServerConfig, initialize, log_level, server::Server, shutdown, LogLevel,
    PRIVATE_KEY_BYTES,
};

#[test]
fn server() {
    initialize().unwrap();
    log_level(LogLevel::Debug);

    server_main();

    shutdown();
}

fn server_main() {
    let time = 100.0;

    let config = ClientServerConfig::default();
    let max_clients = 16;
    let private_key = [0; PRIVATE_KEY_BYTES];

    let server_address = "127.0.0.1:40000".to_string();
    println!("starting server on port {} (insecure)", &server_address);

    let mut server = Server::new(&private_key, server_address, config, time);
    server.start(max_clients);

    println!("server started");

    // TODO: loop

    println!("stopping server");
    server.stop();
}
