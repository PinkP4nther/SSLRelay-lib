use sslrelay::{self, ConfigType, HandlerCallbacks};

// Handler object
#[derive(Clone)] // Must have Clone trait implemented.
struct Handler;

/*
    Callback traits that can be used to read or inject/modify data
    into upstream or downstream data.
*/
impl HandlerCallbacks for Handler {

    // DownStream non blocking callback
    fn ds_nb_callback(&self, _in_data: Vec<u8>) {
        println!("[+] Data before complete rewrite:\n{:#04X?}", _in_data);
    }

    // DownStream blocking callback
    fn ds_b_callback(&self, _in_data: Vec<u8>) {
        _in_data.reverse();
        println!("[+] Data rewritten to:\n{:#04X?}", _in_data);
        CallbackRet::Relay(_in_data)
    }

    // UpStream non blocking callback
    fn us_nb_callback(&self, _in_data: Vec<u8>) {
        println!("[+] Data before complete rewrite:\n{:#04X?}", _in_data);
    }

    // UpStream blocking callback
    fn us_b_callback(&self, _in_data: Vec<u8>) {
        _in_data.reverse();
        println!("[+] Data rewritten to:\n{:#04X?}", _in_data);
        CallbackRet::Relay(_in_data)
    }
}

fn main() {

    // Create new SSLRelay object
    let mut relay = sslrelay::SSLRelay::new(Handler);

    // Load Configuration
    relay.load_config(ConfigType::Default);

    // Start listening
    relay.start();
}