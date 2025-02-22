use std::process::{Child, Command, Stdio};

/// Create a process that prints messages and sleeps.
pub fn create_printing_process<'a, const N: usize>(
    messages: [&str; N],
    sleep: f64,
    last: u64,
) -> Child {
    let mut args = format!("sleep {sleep}");

    for _ in 0..(last as f64 / sleep / messages.len() as f64) as usize {
        for message in messages {
            args.push_str(&format!(" && echo {message} && sleep {sleep}"));
        }
    }

    Command::new("sh")
        .arg("-c")
        .arg(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}
