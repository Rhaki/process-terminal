use crossterm::event::KeyCode;

#[derive(Clone, PartialEq)]
pub struct ProcessSettings {
    pub messages: MessageSettings,
    pub scroll: ScrollSettings,
}

impl ProcessSettings {
    pub fn new(messages: MessageSettings) -> Self {
        Self {
            messages,
            scroll: ScrollSettings::Disable,
        }
    }

    pub fn new_with_scroll(messages: MessageSettings, scroll: ScrollSettings) -> Self {
        Self { messages, scroll }
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
    Enable {
        up_right: KeyCode,
        down_left: KeyCode,
    },
}

impl ScrollSettings {
    pub fn enable(up_right: KeyCode, down_left: KeyCode) -> Self {
        ScrollSettings::Enable {
            up_right,
            down_left,
        }
    }
}
