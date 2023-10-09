pub struct ClientHello {
    pub sni_host: String,
}

#[derive(Debug)]
pub struct SNIError {
    msg: String,
}

impl std::error::Error for SNIError {}

impl std::fmt::Display for SNIError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

// Pre-check if the header looks alright (e.g. clienthello is complete)
pub fn pre_check(data: &[u8]) -> bool {
    data.len() > 5
        && data[0] == 0x16
        && data[1] == 0x03
        && (data[2] > 0x00 && data[2] < 0x05)
        && (data[3] as usize * 256 + data[4] as usize) == data.len() - 5
}

// Skip x bytes in input and return the skipped bytes
fn skip(data: &[u8], count: usize) -> &[u8] {
    &data[count..]
}

// Read `number_of_bytes` bytes and convert read number as usize, then skip them 
// in the byte array. Return the skipped bytes 
fn read_length_and_skip(data: &[u8], number_of_bytes: usize) -> &[u8] {
    let mut count = 1;
    let mut size = data[0] as usize;
    while count < number_of_bytes {
        size = size * 256 + data[count] as usize;
        count += 1;
    }
    let to_skip = size + number_of_bytes;
    skip(data, to_skip)
}

// Read TLS extension
fn read_extension(client_hello: &[u8]) -> (&[u8], usize, &[u8]) {
    let byte0 = client_hello[0];
    let byte1 = client_hello[1];
    let byte2 = client_hello[2];
    let byte3 = client_hello[3];
    let extension_type = (byte0 as usize) * 256 + (byte1 as usize);
    let length = (byte2 as usize) * 256 + (byte3 as usize);
    let data = &client_hello[4..4 + length];
    let remaining = &client_hello[4 + length..];
    (remaining, extension_type, data)
}

// Convert bytes to usize
fn to_int(data: &[u8]) -> usize {
    let mut count = 1;
    let mut size = data[0] as usize;
    while count < data.len() {
        size = size * 256 + data[count] as usize;
        count += 1;
    }
    size
}

// Parse the client hello bytes and return ClientHello object. Currently only SNIHost is returned.
// It is possible to add other info as well
pub fn parse(client_hello: &[u8]) -> Result<ClientHello, SNIError> {
    let len = client_hello.len();
    if !pre_check(client_hello) {
        return Err(SNIError {
            msg: format!("tls header pre check failed ({len} bytes)"),
        });
    }
    if client_hello[0] != 0x16 {
        return Err(SNIError {
            msg: String::from("invalid initial byte. Expect 0x16"),
        });
    }
    if client_hello[1] != 0x03 {
        return Err(SNIError {
            msg: String::from("expect version byte 0x03"),
        });
    }
    if client_hello[2] < 0x01 || client_hello[3] > 0x04 {
        return Err(SNIError {
            msg: String::from("only support TLS 1.0 ~ 1.3 (outer)"),
        });
    }

    let data_len = to_int(&client_hello[3..5]);
    if client_hello.len() < data_len + 5 {
        return Err(SNIError {
            msg: String::from("data length mismatch(outer)"),
        });
    }
    let inner_data_len = to_int(&client_hello[7..9]);
    if client_hello.len() != inner_data_len + 9 {
        return Err(SNIError {
            msg: String::from("data length mismatch(inner)"),
        });
    }

    let inner_version_bytes = &client_hello[9..11];
    if inner_version_bytes[0] != 0x03 {
        return Err(SNIError {
            msg: String::from("expect version byte 0x03 (inner)"),
        });
    }
    if inner_version_bytes[1] < 0x01 || inner_version_bytes[1] > 0x04 {
        return Err(SNIError {
            msg: String::from("only support TLS 1.0 ~ 1.3 (inner)"),
        });
    }

    let mut client_hello = skip(client_hello, 43);
    client_hello = read_length_and_skip(client_hello, 1); // read the 31 random bytes
    client_hello = read_length_and_skip(client_hello, 2); // read and skip cipher suites 00 62
    client_hello = read_length_and_skip(client_hello, 1); // skip the compression extension
    client_hello = skip(client_hello, 2); // skip remaining size identifier

    let mut sni_data: Option<&[u8]> = None;
    while !client_hello.is_empty() {
        let (new_client_hello, extension_type, extension_data) = read_extension(client_hello);
        client_hello = new_client_hello;
        if extension_type == 0 {
            sni_data = Some(extension_data);
        }
    }

    if let Some(data) = sni_data {
        let byte0 = data[0] as usize;
        let byte1 = data[1] as usize;
        let size = byte0 * 256 + byte1;
        if data.len() == size + 2 {
            let byte2 = data[2] as usize;
            let byte3 = data[3] as usize;
            let byte4 = data[4] as usize;
            if byte2 == 0 {
                let strlen = byte3 * 256 + byte4;
                if strlen == size - 3 {
                    let host = String::from_utf8_lossy(&data[5..]).into_owned();
                    return Ok(ClientHello { sni_host: host });
                }
            }
        }
    }

    Err(SNIError {
        msg: String::from("extension 0x00 0x00 not found"),
    })
}
