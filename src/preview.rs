use anyhow::Result;

#[cfg(target_os = "windows")]
pub fn open_lora_preview(url: &str) -> Result<()> {
    let target = url.to_string();
    std::thread::Builder::new()
        .name("lora-preview-window".to_string())
        .spawn(move || {
            if let Err(err) = run_preview_window(&target) {
                log::warn!("failed to open in-app preview window: {err:#}");
            }
        })?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn open_lora_preview(url: &str) -> Result<()> {
    open::that(url)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn run_preview_window(url: &str) -> Result<()> {
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    // `url` comes from third-party metadata (Civitai/HF), so it must be
    // treated as untrusted before it's spliced into HTML/JS below.
    if !is_safe_http_url(url) {
        return Err(anyhow::anyhow!(
            "refusing to open preview window for non-http(s) URL"
        ));
    }

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("LoRA Preview")
        .build(&event_loop)?;

    let html = if is_video_url(url) {
        // `serde_json::to_string` correctly escapes the URL as a JS string
        // literal, but a literal `</script>` substring inside that literal
        // would still terminate the surrounding <script> block early (the
        // HTML tokenizer looks for that byte sequence regardless of JS
        // string context). Neutralize `<` so `</script>` can never appear.
        let js_string_literal = serde_json::to_string(url)?.replace('<', "\\u003C");
        format!(
            r#"<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <style>
    html, body {{ margin: 0; width: 100%; height: 100%; background: #10151d; }}
    video {{ width: 100%; height: 100%; object-fit: contain; background: #000; }}
  </style>
</head>
<body>
  <video id="preview" controls autoplay muted loop playsinline></video>
  <script>
    const src = {js_string_literal};
    const player = document.getElementById("preview");
    player.src = src;
    player.play().catch(() => {{}});
  </script>
</body>
</html>"#
        )
    } else {
        // HTML-attribute-escape the URL rather than JSON-escaping it: JSON
        // escaping is for JS string contexts, and leaves raw `"` in place
        // for a URL like `x.png" onerror="...`, which would close the
        // attribute early and let arbitrary attributes be injected.
        let attr_escaped_url = html_escape_attribute(url);
        format!(
            r#"<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <style>
    html, body {{ margin: 0; width: 100%; height: 100%; background: #10151d; }}
    img {{ width: 100%; height: 100%; object-fit: contain; }}
  </style>
</head>
<body>
  <img src="{attr_escaped_url}" />
</body>
</html>"#
        )
    };

    let _webview = WebViewBuilder::new().with_html(html).build(&window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    })
}

#[cfg(target_os = "windows")]
fn is_safe_http_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

#[cfg(target_os = "windows")]
fn html_escape_attribute(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(target_os = "windows")]
fn is_video_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.ends_with(".mp4")
        || lower.ends_with(".webm")
        || lower.ends_with(".mov")
        || lower.contains(".mp4?")
        || lower.contains(".webm?")
        || lower.contains(".mov?")
}
