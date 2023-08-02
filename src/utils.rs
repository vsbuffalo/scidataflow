use std::path::{Path,PathBuf};
use std::fs::{File};
use std::io::Read;
use md5::{Digest, Context};

const BUFFER_SIZE: usize = 4096;


pub fn load_file(path: &PathBuf) -> String {
    let mut file = File::open(&path).expect("unable to open file");
    let mut contents = String::new();
    file.read_to_string(&mut contents).expect("unable to read file");
    contents
}

pub fn compute_md5(file_path: &Path) -> Option<String> {
    let mut file = match File::open(file_path) {
        Ok(file) => file,
        Err(_) => return None,
    };

    let mut buffer = [0; BUFFER_SIZE];
    let mut md5 = Context::new();

    loop {
        let bytes_read = match file.read(&mut buffer) {
            Ok(0) => break, // EOF
            Ok(bytes_read) => bytes_read,
            Err(_) => return None,
        };

        md5.consume(&buffer[..bytes_read]);
    }
    let result = md5.compute();
    Some(format!("{:x}", result))
}
