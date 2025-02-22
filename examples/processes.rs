use {
    process_terminal::{
        KeyCode, MessageSettings, ProcessSettings, ScrollSettings, add_process,
        block_search_message, end_terminal, tprintln, utils::create_printing_process,
    },
    std::{thread::sleep, time::Duration},
};

fn main() {
    let process_foo = create_printing_process(["hello", "world", "foo", "bar"], 1.0, 30);

    add_process(
        "Foo",
        process_foo,
        ProcessSettings::new_with_scroll(
            MessageSettings::Output,
            ScrollSettings::enable(KeyCode::Left, KeyCode::Right),
        ),
    )
    .unwrap();

    sleep(Duration::from_secs(2));

    let process_bar = create_printing_process(
        ["hello", "Err: this is an error! >&2", "foo", "bar"],
        0.1,
        8,
    );

    add_process(
        "Bar",
        process_bar,
        ProcessSettings::new(MessageSettings::All),
    )
    .unwrap();

    sleep(Duration::from_secs(2));

    tprintln!("searching_message");
    let msg = block_search_message("Foo", "llo").unwrap();
    tprintln!("msg found: {}", msg);
    assert_eq!(msg, "hello");

    tprintln!("searching_message");
    let msg = block_search_message("Bar", "ar").unwrap();
    tprintln!("msg found: {}", msg);
    assert_eq!(msg, "bar");

    sleep(Duration::from_secs(20));

    end_terminal();
}
