use httparse::{self, Header};
use chunked_transfer::Decoder;
use flate2::read::GzDecoder;
use std::thread;
use std::time::Duration;
use std::io::Read;

pub struct RelayedResponse<'a> {
    http_version: Option<u8>,
    http_code: Option<u16>,
    http_reason: Option<String>,
    http_headers: Option<Vec<Header<'a>>>,
    http_body: Option<String>,
}

pub struct RelayRequest<'a> {
    http_method: Option<String>,
    http_path: Option<String>,
    http_version: Option<u8>,
    http_headers: Option<Vec<Header<'a>>>,
    http_body: Option<String>,
}

/* Unused HTTP helper functions
pub fn get_req_headers<'a>(data: &'a Vec<u8>) -> Option<Vec<Header<'a>>> {

    let mut headers = [httparse::EMPTY_HEADER; 128];
    let mut request = httparse::Request::new(&mut headers);

    let req = request.parse(data).unwrap();

    if req.is_complete() {
        return Some(headers.to_vec());
    }
    None
}

pub fn get_res_headers(data: &Vec<u8>) -> Option<Vec<Header>> {

    let mut headers = [httparse::EMPTY_HEADER; 128];
    let mut response = httparse::Response::new(&mut headers);

    let res = response.parse(data).unwrap();

    if res.is_complete() {
        return Some(headers.to_vec());
    }
    None
}

pub fn get_host(data: &Vec<u8>) -> Option<String> {

    return Some("137.220.37.67".to_string());
    let mut headers = [httparse::EMPTY_HEADER; 128];

    let mut request = httparse::Request::new(&mut headers);

    let req = request.parse(data).unwrap();

    if req.is_complete() {
        for header in headers.iter() {
            if header.name == "Host" || header.name == "host" {
                //println!("Host -> {}", String::from_utf8(header.value.to_vec()).unwrap());
                return Some(String::from_utf8(header.value.to_vec()).unwrap());
            }
        }
        None
    } else if req.is_partial() {
        for header in headers.iter() {
            if header.name == "Host" || header.name == "host" {
                //println!("Host -> {}", String::from_utf8(header.value.to_vec()).unwrap());
                return Some(String::from_utf8(header.value.to_vec()).unwrap());
            }
        }
        None
    } else {
        None
    }
}

pub fn get_cookie(data: &Vec<u8>) -> Option<String> {

    let mut headers = [httparse::EMPTY_HEADER; 128];

    let mut request = httparse::Request::new(&mut headers);

    let req = request.parse(data).unwrap();

    if req.is_complete() {
        for header in headers.iter() {
            if header.name == "Cookie" || header.name == "cookie" {
                //println!("Host -> {}", String::from_utf8(header.value.to_vec()).unwrap());
                return Some(String::from_utf8(header.value.to_vec()).unwrap());
            }
        }
        None
    } else if req.is_partial() {
        for header in headers.iter() {
            if header.name == "Cookie" || header.name == "cookie" {
                //println!("Host -> {}", String::from_utf8(header.value.to_vec()).unwrap());
                return Some(String::from_utf8(header.value.to_vec()).unwrap());
            }
        }
        None
    } else {
        None
    }
}
*/
pub fn http_req_verbose(data: &Vec<u8>, mode: u8) {

    let req_info = get_request(&data);
    let req_header_list = req_info.http_headers.unwrap();
    let mut req_header_string = String::new();

    for header in req_header_list {
        if header.value.to_vec().len() == 0 {continue;}
        req_header_string.push_str(format!("[-->]   {}: {}\n", header.name, String::from_utf8(header.value.to_vec()).unwrap()).as_str());
    }
    if mode == 1 {
        println!("================================\n[-->] HTTP Version: {}\n[-->] HTTP Method: {} {}\n[-->] HTTP Headers:\n{}\n[-->] HTTP Body:\n{}\n"
            ,req_info.http_version.unwrap()
            ,req_info.http_method.unwrap()
            ,req_info.http_path.unwrap()
            ,req_header_string
            ,req_info.http_body.unwrap()
        );
    } else if mode == 2 {
        println!("[Req] {} {}", req_info.http_method.unwrap(), req_info.http_path.unwrap());
    }
}

pub fn http_res_verbose(response_data: &Vec<u8>, mode: u8) {

    let res_info = get_response(&response_data);
    let header_list = res_info.http_headers.unwrap();
    let mut header_string = String::new();

    for header in header_list {
        if header.value.to_vec().len() == 0 {continue;}
        header_string.push_str(format!("[<--]   {}: {}\n", header.name, String::from_utf8(header.value.to_vec()).unwrap()).as_str());
    }
    if mode == 1 {
        println!("\n[<--] HTTP Version: {}\n[<--] HTTP Code: {} {}\n[<--] HTTP Headers:\n{}\n[<--] HTTP Body:\n{}\n================================\n"
            ,res_info.http_version.unwrap()
            ,res_info.http_code.unwrap()
            ,res_info.http_reason.unwrap()
            ,header_string
            ,res_info.http_body.unwrap()
        );
    } else if mode == 2 {
        println!("[Res] {} {}", res_info.http_code.unwrap(), res_info.http_reason.unwrap());
    }
}

