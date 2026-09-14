use std::fs::{create_dir_all, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use serde::Serialize;
use serde_json::json;

pub struct ExperimentLogger {
    dir: PathBuf,
    writer: Option<BufWriter<File>>,
    path: Option<PathBuf>,
}

impl ExperimentLogger {
    pub fn new(dir: impl Into<PathBuf>) -> Self { Self { dir: dir.into(), writer: None, path: None } }
    pub fn start<T: Serialize>(&mut self, meta: &T) -> Result<()> {
        create_dir_all(&self.dir)?;
        let name = format!("experiment_{}.jsonl", Utc::now().format("%Y%m%dT%H%M%S%.3fZ"));
        let path = self.dir.join(name);
        self.writer = Some(BufWriter::new(File::create(&path)?));
        self.path = Some(path);
        self.write("experiment_start", meta)
    }
    pub fn write<T: Serialize>(&mut self, kind: &str, data: &T) -> Result<()> {
        if let Some(w) = self.writer.as_mut() {
            let row = json!({ "ts": Utc::now().timestamp_millis() as f64 / 1000.0, "kind": kind, "data": data });
            serde_json::to_writer(&mut *w, &row)?;
            w.write_all(b"\n")?;
        }
        Ok(())
    }
    pub fn flush(&mut self) -> Result<()> {
        if let Some(w) = self.writer.as_mut() { w.flush()?; }
        Ok(())
    }
    pub fn close<T: Serialize>(&mut self, summary: &T) -> Result<()> {
        self.write("experiment_end", summary)?;
        self.flush()?;
        self.writer = None;
        Ok(())
    }
    pub fn path(&self) -> Option<String> { self.path.as_ref().map(|p| p.display().to_string()) }
    pub fn active(&self) -> bool { self.writer.is_some() }
}
