use {
    crate::{
        keyboard_actions::{
            Action, ActionScroll, ActionType, BaseStatus, DetachBaseStatus, KeyBoardActions,
            KeyCodeExt, ScrollStatus,
        },
        shared::Shared,
        MessageSettings, ProcessSettings, ScrollSettings,
    },
    anyhow::{anyhow, Result},
    crossterm::event::KeyModifiers,
    ratatui::{
        layout::{Constraint, Direction, Layout, Rect},
        style::{Style, Stylize},
        text::{Line, Text},
        widgets::{Block, Borders, List, ListState},
        Frame,
    },
    std::{
        io::{BufRead, BufReader},
        process::{Child, ChildStderr, ChildStdout},
        sync::LazyLock,
        thread::sleep,
        time::Duration,
    },
};

pub static TERMINAL: LazyLock<Terminal> = LazyLock::new(Terminal::new);

pub(crate) type SharedMessages = Shared<Vec<String>>;
type SharedProcesses = Shared<Vec<Process>>;
type DetachProcess = Process<Vec<String>, Vec<String>, ScrollStatus, ()>;
type DrawCacheDetach = DrawCache<Vec<String>, DetachBaseStatus, Vec<DetachProcess>>;
pub(crate) type ExitCallback = Option<Box<dyn Fn() + Send + Sync>>;

macro_rules! spawn_thread {
    ($callback:expr) => {
        std::thread::spawn(move || $callback);
    };
}

macro_rules! let_clone {
    ($init:expr, $( $name:ident | $($clone:ident)|* : $ty:ty),*) => {
        $(
            let $name: $ty = $init;
            $(
                let $clone = $name.clone();
            )*
        )*
    };
}

pub struct Terminal {
    processes: SharedProcesses,
    main_messages: SharedMessages,
    inputs: Shared<KeyBoardActions>,
    exit_callback: Shared<ExitCallback>,
}

