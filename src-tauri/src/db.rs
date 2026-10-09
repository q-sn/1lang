//! SQLite: history, translation cache and usage statistics.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use onelang_core::{DictionaryEntry, TranslationResult};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;

pub struct Db {
    conn: Connection,
}

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Main,
    Selection,
    Clipboard,
    Ocr,
    Replace,
}

impl Origin {
    fn as_str(self) -> &'static str {
        match self {
            Origin::Main => "main",
            Origin::Selection => "selection",
            Origin::Clipboard => "clipboard",
            Origin::Ocr => "ocr",
            Origin::Replace => "replace",
        }
    }
    fn parse(s: &str) -> Origin {
        match s {
            "selection" => Origin::Selection,
            "clipboard" => Origin::Clipboard,
            "ocr" => Origin::Ocr,
            "replace" => Origin::Replace,
            _ => Origin::Main,
        }
    }
}

#[derive(Serialize, Deserialize, Type, Clone, Debug)]
pub struct HistoryItem {
    pub id: i32,
    /// Unix time, ms.
    pub created_at: f64,
    pub source_text: String,
    pub result_text: String,
    pub dictionary: Option<DictionaryEntry>,
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub provider_name: String,
    pub model: Option<String>,
    pub cost_usd: Option<f64>,
    pub origin: Origin,
    pub app: Option<String>,
    pub favorite: bool,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, Default)]
pub struct HistoryQuery {
    pub search: Option<String>,
    pub favorites_only: bool,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, Default)]
pub struct UsageRow {
    pub provider_id: String,
    pub provider_name: String,
    pub requests: u32,
    pub input_tokens: f64,
    pub output_tokens: f64,
    pub characters: f64,
    pub cost_usd: f64,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug, Default)]
