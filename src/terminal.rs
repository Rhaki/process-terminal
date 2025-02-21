use {
    crate::{
        MessageSettings, ProcessSettings, ScrollSettings, let_clone, shared::Shared, spawn_thread,
    },
    anyhow::{Result, anyhow},
    crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers},
    ratatui::{
        Frame,
        layout::{Constraint, Direction, Layout, Rect},
        widgets::{Block, Borders, List, ListState},
    },
    std::{
        io::{BufRead, BufReader},
        process::{Child, ChildStderr, ChildStdout},
        thread::sleep,
        time::Duration,
    },
};

type SharedMessages = Shared<Vec<String>>;
type SharedProcesses = Shared<Vec<Process>>;

pub struct Terminal {
    processes: SharedProcesses,
    main_messages: SharedMessages,
}

impl Terminal {
    pub fn new() -> Terminal {
        let_clone!(
            Default::default(),
            main_messages | _main_messages: SharedMessages,
            processes     | _processes:     SharedProcesses,
            _scroll_d     | _scroll_s:      Shared<ScrollStatus>
        );

        spawn_thread!(thread_draw(_main_messages, _scroll_d, _processes));

        spawn_thread!(thread_scroll(_scroll_s, KeyCode::Up, KeyCode::Down,));

        Terminal {
            processes,
            main_messages,
        }
    }

    pub fn add_process(
        &self,
        name: &str,
        mut child: Child,
        settings: ProcessSettings,
    ) -> Result<()> {
        let process = Process::new(name.to_string(), settings);

        self.processes.write_with(|mut processes| {
            processes.push(process.clone());
        });

        match &process.settings.messages {
            MessageSettings::Output => {
                let stdout = child
                    .stdout
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get stdout on process: {name}"))?;

                spawn_thread!(thread_output(
                    stdout,
                    process.out_messages,
                    process.search_message
                ));
            }
            MessageSettings::Error => {
                let stderr = child
                    .stderr
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get stderr on process: {name}"))?;

                let main_messages = self.main_messages.clone();

                spawn_thread!(thread_error(
                    child,
                    stderr,
                    process.err_messages,
                    main_messages
                ));
            }
            MessageSettings::All => {
                let stdout = child
                    .stdout
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get stdout on process: {name}"))?;

                let stderr = child
                    .stderr
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("Failed to get stderr on process: {name}"))?;

                let main_messages = self.main_messages.clone();

                spawn_thread!(thread_output(
                    stdout,
                    process.out_messages,
                    process.search_message
                ));
                spawn_thread!(thread_error(
                    child,
                    stderr,
                    process.err_messages,
                    main_messages
                ));
            }
        };

        if let ScrollSettings::Enable {
            up_right,
            down_left,
        } = process.settings.scroll
        {
            let scroll = process.scroll_status.clone();

            spawn_thread!(thread_scroll(scroll, up_right, down_left));
        }