impl Terminal {
    fn new() -> Terminal {
        let_clone!(
            Default::default(),
            main_messages | _main_messages: SharedMessages,
            processes     | _processes:     SharedProcesses
        );

        let (inputs, scroll_status, exit_callback) = KeyBoardActions::new(main_messages.clone());

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
            exit_callback,
        }
    }

    pub(crate) fn add_process(
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

                    let _out_messages = process.out_messages.clone();

                    spawn_thread!(thread_output(
                        stdout,
                        _out_messages,
                        process.search_message,
                        process.settings.clear_regex
                    ));

                    vec![pre_count + 1]
                }
                MessageSettings::Error => {
                    let stderr = child.stderr.take().ok_or_else(|| {
                        anyhow::anyhow!("Failed to get stderr on process: {name}")
                    })?;

                    let _err_messages = process.err_messages.clone();

                    spawn_thread!(thread_error(
                        stderr,
                        _err_messages,
                        process.settings.clear_regex
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

                    let _out_messages = process.out_messages.clone();
                    let _err_messages = process.err_messages.clone();

                    spawn_thread!(thread_output(
                        stdout,
                        _out_messages,
                        process.search_message,
                        process.settings.clear_regex
                    ));
                    spawn_thread!(thread_error(
                        stderr,
                        _err_messages,
                        process.settings.clear_regex
                    ));

                    vec![pre_count + 1, pre_count + 2]
                }
                MessageSettings::None => vec![],
            };

        let main_messages = self.main_messages.clone();
        let name = name.to_string();

        spawn_thread!(thread_exit(name, child, main_messages));

        if let ScrollSettings::Enable {
            up: up_right,
            down: down_left,
        } = process.settings.scroll
        {
            for (scroll_status, messages) in [
                (
                    process.scroll_status_out.clone(),
                    process.out_messages.clone(),
                ),
                (
                    process.scroll_status_err.clone(),
                    process.err_messages.clone(),
                ),
            ] {
                let action_scroll = ActionScroll {
                    status: scroll_status.clone(),
                    messages: messages.clone(),
                };

                self.inputs.write_with(|mut inputs| {
                    inputs.push(Action::new(
                        up_right.into_event_no_modifier(),
                        ActionType::ScrollUp(action_scroll.clone()),
                    ));
                    inputs.push(Action::new(
                        down_left.into_event_no_modifier(),
                        ActionType::ScrollDown(action_scroll.clone()),
                    ));
                    inputs.push(Action::new(
                        down_left.into_event(KeyModifiers::SHIFT),
                        ActionType::StopScrolling(process.scroll_status_out.clone()),
                    ));
                    inputs.push(Action::new(
                        down_left.into_event(KeyModifiers::SHIFT),
                        ActionType::StopScrolling(process.scroll_status_err.clone()),
                    ));
                });
            }
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

    pub(crate) fn block_search_message<S, P>(&self, process: P, submsg: S) -> Result<String>
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

    pub(crate) fn with_exit_callback<F: Fn() + Send + Sync + 'static>(&self, closure: F) {
        self.exit_callback.write_with(|mut terminal| {
            *terminal = Some(Box::new(closure));
        });
    }

    pub(crate) fn kill(&self) {
        ratatui::restore();
        if let Some(callback) = self.exit_callback.read_access().as_ref() {
            callback();
        }

        std::process::exit(0);
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
    clear_regex: bool,
) {
    let regex = if clear_regex {
        Some(Regex::new())
    } else {
        None
    };

    for line in BufReader::new(stdout).lines() {
        let line = line.expect("Failed to read line from stdout.");
        let line = if let Some(regex) = &regex {
            regex.clear(line)
        } else {
            line
        };

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

fn thread_error(stderr: ChildStderr, messages: SharedMessages, clear_regex: bool) {
    let regex: Option<Regex> = if clear_regex {
        Some(Regex::new())
    } else {
        None
    };

    for line in BufReader::new(stderr).lines() {
        let line = line.expect("Failed to read line from stderr.");
        let line = if let Some(regex) = &regex {
            regex.clear(line)
        } else {
            line
        };

        messages.write_with(|mut messages| {
            messages.push(line);
        });
    }
}

fn thread_exit(process_name: String, mut child: Child, main_messages: SharedMessages) {
    let exit_status = match child.wait() {
        Ok(status) => format!("ok: {status}."),

        Err(err) => format!("fail with error: {err}."),
    };

    main_messages.write_with(|mut messages| {
        messages.push(format!("Process '{process_name}' exited: {exit_status}"));
    });
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
                            BlockFocus::Exit,
                            main_messages,
                            &main_scroll.main_scroll,
                        );
                    } else {
                        let mut index = 0;
                        for i in processes {
                            if let Some((ty, messages, scroll)) = match i.settings.messages {
                                MessageSettings::Output => {
                                    index += 1;

                                    if index == focus {
                                        Some((BlockType::Out, i.out_messages, i.scroll_status_out))
                                    } else {
                                        None
                                    }
                                }
                                MessageSettings::Error => {
                                    index += 1;

                                    if index == focus {
                                        Some((BlockType::Err, i.err_messages, i.scroll_status_err))
                                    } else {
                                        None
                                    }
                                }
                                MessageSettings::All => {
                                    index += 1;

                                    if index == focus {
                                        Some((BlockType::Out, i.out_messages, i.scroll_status_out))
                                    } else if index + 1 == focus {
                                        Some((BlockType::Err, i.err_messages, i.scroll_status_err))
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
                                    ty,
                                    BlockFocus::Exit,
                                    messages,
                                    &scroll,
                                );
                                break;
                            }
                        }
                    }
                } else {
                    let main_chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints(if processes.is_empty() {
                            vec![Constraint::Percentage(100)]
                        } else {
                            vec![Constraint::Percentage(30), Constraint::Percentage(70)]
                        })
                        .split(frame.area());

                    render_frame(
                        frame,
                        main_chunks[0],
                        "",
                        BlockType::Main,
                        BlockFocus::Enter(0),
                        main_messages,
                        &main_scroll.main_scroll,
                    );

                    if processes.is_empty() {
                        return;
                    }

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
                                    BlockFocus::Enter(focus),
                                    process.out_messages,
                                    &process.scroll_status_out,
                                );
                            }
                            MessageSettings::Error => {
                                focus += 1;

                                render_frame(
                                    frame,
                                    processes_chunks[index],
                                    process.name,
                                    BlockType::Err,
                                    BlockFocus::Enter(focus),
                                    process.err_messages,
                                    &process.scroll_status_err,
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
                                    BlockFocus::Enter(focus),
                                    process.out_messages,
                                    &process.scroll_status_out,
                                );

                                focus += 1;
                                render_frame(
                                    frame,
                                    process_chunks[1],
                                    process.name,
                                    BlockType::Err,
                                    BlockFocus::Enter(focus),
                                    process.err_messages,
                                    &process.scroll_status_err,
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
    focus: BlockFocus,
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

    let sub_title = match ty {
        BlockType::Main => Line::from("Main").cyan().bold(),
        BlockType::Out => Line::from("Out").light_green().bold(),
        BlockType::Err => Line::from("Err").light_red().bold(),
    };

    let focus_txt = match focus {
        BlockFocus::Enter(index) => format!("full screen: '{index}'"),
        BlockFocus::Exit => format!("press 'Esc' to exit full screen"),
    };

    let mut block = Block::default()
        .title(Line::from(name.to_string()).gray().bold().centered())
        .title(sub_title.centered())
        .title(Line::from(focus_txt).right_aligned().italic().dark_gray())
        .borders(Borders::ALL);

    let is_scrolling = if let Some(y) = scroll.y {
        let offset = messages.len().saturating_sub(y as usize);

        state.scroll_up_by(offset as u16);

        block = block.title(
            Line::from(format!(
                "Scrolling: offset {offset} - press 'shift + scroll_down' key to stop scrolling."
            ))
            .bold()
            .left_aligned()
            .yellow(),
        );

        true
    } else {
        false
    };

    let messages = messages
        .into_iter()
        .flat_map(|message| {
            let messages = textwrap::wrap(&message, chunk.width.saturating_sub(3) as usize);

            let leading_spaces = messages
                .first()
                .map(|first_message| {
                    " ".repeat(first_message.chars().take_while(|&c| c == ' ').count())
                })
                .unwrap_or_default();

            messages
                .into_iter()
                .enumerate()
                .map(|(i, message)| {
                    let mut message = message.into_owned();

                    if i != 0 {
                        message.insert_str(0, &leading_spaces);
                    }

                    Text::from(message)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let mut list = List::new(messages).block(block);

    if is_scrolling {
        list = list.highlight_style(Style::default().yellow().bold());
    }

    frame.render_stateful_widget(list, chunk, &mut state);
}

fn sleep_thread() {
    sleep(Duration::from_millis(50));
}

enum BlockType {
    Main,
    Out,
    Err,
}

enum BlockFocus {
    Enter(usize),
    Exit,
}

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
    pub scroll_status_out: S,
    pub scroll_status_err: S,
    pub search_message: SM,
}

impl Process {
    pub fn new(name: String, settings: ProcessSettings) -> Process {
        Process {
            name,
            settings,
            out_messages: Default::default(),
            err_messages: Default::default(),
            scroll_status_out: Default::default(),
            scroll_status_err: Default::default(),
            search_message: Default::default(),
        }
    }

    pub fn detach(&self) -> DetachProcess {
        Process {
            name: self.name.clone(),
            settings: self.settings.clone(),
            out_messages: self.out_messages.read_access().clone(),
            err_messages: self.err_messages.read_access().clone(),
            scroll_status_out: self.scroll_status_out.read_access().clone(),
            scroll_status_err: self.scroll_status_err.read_access().clone(),
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

pub struct Focus {
    pub index: usize,
    pub at: usize,
}
