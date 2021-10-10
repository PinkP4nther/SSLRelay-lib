use sslrelay::{self, RelayConfig, HandlerCallbacks, CallbackRet, TCPDataType, TLSConfig};

// Handler object
#[derive(Clone)] // Must have Clone trait implemented.
struct Handler;

/*
    Callback traits that can be used to read or inject data
    into data upstream or downstream.
*/
impl HandlerCallbacks for Handler {

    // DownStream non blocking callback
    fn ds_nb_callback(&self, _in_data: Vec<u8>) {
        println!("[CALLBACK] Down Stream Non Blocking CallBack!");
    }

    // DownStream blocking callback
    fn ds_b_callback(&mut self, _in_data: Vec<u8>) -> CallbackRet {
        println!("[CALLBACK] Down Stream Blocking CallBack!");
        CallbackRet::Relay(_in_data)
    }

    // UpStream non blocking callback
    fn us_nb_callback(&self, _in_data: Vec<u8>) {
        println!("[CALLBACK] Up Stream Non Blocking CallBack!");
    }

    // UpStream blocking callback
    fn us_b_callback(&mut self, _in_data: Vec<u8>) -> CallbackRet {
        println!("[CALLBACK] Up Stream Blocking CallBack!");
        CallbackRet::Relay(_in_data)
    }
}

fn main() {

    // Create new SSLRelay object
    let mut relay = sslrelay::SSLRelay::new(
        Handler, 
        RelayConfig {
            downstream_data_type: TCPDataType::TLS,
            upstream_data_type: TCPDataType::TLS,
            bind_host: "0.0.0.0".to_string(),
            bind_port: "443".to_string(),
            remote_host: "remote.com".to_string(),
            remote_port: "443".to_string(),
            tls_config: TLSConfig::FILE{
                certificate_path: "./tls.crt".to_string(),
                private_key_path: "./tls.key".to_string(),
            },
        }
    );

    // Start listening
    relay.start();
}