pub fn get_response<'a>(data: &'a Vec<u8>) -> RelayedResponse<'a> {

    let mut headers = [httparse::EMPTY_HEADER; 128];
    let mut res = httparse::Response::new(&mut headers);

    let res_chk = res.parse(data);

    loop {

        if !res_chk.unwrap().is_complete() {
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        let reason = match res.reason {
            Some(r) => Some(r.to_string()),
            None => None,
        };

        let header_vec = res.headers.to_vec();

        let body = get_http_body(&data, header_vec).unwrap_or(String::from("[-] Failed to get http response body!").into_bytes());
        let string_body = String::from_utf8(body).unwrap();

        return RelayedResponse {
            http_version: res.version,
            http_code: res.code,
            http_reason: reason,
            http_headers: Some(headers.to_vec()),
            http_body: Some(string_body),
        };
    }
}

pub fn get_request<'a>(data: &'a Vec<u8>) -> RelayRequest<'a> {

    let mut headers = [httparse::EMPTY_HEADER; 128];
    let mut req = httparse::Request::new(&mut headers);

    let req_chk = req.parse(data);

    loop {

        if !req_chk.unwrap().is_complete() {
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        let method = match req.method {
            Some(r) => Some(r.to_string()),
            None => None,
        };

        let path = match req.path {
            Some(p) => Some(p.to_string()),
            None => None,
        };

        let header_vec = req.headers.to_vec();

        let body = get_http_body(&data, header_vec).unwrap_or(String::from("[-] Failed to get http request body!").into_bytes());
        let string_body = String::from_utf8(body).unwrap();

        return RelayRequest {
            http_method: method,
            http_path: path,
            http_version: req.version,
            http_headers: Some(headers.to_vec()),
            http_body: Some(string_body),
        };
    }
}

pub fn get_http_body(data: &Vec<u8>, headers: Vec<Header>) -> Option<Vec<u8>> {

    let mut cl: u64 = 0;
    let encoding_check = headers.clone();
    for header in headers {

        if header.name == "Content-Length" || header.name == "content-length" {
            let length = match String::from_utf8(header.value.to_vec()) {
                Ok(s) => s,
                Err(e) => {
                    println!("[!] ERROR: {}",e);
                    return None;
                },
            };

            cl += length.parse::<u64>().unwrap();
            let mut r_body = data.to_vec();
            r_body.reverse();
            r_body.truncate(cl as usize);
            r_body.reverse();

            return Some(r_body);

        } else if header.name == "Transfer-Encoding" || header.name == "transfer-encoding" {

            let te_string = String::from_utf8(header.value.to_vec()).unwrap();

            if te_string == "chunked" {

                let mut new_vec = data.to_vec();
                let new_vec_iter = new_vec.to_vec();
                let mut new_vec_iter = new_vec_iter.iter();
                let mut i = 0;

                loop {
                    if let Some(&0x0d) = new_vec_iter.next() {
                        i += 1;
                        if let Some(&0x0a) = new_vec_iter.next() {
                            i += 1;
                            if let Some(&0x0d) = new_vec_iter.next() {
                                i += 1;
                                if let Some(&0x0a) = new_vec_iter.next() {
                                    i += 1;
                                    break;
                                } else {i += 1;}
                            } else {i += 1;}
                        } else {i += 1;}
                    } else {i += 1;}
                }

                new_vec.reverse();
                new_vec.truncate(data.len() - i);
                new_vec.reverse();

                let mut decoder = Decoder::new(new_vec.as_slice());
                let mut blob = Vec::new();
                let _ = decoder.read_to_end(&mut blob);

                let mut g_encoded = false;
                for header in encoding_check {
                    if header.name == "Content-Encoding" || header.name == "content-encoding" {
                        let s = String::from_utf8(header.value.to_vec()).unwrap();
                        if s == "gzip" {
                            g_encoded = true;
                        }
                    }
                }
                if g_encoded {
                    let mut gzd = GzDecoder::new(&blob[..]);
                    let mut unzipped = Vec::new();
                    match gzd.read_to_end(&mut unzipped) {
                        Ok(_) => {},
                        Err(_) => return None,
                    }
    
                    return Some(unzipped);
                } else {
                    return Some(blob);
                }
            }
        }
    }
    return Some(String::from("None").into_bytes());
}