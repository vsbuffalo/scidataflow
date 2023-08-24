use anyhow::Result;

pub fn check_error<T>(result: Result<T>, pattern: &str) {
    match result {
        Ok(_) => assert!(false, "Expected an error, but got Ok"),
        Err(err) => {
            assert!(err.to_string().contains(pattern),
            "Unexpected error {:?} containing pattern \"{:?}\" ", 
            err, pattern);
        }
    }
}
