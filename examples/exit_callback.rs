use process_terminal::{tprintln, with_exit_callback};

fn main() {
    with_exit_callback(|| {
        println!("Exit callback called!");
    });

    tprintln!("Press Ctrl+C to exit the program.");

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
