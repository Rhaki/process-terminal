use {
    crate::{
        MessageSettings, ProcessSettings, ScrollSettings,
        keyboard_actions::{
            Action, ActionType, BaseStatus, DetachBaseStatus, KeyBoardActions, KeyCodeExt,
            ScrollStatus,
        },
        let_clone,
        shared::Shared,
        spawn_thread,
    },
    anyhow::{Result, anyhow},
    crossterm::event::KeyModifiers,
    ratatui::{
        Frame,
        layout::{Constraint, Direction, Layout, Rect},
        style::Stylize,
        text::Line,
        widgets::{Block, Borders, List, ListState},
    },
    std::{
        cmp::min,
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
    inputs: Shared<KeyBoardActions>,
}

impl Terminal {
    pub fn new() -> Terminal {
        let_clone!(
            Default::default(),
            main_messages | _main_messages: SharedMessages,
            processes     | _processes:     SharedProcesses
        );

        let (inputs, scroll_status) = KeyBoardActions::new();

        let_clone!(
            Shared::new(inputs),
            inputs | _inputs: Shared<KeyBoardActions>
        );

        #[cfg(test)]
        let not_in_test = false;
        #[cfg(not(test))]
        let not_in_test = true;

        if std::env::args().any(|arg| arg.starts_with("--exact")) || not_in_test {
            spawn_thread!(thread_draw(_main_messages, scroll_status, _processes));
        }

        spawn_thread!(thread_input(_inputs));

        Terminal {
            processes,
            main_messages,
            inputs,
        }
    }

    pub fn add_process(
        &self,
        name: &str,
        mut child: Child,
        settings: ProcessSettings,
    ) -> Result<()> {
        let process = Process::new(name.to_string(), settings);

        let pre_count = self.processes.write_with(|mut processes| {
            let pre_count = processes.iter().fold(0, |buff, process| {
                let count = match &process.settings.messages {
                    MessageSettings::Output | MessageSettings::Error => 1,
                    MessageSettings::All => 2,
                    MessageSettings::None => 0,
                };

                buff + count
            });

            processes.push(process.clone());
            pre_count
        });

        let focus_indexes =
            match &process.settings.messages {
                MessageSettings::Output => {
                    let stdout = child.stdout.take().ok_or_else(|| {
                        anyhow::anyhow!("Failed to get stdout on process: {name}")
                    })?;

                    spawn_thread!(thread_output(
                        stdout,
                        process.out_messages,
                        process.search_message
                    ));

                    vec![pre_count + 1]
                }
                MessageSettings::Error => {
                    let stderr = child.stderr.take().ok_or_else(|| {
                        anyhow::anyhow!("Failed to get stderr on process: {name}")
                    })?;

                    let main_messages = self.main_messages.clone();

                    spawn_thread!(thread_error(
                        child,
                        stderr,
                        process.err_messages,
                        main_messages
                    ));

                    vec![pre_count + 1]
                }
                MessageSettings::All => {
                    let stdout = child.stdout.take().ok_or_else(|| {
                        anyhow::anyhow!("Failed to get stdout on process: {name}")
                    })?;

                    let stderr = child.stderr.take().ok_or_else(|| {
                        anyhow::anyhow!("Failed to get stderr on process: {name}")
                    })?;

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

                    vec![pre_count + 1, pre_count + 2]
                }
                MessageSettings::None => vec![],
            };

        if let ScrollSettings::Enable {
            up_right,
            down_left,
        } = process.settings.scroll
        {
            self.inputs.write_with(|mut inputs| {
                inputs.push(Action::new(
                    up_right.into_event_no_modifier(),
                    ActionType::ScrollUp(process.scroll_status.clone()),
                ));
                inputs.push(Action::new(
                    down_left.into_event_no_modifier(),
                    ActionType::ScrollDown(process.scroll_status.clone()),
                ));
                inputs.push(Action::new(
                    up_right.into_event(KeyModifiers::SHIFT),
                    ActionType::ScrollRight(process.scroll_status.clone()),
                ));
                inputs.push(Action::new(
                    down_left.into_event(KeyModifiers::SHIFT),
                    ActionType::ScrollLeft(process.scroll_status.clone()),
                ));
            });
        }

        if !focus_indexes.is_empty() {
            self.inputs
                .write_with(|mut inputs| inputs.push_focus(&focus_indexes))?;
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

            sleep_thread();
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

fn thread_input(inputs: Shared<KeyBoardActions>) {
    loop {
        let event = crossterm::event::read().expect("Failed to read event.");

        inputs.read_with(|inputs| {
            inputs.apply_event(event);
        });
    }
}

fn thread_draw(main_messages: SharedMessages, main_scroll: BaseStatus, processes: SharedProcesses) {
    let mut terminal = ratatui::init();

    let data = DrawCache::new(main_messages, main_scroll, processes);

    let mut cache = DrawCache::default_detach();

    loop {
        let read = data.detach();

        if read == cache {
            sleep_thread();
            continue;
        } else {
            cache = read.clone();
        }

        let DrawCache {
            main_messages,
            main_scroll,
            processes,
        } = read;

        terminal
            .draw(|frame| {
                if let Some(focus) = main_scroll.focus {
                    if focus == 0 {
                        render_frame(
                            frame,
                            frame.area(),
                            "",
                            BlockType::Main,
                            BlocFocus::Exit,
                            main_messages,
                            &main_scroll.main_scroll,
                        );
                    } else {
                        let mut index = 0;
                        for i in processes {
                            if let Some((t, messages)) = match i.settings.messages {
                                MessageSettings::Output => {
                                    index += 1;

                                    if index == focus {
                                        Some((BlockType::Out, i.out_messages))
                                    } else {
                                        None
                                    }
                                }
                                MessageSettings::Error => {
                                    index += 1;

                                    if index == focus {
                                        Some((BlockType::Err, i.err_messages))
                                    } else {
                                        None
                                    }
                                }
                                MessageSettings::All => {
                                    index += 1;

                                    if index == focus {
                                        Some((BlockType::Out, i.out_messages))
                                    } else if index + 1 == focus {
                                        Some((BlockType::Err, i.err_messages))
                                    } else {
                                        index += 1;
                                        None
                                    }
                                }
                                MessageSettings::None => None,
                            } {
                                render_frame(
                                    frame,
                                    frame.area(),
                                    i.name,
                                    t,
                                    BlocFocus::Exit,
                                    messages,
                                    &i.scroll_status,
                                );
                                break;
                            }
                        }
                    }
                } else {
                    let main_chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                        .split(frame.area());

                    render_frame(
                        frame,
                        main_chunks[0],
                        "",
                        BlockType::Main,
                        BlocFocus::Enter(0),
                        main_messages,
                        &main_scroll.main_scroll,
                    );

                    let processes_chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints(vec![
                            Constraint::Ratio(1, processes.len() as u32);
                            processes.len()
                        ])
                        .split(main_chunks[1]);

                    let mut focus = 0;

                    for (index, process) in processes.into_iter().enumerate() {
                        match process.settings.messages {
                            MessageSettings::Output => {
                                focus += 1;

                                render_frame(
                                    frame,
                                    processes_chunks[index],
                                    process.name,
                                    BlockType::Out,
                                    BlocFocus::Enter(focus),
                                    process.out_messages,
                                    &process.scroll_status,
                                );
                            }
                            MessageSettings::Error => {
                                focus += 1;

                                render_frame(
                                    frame,
                                    processes_chunks[index],
                                    process.name,
                                    BlockType::Err,
                                    BlocFocus::Enter(focus),
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

                                focus += 1;
                                render_frame(
                                    frame,
                                    process_chunks[0],
                                    &process.name,
                                    BlockType::Out,
                                    BlocFocus::Enter(focus),
                                    process.out_messages,
                                    &process.scroll_status,
                                );

                                focus += 1;
                                render_frame(
                                    frame,
                                    process_chunks[1],
                                    process.name,
                                    BlockType::Err,
                                    BlocFocus::Enter(focus),
                                    process.err_messages,
                                    &process.scroll_status,
                                );
                            }
                            MessageSettings::None => {}
                        }
                    }
                }
            })
            .unwrap();

        sleep_thread();
    }
}

fn render_frame<N>(
    frame: &mut Frame,
    chunk: Rect,
    name: N,
    ty: BlockType,
    focus: BlocFocus,
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

    if scroll.y > 0 {
        state.scroll_up_by(min(scroll.y as usize, messages.len()) as u16);
    }

    let sub_title = match ty {
        BlockType::Main => Line::from("Main").cyan().bold(),
        BlockType::Out => Line::from("Out").light_green().bold(),
        BlockType::Err => Line::from("Err").light_red().bold(),
    };

    let focus = match focus {
        BlocFocus::Enter(key) => format!("full screen: '{key}'"),
        BlocFocus::Exit => format!("press 'Esc' to exit full screen"),
    };

    let block = Block::default()
        .title(Line::from(name.to_string()).gray().bold().centered())
        .title(sub_title.centered())
        .title(Line::from(focus).right_aligned().italic().dark_gray())
        .borders(Borders::ALL);

    let list = List::new(messages).block(block);

    frame.render_stateful_widget(list, chunk, &mut state);
}

fn sleep_thread() {
    sleep(Duration::from_millis(50));
}

pub enum BlockType {
    Main,
    Out,
    Err,
}

pub enum BlocFocus {
    Enter(usize),
    Exit,
}

type DetachProcess = Process<Vec<String>, Vec<String>, ScrollStatus, ()>;

#[derive(Clone, PartialEq)]
struct Process<
    O = SharedMessages,
    E = SharedMessages,
    S = Shared<ScrollStatus>,
    SM = Shared<Option<SearchMessage>>,
> {
    pub name: String,
    pub out_messages: O,
    pub err_messages: E,
    pub settings: ProcessSettings,
    pub scroll_status: S,
    pub search_message: SM,
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

    pub fn detach(&self) -> DetachProcess {
        Process {
            name: self.name.clone(),
            settings: self.settings.clone(),
            out_messages: self.out_messages.read_access().clone(),
            err_messages: self.err_messages.read_access().clone(),
            scroll_status: self.scroll_status.read_access().clone(),
            search_message: (),
        }
    }
}

#[derive(PartialEq)]
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

type DrawCacheDetach = DrawCache<Vec<String>, DetachBaseStatus, Vec<DetachProcess>>;

#[derive(Clone, PartialEq)]
struct DrawCache<MM = SharedMessages, MS = BaseStatus, P = SharedProcesses> {
    pub main_messages: MM,
    pub main_scroll: MS,
    pub processes: P,
}

impl DrawCache {
    pub fn new(
        main_messages: SharedMessages,
        main_scroll: BaseStatus,
        processes: SharedProcesses,
    ) -> Self {
        Self {
            main_messages,
            main_scroll,
            processes,
        }
    }

    pub fn default_detach() -> DrawCacheDetach {
        DrawCache {
            main_messages: Default::default(),
            main_scroll: Default::default(),
            processes: Default::default(),
        }
    }

    pub fn detach(&self) -> DrawCacheDetach {
        DrawCache {
            main_messages: self.main_messages.read_access().clone(),
            main_scroll: self.main_scroll.detach(),
            processes: self
                .processes
                .read_access()
                .iter()
                .map(Process::detach)
                .collect::<Vec<_>>(),
        }
    }
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
                "Foo",
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
                "Bar",
                process2,
                ProcessSettings::new(crate::MessageSettings::All),
            )
            .unwrap();

        sleep(Duration::from_secs(2));
        terminal.add_message("searching_message");
        let msg = terminal.block_search_message("Foo", "llo").unwrap();
        terminal.add_message(msg);

        sleep(Duration::from_secs(2));
        terminal.add_message("searching_message");
        let msg = terminal.block_search_message("Bar", "ar").unwrap();
        terminal.add_message(msg);

        sleep(Duration::from_secs(50));
    }
}
