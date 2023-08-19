#[macro_export]
macro_rules! print_warn {
    ($($arg:tt)*) => {
        println!("{}: {}", "Warning".to_string().red().bold(), format!($($arg)*));
    };
}

#[macro_export]
macro_rules! print_info {
    ($($arg:tt)*) => {
        println!("{}: {}", "Info".to_string().green().bold(), format!($($arg)*));
    };
}
