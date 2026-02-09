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

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("LoRA Preview")
        .build(&event_loop)?;

    let is_video = is_video_url(url);
    let escaped_url = serde_json::to_string(url)?;
    let html = if is_video {
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
    const src = {escaped_url};
    const player = document.getElementById("preview");
    player.src = src;
    player.play().catch(() => {{}});
  </script>
</body>
</html>"#
        )
    } else {
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
  <img src={escaped_url} />
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
fn is_video_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.ends_with(".mp4")
        || lower.ends_with(".webm")
        || lower.ends_with(".mov")
        || lower.contains(".mp4?")
        || lower.contains(".webm?")
        || lower.contains(".mov?")
}
