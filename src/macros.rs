use colored::*;

#[macro_export]
macro_rules! warning {
    ($($arg:tt)*) => {
        println!("{}: {}", "Warning".to_string().red().bold(), format!($($arg)*));
    };
}
