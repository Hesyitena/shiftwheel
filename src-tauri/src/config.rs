use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WheelItem {
    pub id: String,
    pub label: String,
    pub caption: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multi: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactive: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WheelConfig {
    pub convert: HashMap<String, Vec<WheelItem>>,
    pub tools: HashMap<String, Vec<WheelItem>>,
    pub actions: Vec<WheelItem>,
}

fn conv(fmt: &str, caption: Option<&str>) -> WheelItem {
    WheelItem {
        id: format!("convert:{fmt}"),
        label: fmt.to_uppercase(),
        caption: caption
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Convert to {}", fmt.to_uppercase())),
        icon: None,
        multi: None,
        interactive: None,
    }
}

fn tool(
    cat: &str,
    id: &str,
    label: &str,
    caption: &str,
    icon: &str,
    multi: bool,
    interactive: Option<&str>,
) -> WheelItem {
    WheelItem {
        id: format!("tool:{cat}.{id}"),
        label: label.to_string(),
        caption: caption.to_string(),
        icon: Some(icon.to_string()),
        multi: if multi { Some(true) } else { None },
        interactive: interactive.map(|s| s.to_string()),
    }
}

fn action(id: &str, label: &str, caption: &str, icon: &str) -> WheelItem {
    WheelItem {
        id: format!("action:{id}"),
        label: label.to_string(),
        caption: caption.to_string(),
        icon: Some(icon.to_string()),
        multi: None,
        interactive: None,
    }
}

/// The wheel contents, built once.
pub fn wheel_config() -> &'static WheelConfig {
    static CONFIG: std::sync::OnceLock<WheelConfig> = std::sync::OnceLock::new();
    CONFIG.get_or_init(get_wheel_config)
}

pub fn get_wheel_config() -> WheelConfig {
    let mut convert = HashMap::new();
    // Plain-text files with other extensions (.json, .log, .xml, code, ...).
    convert.insert(
        "text".into(),
        vec![conv("pdf", None), conv("docx", None), conv("odt", None), conv("txt", None)],
    );
    convert.insert(
        "image".into(),
        vec![
            conv("jpg", None),
            conv("png", None),
            conv("webp", None),
            conv("avif", None),
            conv("tiff", None),
            conv("gif", None),
            conv("bmp", None),
            conv("pdf", None),
            conv("txt", Some("Extract text from the image (OCR)")),
        ],
    );
    convert.insert(
        "pdf".into(),
        vec![
            conv("docx", None),
            conv("txt", None),
            conv("jpg", None),
            conv("png", None),
        ],
    );
    convert.insert(
        "document".into(),
        vec![
            conv("pdf", None),
            conv("docx", None),
            conv("odt", None),
            conv("rtf", None),
            conv("txt", None),
            conv("html", None),
        ],
    );
    convert.insert(
        "sheet".into(),
        vec![
            conv("pdf", None),
            conv("xlsx", None),
            conv("ods", None),
            conv("csv", None),
        ],
    );
    convert.insert(
        "slides".into(),
        vec![
            conv("pdf", None),
            conv("pptx", None),
            conv("odp", None),
        ],
    );
    convert.insert(
        "audio".into(),
        vec![
            conv("mp3", None),
            conv("m4a", None),
            conv("wav", None),
            conv("flac", None),
            conv("ogg", None),
            conv("opus", None),
            conv("aiff", None),
        ],
    );
    convert.insert(
        "video".into(),
        vec![
            conv("mp4", None),
            conv("mov", None),
            conv("mkv", None),
            conv("webm", None),
            conv("gif", None),
            conv("mp3", None),
        ],
    );

    let mut tools = HashMap::new();
    tools.insert(
        "image".into(),
        vec![
            tool("image", "compress", "Compress", "Smaller file size", "compress", false, None),
            tool("image", "half", "50% size", "Halve the dimensions", "half", false, None),
            tool("image", "fit", "Fit 1080p", "Scale down to max 1920x1080", "fit", false, None),
            tool("image", "square", "Square", "Crop the center to 1:1", "square", false, None),
            tool("image", "crop", "Crop", "Select an area to cut out", "crop", false, Some("crop")),
            tool("image", "rotate", "Rotate", "Rotate 90° clockwise", "rotate", false, None),
            tool("image", "flip", "Flip", "Mirror horizontally", "flip", false, None),
            tool("image", "gray", "Black & white", "Remove all color", "gray", false, None),
            tool("image", "trim", "Trim border", "Remove plain margins", "strip", false, None),
            tool("image", "pdf", "To PDF", "Combine into a PDF", "merge", true, None),
        ],
    );
    tools.insert(
        "pdf".into(),
        vec![
            tool("pdf", "compress", "Compress", "Smaller file size", "compress", false, None),
            tool("pdf", "split", "Extract pages", "One file per page", "split", false, None),
            tool("pdf", "images", "Extract images", "Save pages as PNG", "image", false, None),
            tool("pdf", "merge", "Merge PDFs", "Combine into one file", "merge", true, None),
        ],
    );
    tools.insert(
        "audio".into(),
        vec![
            tool("audio", "trim", "Cut edges", "Trim silence at start and end", "scissors", false, None),
            tool("audio", "fade", "Fade in/out", "Smooth volume transition", "fade", false, None),
            tool("audio", "fast", "2x speed", "Double speed, same pitch", "fast", false, None),
            tool("audio", "mono", "To mono", "Mix stereo down to 1 channel", "mono", false, None),
        ],
    );
    tools.insert(
        "video".into(),
        vec![
            tool("video", "mute", "Remove audio", "Save without the sound track", "mute", false, None),
            tool("video", "audio", "Extract audio", "Save sound as MP3", "wave", false, None),
            tool("video", "fast", "2x speed", "Double playback speed", "fast", false, None),
            tool("video", "half", "720p", "Scale down to max 1280x720", "half", false, None),
            tool("video", "reverse", "Reverse", "Play backwards", "reverse", false, None),
        ],
    );

    let actions = vec![
        action("copy-path", "Copy path", "Copy the full path as text", "link"),
        action("copy", "Copy", "Copy to the clipboard, ready to paste", "copy"),
        action("copy-to", "Copy to", "Copy to another folder", "copy-to"),
        action("move-to", "Move to", "Move to another folder", "move-to"),
        action("terminal", "Terminal", "Open the folder in Terminal", "terminal"),
        action("reveal", "Explorer", "Show in Explorer", "folder"),
    ];

    WheelConfig {
        convert,
        tools,
        actions,
    }
}