pub struct UsageStats {
    pub today: Vec<UsageRow>,
    pub month: Vec<UsageRow>,
    pub total: Vec<UsageRow>,
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Local calendar day "YYYY-MM-DD" from SQLite (handles the timezone for us).
const TODAY: &str = "date('now', 'localtime')";

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Db> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at INTEGER NOT NULL,
                source_text TEXT NOT NULL,
                result_text TEXT NOT NULL,
                dictionary TEXT,
                source_lang TEXT,
                target_lang TEXT NOT NULL,
                provider_name TEXT NOT NULL,
                model TEXT,
                cost_usd REAL,
                origin TEXT NOT NULL,
                app TEXT,
                favorite INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS history_created ON history(created_at DESC);
             CREATE TABLE IF NOT EXISTS cache (
                key TEXT PRIMARY KEY,
                result TEXT NOT NULL,
                created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS usage (
                day TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                provider_name TEXT NOT NULL,
                requests INTEGER NOT NULL DEFAULT 0,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                characters INTEGER NOT NULL DEFAULT 0,
                cost_usd REAL NOT NULL DEFAULT 0,
                PRIMARY KEY (day, provider_id)
             );",
        )?;
        Ok(Db { conn })
    }

    pub fn add_history(
        &self,
        source: &str,
        r: &TranslationResult,
        origin: Origin,
        app: Option<&str>,
        limit: u32,
    ) -> rusqlite::Result<i64> {
        // Re-translating the same text moves it to the top instead of duplicating it.
        self.conn.execute(
            "DELETE FROM history WHERE source_text = ?1 AND target_lang = ?2 AND favorite = 0",
            params![source, r.target_lang],
        )?;
        self.conn.execute(
            "INSERT INTO history (created_at, source_text, result_text, dictionary, source_lang, target_lang,
                                  provider_name, model, cost_usd, origin, app)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                now_ms(),
                source,
                r.text,
                r.dictionary.as_ref().and_then(|d| serde_json::to_string(d).ok()),
                r.source_lang,
                r.target_lang,
                r.provider_name,
                r.model,
                r.cost_usd,
                origin.as_str(),
                app,
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.conn.execute(
            "DELETE FROM history WHERE favorite = 0 AND id NOT IN
               (SELECT id FROM history ORDER BY created_at DESC LIMIT ?1)",
            params![limit.max(100)],
        )?;
        Ok(id)
    }

    pub fn list_history(&self, q: &HistoryQuery) -> rusqlite::Result<Vec<HistoryItem>> {
        let search = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{s}%"));
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, source_text, result_text, dictionary, source_lang, target_lang,
                    provider_name, model, cost_usd, origin, app, favorite
             FROM history
             WHERE (?1 IS NULL OR source_text LIKE ?1 OR result_text LIKE ?1)
               AND (?2 = 0 OR favorite = 1)
             ORDER BY created_at DESC LIMIT ?3 OFFSET ?4",
        )?;
        let rows = stmt.query_map(
            params![search, q.favorites_only as i32, if q.limit == 0 { 100 } else { q.limit }, q.offset],
            |row| {
                let dict: Option<String> = row.get(4)?;
                let origin: String = row.get(10)?;
                Ok(HistoryItem {
                    id: row.get(0)?,
                    created_at: row.get::<_, i64>(1)? as f64,
                    source_text: row.get(2)?,
                    result_text: row.get(3)?,
                    dictionary: dict.and_then(|d| serde_json::from_str(&d).ok()),
                    source_lang: row.get(5)?,
                    target_lang: row.get(6)?,
                    provider_name: row.get(7)?,
                    model: row.get(8)?,
                    cost_usd: row.get(9)?,
                    origin: Origin::parse(&origin),
                    app: row.get(11)?,
                    favorite: row.get::<_, i32>(12)? != 0,
                })
            },
        )?;
        rows.collect()
    }

    pub fn toggle_favorite(&self, id: i32) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE history SET favorite = 1 - favorite WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn delete_history(&self, id: i32) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear_history(&self, keep_favorites: bool) -> rusqlite::Result<()> {
        if keep_favorites {
            self.conn.execute("DELETE FROM history WHERE favorite = 0", [])?;
        } else {
            self.conn.execute("DELETE FROM history", [])?;
        }
        Ok(())
    }

    pub fn cache_get(&self, key: &str, max_age_ms: i64) -> Option<TranslationResult> {
        let row: Option<(String, i64)> = self
            .conn
            .query_row("SELECT result, created_at FROM cache WHERE key = ?1", params![key], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()
            .ok()
            .flatten();
        let (json, created) = row?;
        if now_ms() - created > max_age_ms {
            return None;
        }
        serde_json::from_str(&json).ok()
    }

    pub fn cache_put(&self, key: &str, r: &TranslationResult) -> rusqlite::Result<()> {
        let json = serde_json::to_string(r).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO cache (key, result, created_at) VALUES (?1, ?2, ?3)",
            params![key, json, now_ms()],
        )?;
        // Keep the cache bounded.
        self.conn.execute(
            "DELETE FROM cache WHERE key NOT IN (SELECT key FROM cache ORDER BY created_at DESC LIMIT 5000)",
            [],
        )?;
        Ok(())
    }

    pub fn clear_cache(&self) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM cache", [])?;
        Ok(())
    }

    pub fn record_usage(&self, r: &TranslationResult) -> rusqlite::Result<()> {
        self.conn.execute(
            &format!(
                "INSERT INTO usage (day, provider_id, provider_name, requests, input_tokens, output_tokens, characters, cost_usd)
                 VALUES ({TODAY}, ?1, ?2, 1, ?3, ?4, ?5, ?6)
                 ON CONFLICT(day, provider_id) DO UPDATE SET
                    provider_name = excluded.provider_name,
                    requests = requests + 1,
                    input_tokens = input_tokens + excluded.input_tokens,
                    output_tokens = output_tokens + excluded.output_tokens,
                    characters = characters + excluded.characters,
                    cost_usd = cost_usd + excluded.cost_usd"
            ),
            params![
                r.provider_id,
                r.provider_name,
                r.usage.input_tokens,
                r.usage.output_tokens,
                r.usage.characters,
                r.cost_usd.unwrap_or(0.0)
            ],
        )?;
        Ok(())
    }

    fn usage_since(&self, condition: &str) -> rusqlite::Result<Vec<UsageRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT provider_id, MAX(provider_name), SUM(requests), SUM(input_tokens), SUM(output_tokens),
                    SUM(characters), SUM(cost_usd)
             FROM usage WHERE {condition} GROUP BY provider_id ORDER BY SUM(requests) DESC"
        ))?;
        let rows = stmt.query_map([], |r| {
            Ok(UsageRow {
                provider_id: r.get(0)?,
                provider_name: r.get(1)?,
                requests: r.get::<_, i64>(2)? as u32,
                input_tokens: r.get::<_, i64>(3)? as f64,
                output_tokens: r.get::<_, i64>(4)? as f64,
                characters: r.get::<_, i64>(5)? as f64,
                cost_usd: r.get(6)?,
            })
        })?;
        rows.collect()
    }

    pub fn usage_stats(&self) -> rusqlite::Result<UsageStats> {
        Ok(UsageStats {
            today: self.usage_since(&format!("day = {TODAY}"))?,
            month: self.usage_since(&format!("substr(day, 1, 7) = substr({TODAY}, 1, 7)"))?,
            total: self.usage_since("1 = 1")?,
        })
    }

    pub fn reset_usage(&self) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM usage", [])?;
        Ok(())
    }
}

/// Stable 64-bit FNV-1a hash for cache keys.
pub fn cache_key(parts: &[&str]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for p in parts {
        for b in p.as_bytes().iter().chain(std::iter::once(&0u8)) {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}
