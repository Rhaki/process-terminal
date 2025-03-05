use process_terminal::{
    add_process, tprintln, utils::create_printing_process, KeyCode, MessageSettings,
    ProcessSettings, ScrollSettings,
};

fn main() {
    tprintln!("Starting...");

    std::thread::sleep(std::time::Duration::from_secs(2));

    let process_foo = create_printing_process(["hello", "world", "foo", "bar"], 0.1, 30);

    // Add the process to the terminal.
    // The first time `add_process` or `tprintln!` is called, the terminal is automatically initialized.
    add_process(
        "Foo",
        process_foo,
        ProcessSettings::new_with_scroll(
            // Show only the output messages.
            MessageSettings::Output,
            // Enable scrolling with the Left and Right keys.
            // Up and Down keys are reserved for `Main` output.
            ScrollSettings::enable(KeyCode::Left, KeyCode::Right),
        ),
    )
    .unwrap();

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
