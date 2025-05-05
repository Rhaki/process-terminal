use crossterm::event::KeyCode;

#[derive(Clone, PartialEq)]
pub struct ProcessSettings {
    pub messages: MessageSettings,
    pub scroll: ScrollSettings,
    pub clear_regex: bool,
}

impl ProcessSettings {
    pub fn new(messages: MessageSettings) -> Self {
        Self {
            messages,
            scroll: ScrollSettings::Disable,
            clear_regex: true,
        }
    }

    pub fn new_with_scroll(messages: MessageSettings, scroll: ScrollSettings) -> Self {
        Self {
            messages,
            scroll,
            clear_regex: true,
        }
    }

    pub fn disable_clear_regex(self) -> Self {
        Self {
            clear_regex: false,
            ..self
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum MessageSettings {
    None,
    Output,
    Error,
    All,
}

#[derive(Clone, PartialEq)]
pub enum ScrollSettings {
    Disable,
    Enable { up: KeyCode, down: KeyCode },
}

impl ScrollSettings {
    pub fn enable(up: KeyCode, down: KeyCode) -> Self {
        ScrollSettings::Enable { up, down }
    }
}
