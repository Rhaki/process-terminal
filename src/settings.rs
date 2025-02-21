use crossterm::event::KeyCode;

#[derive(Clone)]
pub struct ProcessSettings {
    pub messages: MessageSettings,
    pub scroll: ScrollSettings,
}

impl ProcessSettings {
    pub fn new(messages: MessageSettings) -> Self {
        Self {
            messages,
            scroll: ScrollSettings::disable(),
        }
    }

    pub fn new_with_scroll(messages: MessageSettings, scroll: ScrollSettings) -> Self {
        Self { messages, scroll }
    }
}

#[derive(Clone)]
pub enum MessageSettings {
    Output,
    Error,
    All,
}

#[derive(Clone)]
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

    pub fn disable() -> Self {
        ScrollSettings::Disable
    }
}
