use eframe::egui::{Key, Modifiers};

pub fn format_key(key: Key) -> Option<&'static str> {
    match key {
        Key::A => Some("a"),
        Key::B => Some("b"),
        Key::C => Some("c"),
        Key::D => Some("d"),
        Key::E => Some("e"),
        Key::F => Some("f"),
        Key::G => Some("g"),
        Key::H => Some("h"),
        Key::I => Some("i"),
        Key::J => Some("j"),
        Key::K => Some("k"),
        Key::L => Some("l"),
        Key::M => Some("m"),
        Key::N => Some("n"),
        Key::O => Some("o"),
        Key::P => Some("p"),
        Key::Q => Some("q"),
        Key::R => Some("r"),
        Key::S => Some("s"),
        Key::T => Some("t"),
        Key::U => Some("u"),
        Key::V => Some("v"),
        Key::W => Some("w"),
        Key::X => Some("x"),
        Key::Y => Some("y"),
        Key::Z => Some("z"),
        Key::Num0 => Some("0"),
        Key::Num1 => Some("1"),
        Key::Num2 => Some("2"),
        Key::Num3 => Some("3"),
        Key::Num4 => Some("4"),
        Key::Num5 => Some("5"),
        Key::Num6 => Some("6"),
        Key::Num7 => Some("7"),
        Key::Num8 => Some("8"),
        Key::Num9 => Some("9"),
        Key::Space => Some("space"),
        Key::Enter => Some("enter"),
        Key::Backspace => Some("backspace"),
        Key::Tab => Some("tab"),
        Key::Escape => Some("esc"),
        Key::Delete => Some("delete"),
        Key::Home => Some("home"),
        Key::End => Some("end"),
        Key::PageUp => Some("pageup"),
        Key::PageDown => Some("pagedown"),
        Key::ArrowUp => Some("up"),
        Key::ArrowDown => Some("down"),
        Key::ArrowLeft => Some("left"),
        Key::ArrowRight => Some("right"),
        Key::Minus => Some("minus"),
        Key::Equals => Some("equal"),
        Key::OpenBracket => Some("leftbracket"),
        Key::CloseBracket => Some("rightbracket"),
        Key::Backslash => Some("backslash"),
        Key::Semicolon => Some("semicolon"),
        Key::Quote => Some("quote"),
        Key::Backtick => Some("grave"),
        Key::Comma => Some("comma"),
        Key::Period => Some("period"),
        Key::Slash => Some("slash"),
        Key::F1 => Some("f1"),
        Key::F2 => Some("f2"),
        Key::F3 => Some("f3"),
        Key::F4 => Some("f4"),
        Key::F5 => Some("f5"),
        Key::F6 => Some("f6"),
        Key::F7 => Some("f7"),
        Key::F8 => Some("f8"),
        Key::F9 => Some("f9"),
        Key::F10 => Some("f10"),
        Key::F11 => Some("f11"),
        Key::F12 => Some("f12"),
        _ => None,
    }
}

pub fn format_shortcut(modifiers: &Modifiers, key: Key) -> Option<String> {
    let key_name = format_key(key)?;
    let mut parts = Vec::new();

    if modifiers.ctrl {
        parts.push("ctrl");
    }
    if modifiers.alt {
        parts.push("alt");
    }
    if modifiers.shift {
        parts.push("shift");
    }
    if modifiers.mac_cmd || modifiers.command {
        parts.push("cmd");
    }

    parts.push(key_name);
    Some(parts.join("+"))
}
