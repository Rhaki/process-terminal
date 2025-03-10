use {
    crate::{shared::Shared, ExitCallback, SharedMessages},
    anyhow::{anyhow, Result},
    crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers},
};

pub struct KeyBoardActions {
    actions: Vec<Action>,
    focus: Shared<Option<usize>>,
}

impl KeyBoardActions {
    pub fn new(main_messages: SharedMessages) -> (Self, BaseStatus, Shared<ExitCallback>) {
        let base_status: BaseStatus = Default::default();
        let exit_callback: Shared<ExitCallback> = Default::default();

        let main_action_scroll = ActionScroll {
            status: base_status.main_scroll.clone(),
            messages: main_messages.clone(),
        };

        let actions = vec![
            Action {
                event: KeyCode::Char('c').into_event(KeyModifiers::CONTROL),
                data: ActionType::Close(exit_callback.clone()),
            },
            Action {
                event: KeyCode::Up.into_event_no_modifier(),
                data: ActionType::ScrollUp(main_action_scroll.clone()),
            },
            Action {
                event: KeyCode::Down.into_event_no_modifier(),
                data: ActionType::ScrollDown(main_action_scroll.clone()),
            },
            Action {
                event: KeyCode::Left.into_event_no_modifier(),
                data: ActionType::ScrollLeft(main_action_scroll.clone()),
            },
            Action {
                event: KeyCode::Right.into_event_no_modifier(),
                data: ActionType::ScrollRight(main_action_scroll.clone()),
            },
            Action {
                event: KeyCode::Char('0').into_event_no_modifier(),
                data: ActionType::Focus((0, base_status.focus.clone())),
            },
            Action {
                event: KeyCode::Esc.into_event_no_modifier(),
                data: ActionType::RemoveFocus(base_status.focus.clone()),
            },
        ];

        (
            Self {
                actions,
                focus: base_status.focus.clone(),
            },
            base_status,
            exit_callback,
        )
    }

    pub fn apply_event(&self, event: Event) {
        let events = self
            .actions
            .iter()
            .filter(|action| action.event == event)
            .collect::<Vec<_>>();

        for action in events {
            action.data.apply();
        }
    }

    pub fn push(&mut self, action: Action) {
        self.actions.push(action);
    }

    pub fn push_focus(&mut self, indexes: &[usize]) -> Result<()> {
        for index in indexes {
            let char = to_char(*index)?;

            self.push(Action::new(
                KeyCode::Char(char).into_event_no_modifier(),
                ActionType::Focus((*index, self.focus.clone())),
            ));
        }

        Ok(())
    }
}

pub struct Action {
    pub event: Event,
    pub data: ActionType,
}

impl Action {
    pub fn new(event: Event, data: ActionType) -> Self {
        Self { event, data }
    }
}

pub enum ActionType {
    Close(Shared<ExitCallback>),
    ScrollUp(ActionScroll),
    ScrollDown(ActionScroll),
    ScrollLeft(ActionScroll),
    ScrollRight(ActionScroll),
    StopScrolling(Shared<ScrollStatus>),
    Focus((usize, Shared<Option<usize>>)),
    RemoveFocus(Shared<Option<usize>>),
}

impl ActionType {
    pub fn apply(&self) {
        match self {
            ActionType::Close(exit_callback) => {
                ratatui::restore();

                if let Some(callback) = exit_callback.read_access().as_ref() {
                    callback();
                }

                std::process::exit(0);
            }
            ActionType::ScrollUp(shared) => {
                shared.status.write_with(|mut status| {
                    if let Some(y) = &mut status.y {
                        *y = y.saturating_sub(1);
                    } else {
                        status.y = Some(shared.messages.read_access().len() as u16);
                    }
                });
            }
            ActionType::ScrollDown(shared) => {
                shared.status.write_with(|mut status| {
                    if let Some(y) = &mut status.y {
                        *y += 1
                    }
                });
            }
            ActionType::ScrollLeft(shared) => {
                shared.status.write_with(|mut status| {
                    status.x = status.x.saturating_sub(1);
                });
            }
            ActionType::ScrollRight(shared) => {
                shared.status.write_with(|mut status| {
                    status.x = status.x + 1;
                });
            }
            ActionType::StopScrolling(shared) => {
                shared.write_with(|mut status| {
                    status.y = None;
                });
            }
            ActionType::Focus((index, shared)) => {
                shared.write_with(|mut focus| {
                    *focus = Some(*index);
                });
            }
            ActionType::RemoveFocus(shared) => {
                shared.write_with(|mut focus| {
                    *focus = None;
                });
            }
        }
    }
}

#[derive(Default, Clone, PartialEq)]
pub(crate) struct ScrollStatus {
    pub x: u16,
    pub y: Option<u16>,
}

#[derive(Clone)]
pub(crate) struct ActionScroll {
    pub status: Shared<ScrollStatus>,
    pub messages: SharedMessages,
}

pub trait KeyCodeExt: Sized {
    fn into_event(self, modifier: KeyModifiers) -> Event;

    fn into_event_no_modifier(self) -> Event {
        self.into_event(KeyModifiers::empty())
    }
}

impl KeyCodeExt for KeyCode {
    fn into_event(self, modifier: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new(self, modifier))
    }
}

pub type DetachBaseStatus = BaseStatus<ScrollStatus, Option<usize>>;

#[derive(Default, Clone, PartialEq)]
pub struct BaseStatus<MS = Shared<ScrollStatus>, F = Shared<Option<usize>>> {
    pub main_scroll: MS,
    pub focus: F,
}

impl BaseStatus {
    pub fn detach(&self) -> BaseStatus<ScrollStatus, Option<usize>> {
        BaseStatus {
            main_scroll: self.main_scroll.read_access().clone(),
            focus: self.focus.read_access().clone(),
        }
    }
}

fn to_char(index: usize) -> Result<char> {
    let str_index = index.to_string();
    let mut chars = str_index.chars();

    if let (Some(char), None) = (chars.next(), chars.next()) {
        Ok(char)
    } else {
        return Err(anyhow!("Can't add more then 9 processes."));
    }
}
