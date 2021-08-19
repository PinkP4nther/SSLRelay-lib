use sslrelay::{self, ConfigType, HandlerCallbacks};


// Handler object
struct Handler;

/*
    Callback traits that can be used to read or inject data
    into data upstream or downstream.
*/
impl HandlerCallbacks for Handler {

    // Request non blocking callback
    fn ds_nb_callback(&self, _in_data: Vec<u8>) {
        println!("[+] Request Non Blocking CallBack!");
    }

    // Request blocking callback
    fn ds_b_callback(&self, _in_data: &mut Vec<u8>) {
        println!("[+] Request Blocking CallBack!");
    }

    // Response non blocking callback
    fn us_nb_callback(&self, _in_data: Vec<u8>) {
        println!("[+] Response Non Blocking CallBack!");
    }

    // Response blocking callback
    fn us_b_callback(&self, _in_data: &mut Vec<u8>) {
        println!("[+] Response Blocking CallBack!");
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