        Ok(())
    }

    pub fn add_message<M>(&self, message: M)
    where
        M: ToString,
    {
        self.main_messages.write_with(|mut messages| {
            messages.push(message.to_string());
        });
    }

    pub fn block_search_message<S, P>(&self, process: P, submsg: S) -> Result<String>
    where
        S: ToString,
        P: ToString,
    {
        let process = process.to_string();

        let process = self
            .processes
            .read_access()
            .clone()
            .into_iter()
            .find(|p| p.name == process)
            .ok_or(anyhow!("Process not found."))?;

        process.search_message.write_with(|mut process| {
            *process = Some(SearchMessage::new(submsg.to_string()));
        });

        loop {
            let message = process.search_message.write_with(|mut search_message| {
                let message = search_message.as_ref().unwrap().message.clone();
                if message.is_some() {
                    *search_message = None;
                }
                message
            });

            if let Some(message) = message {
                return Ok(message);
            }

            thread_sleep();
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

fn thread_output(
    stdout: ChildStdout,
    messages: SharedMessages,
    search_message: Shared<Option<SearchMessage>>,
) {
    let regex = Regex::new();

    for line in BufReader::new(stdout).lines() {
        let line = regex.clear(line.expect("Failed to read line from stdout."));

        messages.write_with(|mut messages| {
            messages.push(line.clone());
        });

        search_message.write_with(|mut maybe_search_message| {
            if let Some(search_message) = maybe_search_message.as_mut() {
                if line.contains(&search_message.submsg) {
                    search_message.message = Some(line);
                }
            }
        });
    }
}

fn thread_error(
    mut child: Child,
    stderr: ChildStderr,
    messages: SharedMessages,
    main_messages: SharedMessages,
) {
    let regex = Regex::new();

    for line in BufReader::new(stderr).lines() {
        let line = regex.clear(line.expect("Failed to read line from stderr."));

        messages.write_with(|mut messages| {
            messages.push(line);
        });

        let exit_status = match child.try_wait() {
            Ok(status) => match status {
                Some(status) => format!("ok with status: {status}."),
                None => format!("ok with status: None."),
            },
            Err(err) => format!("fail with error: {err}."),
        };

        main_messages.write_with(|mut messages| {
            messages.push(format!("Process exited: {exit_status}"));
        });
    }
}

fn thread_draw(
    main_messages: SharedMessages,
    main_scroll: Shared<ScrollStatus>,
    processes: SharedProcesses,
) {
    let mut terminal = ratatui::init();

    loop {
        let main_messages = main_messages.read_access().clone();

        let main_scroll = main_scroll.read_access().clone();

        let processes = processes
            .read_access()
            .iter()
            .map(Process::detacth)
            .collect::<Vec<_>>();

        terminal
            .draw(|frame| {
                let main_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                    .split(frame.area());

                render_frame(
                    frame,
                    main_chunks[0],
                    "Main",
                    None,
                    main_messages,
                    &main_scroll,
                );

                let processes_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(vec![
                        Constraint::Ratio(1, processes.len() as u32);
                        processes.len()
                    ])
                    .split(main_chunks[1]);

                for (index, process) in processes.into_iter().enumerate() {
                    match process.settings.messages {
                        MessageSettings::Output => {
                            render_frame(
                                frame,
                                processes_chunks[index],
                                process.name,
                                Some("out"),
                                process.out_messages,
                                &process.scroll_status,
                            );
                        }
                        MessageSettings::Error => {
                            render_frame(
                                frame,
                                processes_chunks[index],
                                process.name,
                                Some("err"),
                                process.err_messages,
                                &process.scroll_status,
                            );
                        }
                        MessageSettings::All => {
                            let process_chunks = Layout::default()
                                .direction(Direction::Vertical)
                                .constraints([
                                    Constraint::Percentage(70),
                                    Constraint::Percentage(30),
                                ])
                                .split(processes_chunks[index]);

                            render_frame(
                                frame,
                                process_chunks[0],
                                &process.name,
                                Some("out"),
                                process.out_messages,
                                &process.scroll_status,
                            );
                            render_frame(
                                frame,
                                process_chunks[1],
                                process.name,
                                Some("err"),
                                process.err_messages,
                                &process.scroll_status,
                            );
                        }
                    }
                }
            })
            .unwrap();

        thread_sleep();
    }
}

fn render_frame<N>(
    frame: &mut Frame,
    chunk: Rect,
    name: N,
    ty: Option<&str>,
    messages: Vec<String>,
    scroll: &ScrollStatus,
) where
    N: ToString,
{
    let select_message = if messages.len() == 0 {
        None
    } else {
        Some(messages.len() - 1)
    };

    let mut state = ListState::default().with_selected(select_message);

    if let Some(y) = scroll.y {
        state.scroll_up_by(y);
    }

    let name = if let Some(ty) = ty {
        format!("{}-{ty}", name.to_string())
    } else {
        name.to_string()
    };

    let list = List::new(messages).block(Block::default().title(name).borders(Borders::ALL));

    frame.render_stateful_widget(list, chunk, &mut state);
}

fn thread_scroll(scroll: Shared<ScrollStatus>, up_right: KeyCode, down_left: KeyCode) {
    loop {
        if let Event::Key(event) = crossterm::event::read().expect("Failed to read event.") {
            // Up
            if event == KeyEvent::new(up_right, KeyModifiers::empty()) {
                scroll.write_with(|mut scroll| {
                    scroll.y = scroll.y.map(|y| y + 1).or(Some(1));
                });
            // Right
            } else if event == KeyEvent::new(up_right, KeyModifiers::SHIFT) {
                scroll.write_with(|mut scroll| {
                    scroll.x = scroll.x.map(|x| x + 1).or(Some(1));
                });
            }
            // Down
            else if event == KeyEvent::new(down_left, KeyModifiers::empty()) {
                scroll.write_with(|mut scroll| {
                    scroll.y = scroll.y.map(|y| y.saturating_sub(1)).or(Some(0));
                });
            }
            // Left
            else if event == KeyEvent::new(down_left, KeyModifiers::SHIFT) {
                scroll.write_with(|mut scroll| {
                    scroll.x = scroll.x.map(|x| x.saturating_sub(1)).or(Some(0));
                });
            } else if event == KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()) {
                // kill process
                ratatui::restore();
                std::process::exit(0);
            }
        }
    }
}

fn thread_sleep() {
    sleep(Duration::from_millis(50));
}

#[derive(Clone)]
struct Process<O = SharedMessages, E = SharedMessages, S = Shared<ScrollStatus>> {
    pub name: String,
    pub out_messages: O,
    pub err_messages: E,
    pub settings: ProcessSettings,
    pub scroll_status: S,
    pub search_message: Shared<Option<SearchMessage>>,
}

impl Process {
    pub fn new(name: String, settings: ProcessSettings) -> Process {
        Process {
            name,
            settings,
            out_messages: Default::default(),
            err_messages: Default::default(),
            scroll_status: Default::default(),
            search_message: Default::default(),
        }
    }

    pub fn detacth(&self) -> Process<Vec<String>, Vec<String>, ScrollStatus> {
        Process {
            name: self.name.clone(),
            settings: self.settings.clone(),
            out_messages: self.out_messages.read_access().clone(),
            err_messages: self.err_messages.read_access().clone(),
            scroll_status: self.scroll_status.read_access().clone(),
            search_message: self.search_message.clone(),
        }
    }
}

#[derive(Default, Clone)]
struct ScrollStatus {
    x: Option<u16>,
    y: Option<u16>,
}

struct Regex(regex::Regex);

impl Regex {
    pub fn new() -> Self {
        Self(regex::Regex::new(r"\x1b\[([\x30-\x3f]*[\x20-\x2f]*[\x40-\x7e])").unwrap())
    }

    pub fn clear(&self, line: String) -> String {
        self.0.replace_all(&line, "").to_string()
    }
}

struct SearchMessage {
    pub submsg: String,
    pub message: Option<String>,
}

impl SearchMessage {
    pub fn new(submsg: String) -> Self {
        Self {
            submsg,
            message: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::Terminal,
        crate::{ProcessSettings, ScrollSettings},
        crossterm::event::KeyCode,
        std::{
            process::{Child, Command, Stdio},
            thread::sleep,
            time::Duration,
        },
    };

    fn create_process<'a, const N: usize>(messages: [&str; N], sleep: f64, last: u64) -> Child {
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

    #[test]
    fn terminal() {
        let t = Terminal::new();

        t.add_message("hello");

        sleep(Duration::from_secs(2));

        t.add_message("world");

        sleep(Duration::from_secs(2));
    }

    #[test]
    fn process() {
        let terminal = Terminal::new();

        let process1 = create_process(["hello", "world", "foo", "bar"], 1.0, 30);

        terminal
            .add_process(
                "test-1",
                process1,
                ProcessSettings::new_with_scroll(
                    crate::MessageSettings::Output,
                    ScrollSettings::enable(KeyCode::Left, KeyCode::Right),
                ),
            )
            .unwrap();

        sleep(Duration::from_secs(2));

        let process2 = create_process(["hello", "world >&2", "foo", "bar"], 0.1, 8);

        terminal
            .add_process(
                "test-2",
                process2,
                ProcessSettings::new(crate::MessageSettings::All),
            )
            .unwrap();

        sleep(Duration::from_secs(2));
        terminal.add_message("searching_message");
        let msg = terminal.block_search_message("test-2", "llo").unwrap();
        terminal.add_message(msg);

        sleep(Duration::from_secs(2));
        terminal.add_message("searching_message");
        let msg = terminal.block_search_message("test-2", "ar").unwrap();
        terminal.add_message(msg);

        sleep(Duration::from_secs(50));
    }
}
