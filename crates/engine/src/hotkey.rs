//! Global hotkey combo model: parsing of key names into Win32 modifiers + virtual-key codes.

pub const MOD_ALT: u32 = 0x1;
pub const MOD_CONTROL: u32 = 0x2;
pub const MOD_SHIFT: u32 = 0x4;
pub const MOD_WIN: u32 = 0x8;
pub const MOD_NOREPEAT: u32 = 0x4000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub mods: u32,
    pub vk: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyError {
    NoModifier,
    UnknownKey(String),
    KnownConflict { app: &'static str },
    InUse,
}

const NAMED_KEYS: &[(&str, u32)] = &[
    ("Num Multiply", 0x6A), ("Num Add", 0x6B), ("Num Subtract", 0x6D),
    ("Num Decimal", 0x6E), ("Num Divide", 0x6F), ("Num Enter", 0x0D),
    ("Espaço", 0x20), ("Insert", 0x2D), ("Home", 0x24), ("End", 0x23),
    ("PgUp", 0x21), ("PgDn", 0x22), ("Del", 0x2E),
    ("↑", 0x26), ("↓", 0x28), ("←", 0x25), ("→", 0x27),
    ("Tab", 0x09), ("Enter", 0x0D), ("Backspace", 0x08), ("Esc", 0x1B),
    ("`", 0xC0), ("-", 0xBD), ("=", 0xBB), ("[", 0xDB), ("]", 0xDD),
    ("\\", 0xDC), (";", 0xBA), ("'", 0xDE), (",", 0xBC), (".", 0xBE), ("/", 0xBF),
];

const KNOWN_CONFLICTS: &[(&str, &str)] = &[
    ("Ctrl+Shift+M", "Discord"),
    ("Alt+Z", "NVIDIA App"),
    ("Alt+R", "AMD Software"),
    ("Win+G", "Xbox Game Bar"),
    ("Ctrl+Shift+Esc", "Windows"),
    ("Ctrl+Shift+S", "Steam"),
    ("Shift+Tab", "Steam"),
];

fn modifier(name: &str) -> Option<u32> {
    match name {
        "Ctrl" => Some(MOD_CONTROL),
        "Alt" => Some(MOD_ALT),
        "Shift" => Some(MOD_SHIFT),
        "Win" => Some(MOD_WIN),
        _ => None,
    }
}

fn key_vk(name: &str) -> Option<u32> {
    if let Some(&(_, vk)) = NAMED_KEYS.iter().find(|(n, _)| *n == name) {
        return Some(vk);
    }
    let b = name.as_bytes();
    if b.len() == 1 {
        match b[0] {
            c @ b'A'..=b'Z' => return Some(c as u32),
            c @ b'0'..=b'9' => return Some(c as u32),
            _ => {}
        }
    }
    if let Some(n) = name.strip_prefix('F').and_then(|r| r.parse::<u32>().ok()) {
        if (1..=24).contains(&n) && name == format!("F{n}") {
            return Some(0x70 + n - 1);
        }
    }
    if let Some(d) = name.strip_prefix("Num ") {
        if d.len() == 1 {
            if let c @ b'0'..=b'9' = d.as_bytes()[0] {
                return Some(0x60 + (c - b'0') as u32);
            }
        }
    }
    None
}

pub fn parse(combo: &[String]) -> Result<Hotkey, HotkeyError> {
    let mut mods = 0;
    let mut key: Option<&str> = None;
    for part in combo {
        match modifier(part) {
            Some(m) => mods |= m,
            None => key = Some(part),
        }
    }
    if mods == 0 {
        return Err(HotkeyError::NoModifier);
    }
    let name = key.unwrap_or("");
    let vk = key_vk(name).ok_or_else(|| HotkeyError::UnknownKey(name.to_string()))?;
    Ok(Hotkey { mods: mods | MOD_NOREPEAT, vk })
}

pub fn known_conflict(combo: &[String]) -> Option<&'static str> {
    let joined = combo.join("+");
    KNOWN_CONFLICTS.iter().find(|(k, _)| *k == joined).map(|&(_, app)| app)
}

pub fn display(combo: &[String]) -> String {
    combo.join(" + ")
}

impl HotkeyError {
    pub fn message(&self, combo: &[String]) -> String {
        match self {
            HotkeyError::NoModifier => "Use pelo menos Ctrl, Alt ou Shift junto com a tecla.".to_string(),
            HotkeyError::UnknownKey(_) => "Tecla não suportada. Escolha outra.".to_string(),
            HotkeyError::KnownConflict { app } => {
                format!("{} já é usado por {}. Escolha outra.", display(combo), app)
            }
            HotkeyError::InUse => {
                format!("{} já é usado por outro app. Escolha outra.", display(combo))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(v: &[&str]) -> Vec<String> { v.iter().map(|x| x.to_string()).collect() }

    #[test]
    fn parses_default_combo() {
        let hk = parse(&s(&["Ctrl", "Shift", "O"])).unwrap();
        assert_eq!(hk.vk, 0x4F);
        assert_eq!(hk.mods, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT);
    }
    #[test]
    fn parses_named_keys() {
        for (name, vk) in [("Espaço", 0x20), ("F12", 0x7B), ("Num 7", 0x67), ("PgUp", 0x21),
                           ("↑", 0x26), ("`", 0xC0), ("-", 0xBD), ("=", 0xBB), ("Del", 0x2E), ("5", 0x35)] {
            assert_eq!(parse(&s(&["Alt", name])).unwrap().vk, vk, "{name}");
        }
    }
    #[test]
    fn rejects_combo_without_modifier() {
        assert_eq!(parse(&s(&["O"])), Err(HotkeyError::NoModifier));
    }
    #[test]
    fn rejects_unknown_key() {
        assert!(matches!(parse(&s(&["Ctrl", "Ç"])), Err(HotkeyError::UnknownKey(_))));
    }
    #[test]
    fn known_conflicts_from_prototype() {
        assert_eq!(known_conflict(&s(&["Ctrl", "Shift", "M"])), Some("Discord"));
        assert_eq!(known_conflict(&s(&["Shift", "Tab"])), Some("Steam"));
        assert_eq!(known_conflict(&s(&["Ctrl", "Shift", "O"])), None);
    }
    #[test]
    fn messages_in_pt_br() {
        assert_eq!(display(&s(&["Ctrl", "Alt", "P"])), "Ctrl + Alt + P");
        assert_eq!(HotkeyError::KnownConflict { app: "Discord" }.message(&s(&["Ctrl", "Shift", "M"])),
                   "Ctrl + Shift + M já é usado por Discord. Escolha outra.");
        assert_eq!(HotkeyError::InUse.message(&s(&["Ctrl", "Shift", "O"])),
                   "Ctrl + Shift + O já é usado por outro app. Escolha outra.");
        assert_eq!(HotkeyError::NoModifier.message(&s(&["O"])),
                   "Use pelo menos Ctrl, Alt ou Shift junto com a tecla.");
        assert_eq!(HotkeyError::UnknownKey("Ç".into()).message(&s(&["Ctrl", "Ç"])),
                   "Tecla não suportada. Escolha outra.");
    }
}
