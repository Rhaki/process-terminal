use {
    crate::{ProcessSettings, TERMINAL},
    anyhow::Result,
    std::process::Child,
};

#[macro_export]
/// Print a message in the Main section of the teminal.
macro_rules! tprintln {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            process_terminal::TERMINAL.add_message(format!($($arg)*));
        }
    };
}

/// Add a process to the terminal.
pub fn add_process(name: &str, child: Child, settings: ProcessSettings) -> Result<()> {
    TERMINAL.add_process(name, child, settings)
}

/// Blocking function that block the current thread, searching for a substring in a specific process output, returning the whole output message.
pub fn block_search_message<S, P>(process: P, submsg: S) -> Result<String>
where
    S: ToString,
    P: ToString,
{
    TERMINAL.block_search_message(process, submsg)
}

pub fn end_terminal() {
    TERMINAL.kill();
}

pub fn with_exit_callback<F: Fn() + Send + Sync + 'static>(closure: F) {
    TERMINAL.with_exit_callback(closure);
}
