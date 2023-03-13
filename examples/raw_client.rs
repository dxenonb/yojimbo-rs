/*
 * Netcode client.c ported to Rust for testing.
 */
use yojimbo::{bindings::*, gf_init_default, PRIVATE_KEY_BYTES};
use std::mem::size_of;
use std::sync::mpsc::channel;
use std::thread::sleep;
use std::time::Duration;

fn main() {
    unsafe {
        if netcode_init() != NETCODE_OK as _ {
            eprintln!("failed to initialize netcode");
            return;
        }

        netcode_log_level(NETCODE_LOG_LEVEL_INFO as _);

        raw_client();

        netcode_term();
    }
}

unsafe fn raw_client() {
    let mut time = 0.0;
    let delta_time = 1.0 / 60.0;

    let mut client_config =
        gf_init_default!(netcode_client_config_t, netcode_default_client_config);
    let client = netcode_client_create(
        b"0.0.0.0\0".as_ptr() as *const i8 as *mut i8,
        &mut client_config as *mut _,
        time,
    );

    if client.is_null() {
        eprintln!("failed to create client");
        return;
    }

    let mut client_id: u64 = 0;
    unsafe { netcode_random_bytes(&mut client_id as *mut u64 as *mut u8, size_of::<u64>() as _) };
    println!("client id is {:02x}", client_id);

    let mut private_key = [0u8; PRIVATE_KEY_BYTES];
    let mut user_data = [0u8; 256];
    let mut connect_token = [0u8; NETCODE_CONNECT_TOKEN_BYTES as _];

    let protocol_id = 0;
    let connect_token_expiry = 30;
    let connect_token_timeout = 5;
    let server_address = b"127.0.0.1:40000\0";

    let ok = unsafe {
        netcode_generate_connect_token(
            1,
            &mut (server_address.as_ptr() as *const _),
            &mut (server_address.as_ptr() as *const _),
            connect_token_expiry,
            connect_token_timeout,
            client_id,
            protocol_id,
            private_key.as_mut_ptr(),
            user_data.as_mut_ptr(),
            connect_token.as_mut_ptr(),
        ) == (NETCODE_OK as i32)
    };

    if !ok {
        eprintln!("failed to generate connect token");
        return;
    }

    netcode_client_connect(client, connect_token.as_mut_ptr());

    let (stop_tx, stop_rx) = channel();
    ctrlc::set_handler(move || stop_tx.send(()).unwrap()).expect("Failed to set Ctrl-C handler");
    println!("client connecting; Ctrl-C to stop");

    loop {
        if stop_rx.try_recv().is_ok() {
            break;
        }

        netcode_client_update(client, time);

        // TODO: SEND TEST PACKET
        // TODO: RECEIVE PACKETS

        if netcode_client_state(client) < NETCODE_CLIENT_STATE_DISCONNECTED as _ {
            break;
        }

        sleep(Duration::from_secs_f64(delta_time));

        time += delta_time;
    }

    println!("client shutting down");

    netcode_client_destroy(client);
}
