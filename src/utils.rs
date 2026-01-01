use std::io::{self, Read};
use std::process::Command;

pub fn fix_icon_name(id: &str) -> String {
    let id_lower = id.to_lowercase();

    if id_lower.contains("firefox") {
        return "firefox".to_string();
    }
    if id_lower.contains("chrome") {
        return "google-chrome".to_string();
    }
    if id_lower.contains("chromium") {
        return "chromium".to_string();
    }
    if id_lower.contains("zen") {
        return "zen-browser".to_string();
    }
    if id_lower.contains("steam") {
        return "steam".to_string();
    }
    if id_lower.contains("code") {
        return "com.visualstudio.code".to_string();
    }
    if id_lower.contains("vscode") {
        return "com.visualstudio.code".to_string();
    }
    if id_lower.contains("discord") {
        return "discord".to_string();
    }
    if id_lower.contains("telegram") {
        return "telegram".to_string();
    }
    if id_lower.contains("spotify") {
        return "spotify".to_string();
    }
    if id_lower.contains("gimp") {
        return "gimp".to_string();
    }
    if id_lower.contains("inkscape") {
        return "inkscape".to_string();
    }
    if id_lower.contains("blender") {
        return "blender".to_string();
    }
    if id_lower.contains("obs") {
        return "com.obsproject.Studio".to_string();
    }
    if id_lower.contains("vlc") {
        return "vlc".to_string();
    }
    if id_lower.contains("thunderbird") {
        return "thunderbird".to_string();
    }
    if id_lower.contains("libreoffice") {
        if id_lower.contains("writer") {
            return "libreoffice-writer".to_string();
        }
        if id_lower.contains("calc") {
            return "libreoffice-calc".to_string();
        }
        if id_lower.contains("impress") {
            return "libreoffice-impress".to_string();
        }
        return "libreoffice-startcenter".to_string();
    }
    if id_lower.contains("files") || id_lower.contains("nautilus") {
        return "system-file-manager".to_string();
    }
    if id_lower.contains("terminal")
        || id_lower.contains("konsole")
        || id_lower.contains("kitty")
        || id_lower.contains("alacritty")
    {
        return "utilities-terminal".to_string();
    }
    if id_lower.contains("launcher") {
        return "view-grid-symbolic".to_string();
    }

    id.to_string()
}

pub fn exec_command(cmd: &str) -> String {
    let output = Command::new("sh").arg("-c").arg(cmd).output();

    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => String::new(),
    }
}
