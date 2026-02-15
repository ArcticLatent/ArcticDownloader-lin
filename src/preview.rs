use anyhow::Result;

pub fn open_lora_preview(url: &str) -> Result<()> {
    open::that(url)?;
    Ok(())
}
