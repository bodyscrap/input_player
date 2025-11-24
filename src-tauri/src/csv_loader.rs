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
            thumb_lx: 0,
            thumb_ly: 0,
            thumb_rx: 0,
            thumb_ry: 0,
            left_trigger: 0,
            right_trigger: 0,
        });
    }

    Ok(frames)
}

pub fn get_csv_button_names(path: &Path) -> Result<Vec<String>> {
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;

    let headers = reader.headers()?;
    
    // 3列目以降（インデックス2以降）がボタン名
    let button_names: Vec<String> = headers.iter()
        .skip(2)
        .map(|s| s.to_string())
        .collect();

    Ok(button_names)
}
