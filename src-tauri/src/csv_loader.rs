use crate::types::InputFrame;
use anyhow::Result;
use csv::ReaderBuilder;
use std::collections::HashMap;
use std::path::Path;

pub fn load_csv(path: &Path) -> Result<Vec<InputFrame>> {
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;

    let headers = reader.headers()?.clone();
    let mut frames = Vec::new();

    for result in reader.records() {
        let record = result?;
        
        let duration: u32 = record.get(0)
            .ok_or_else(|| anyhow::anyhow!("Missing duration"))?
            .parse()?;
        
        let direction: u8 = record.get(1)
            .ok_or_else(|| anyhow::anyhow!("Missing direction"))?
            .parse()?;

        let mut buttons = HashMap::new();
        
        // duration, direction以外のカラムをボタンとして処理
        for (i, header) in headers.iter().enumerate().skip(2) {
            if let Some(value_str) = record.get(i) {
                if let Ok(value) = value_str.parse::<u8>() {
                    buttons.insert(header.to_string(), value);
                }
            }
        }

        frames.push(InputFrame {
            duration,
            direction,
            buttons,
        });
    }

    Ok(frames)
}
