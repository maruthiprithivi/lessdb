//! Optional language-model voice for Lessville agents.
//!
//! Design rules (kept deliberately small):
//! * One call = one concise task. Only two tasks exist:
//!   - `phrase`: rewrite a line the deterministic policy already chose, in the
//!     speaker's voice, without adding facts (≤ 18 words).
//!   - `reflect`: at night, turn up to six of the agent's own memories from
//!     that day into one first-person lesson (≤ 25 words), stored as a new
//!     permanent private memory.
//! * Context is pulled from LessDB with an explicit, bounded SQL query and the
//!   ids of every row used are recorded with the call.
//! * Decisions never depend on model output. If the model is unavailable,
//!   over budget or returns something invalid, the rule-based text stands.
//! * Every call (accepted, rejected or failed) is an `lv_llm_calls` row.
//!
//! The model itself runs behind the Cloudflare Worker (`/internal/llm`), which
//! enforces the hard daily neuron budget. This module never sees provider keys.

use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc};

use crate::context::{RecallIntent, recall};
use crate::sim::{DAY, Sim};
use crate::store::{Store, q};

pub const MODEL: &str = "@cf/meta/llama-3.1-8b-instruct-fp8-fast";
const PHRASE_WORDS: usize = 18;
const REFLECT_WORDS: usize = 25;
const PHRASE_COOLDOWN: Duration = Duration::from_secs(40);
const MAX_QUEUE_FOR_PHRASE: usize = 4;

pub enum Job {
    Phrase {
        agent: String,
        text: String,
        minute: i64,
    },
    Reflect {
        agent: String,
        day: i64,
        minute: i64,
    },
}

#[derive(Clone)]
pub struct Gateway {
    url: String,
    key: String,
}

#[derive(Default, Clone, serde::Serialize)]
pub struct Status {
    pub enabled: bool,
    pub model: String,
    pub calls_ok: u64,
    pub calls_rejected: u64,
    pub calls_failed: u64,
    pub skipped: u64,
    pub budget_used: f64,
    pub budget_cap: f64,
    pub budget_exhausted: bool,
    pub last_error: Option<String>,
}

pub fn from_env() -> Option<Gateway> {
    let url = std::env::var("LESSVILLE_LLM_URL")
        .ok()
        .filter(|u| u.starts_with("http"))?;
    let key = std::env::var("LESSVILLE_INTERNAL_KEY")
        .ok()
        .filter(|k| k.len() >= 16)?;
    Some(Gateway { url, key })
}

pub fn spawn(
    gw: Gateway,
    mut rx: mpsc::UnboundedReceiver<Job>,
    sim: Arc<Mutex<Sim>>,
    store: Arc<Store>,
    status: Arc<Mutex<Status>>,
) {
    tokio::spawn(async move {
        {
            let mut st = status.lock().await;
            st.enabled = true;
            st.model = MODEL.into();
        }
        let mut last_phrase: HashMap<String, Instant> = HashMap::new();
        let mut exhausted_at: Option<Instant> = None;
        while let Some(job) = rx.recv().await {
            if let Some(t) = exhausted_at {
                // Budget resets at UTC midnight; re-probe at most every 10 minutes.
                if t.elapsed() < Duration::from_secs(600) {
                    status.lock().await.skipped += 1;
                    continue;
                }
                exhausted_at = None;
            }
            let outcome = match job {
                Job::Phrase {
                    agent,
                    text,
                    minute,
                } => {
                    let recent = last_phrase
                        .get(&agent)
                        .map(|t| t.elapsed() < PHRASE_COOLDOWN)
                        .unwrap_or(false);
                    if recent || rx.len() > MAX_QUEUE_FOR_PHRASE {
                        status.lock().await.skipped += 1;
                        continue;
                    }
                    last_phrase.insert(agent.clone(), Instant::now());
                    phrase(&gw, &sim, &store, &agent, &text, minute).await
                }
                Job::Reflect { agent, day, minute } => {
                    reflect(&gw, &sim, &store, &agent, day, minute).await
                }
            };
            let mut st = status.lock().await;
            match outcome {
                Outcome::Ok { used, cap } => {
                    st.calls_ok += 1;
                    st.budget_used = used;
                    st.budget_cap = cap;
                    st.budget_exhausted = false;
                }
                Outcome::Rejected { used, cap } => {
                    st.calls_rejected += 1;
                    st.budget_used = used;
                    st.budget_cap = cap;
                }
                Outcome::Exhausted { used, cap } => {
                    st.budget_exhausted = true;
                    st.budget_used = used;
                    st.budget_cap = cap;
                    exhausted_at = Some(Instant::now());
                }
                Outcome::Skipped => st.skipped += 1,
                Outcome::Failed(e) => {
                    st.calls_failed += 1;
                    st.last_error = Some(e);
                }
            }
        }
    });
}

enum Outcome {
    Ok { used: f64, cap: f64 },
    Rejected { used: f64, cap: f64 },
    Exhausted { used: f64, cap: f64 },
    Skipped,
    Failed(String),
}

struct Reply {
    text: String,
    input_tokens: i64,
    output_tokens: i64,
    neurons: f64,
    used: f64,
    cap: f64,
}

enum CallErr {
    Exhausted { used: f64, cap: f64 },
    Other(String),
}

fn call(
    gw: &Gateway,
    task: &str,
    agent: &str,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<Reply, CallErr> {
    let body = json!({
        "task": task, "agent": agent, "max_tokens": max_tokens,
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
    })
    .to_string();
    // Header and body go through stdin/config so the key never appears in argv.
    let cfg = format!(
        "url = \"{}\"\nrequest = \"POST\"\nheader = \"content-type: application/json\"\nheader = \"x-lessville-key: {}\"\ndata-binary = \"@-\"\nmax-time = 25\nsilent\nshow-error\nwrite-out = \"\\n%{{http_code}}\"\n",
        gw.url, gw.key
    );
    let dir = std::env::temp_dir().join(format!("lv-llm-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let cfg_path = dir.join("curl.cfg");
    std::fs::write(&cfg_path, cfg).map_err(|e| CallErr::Other(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&cfg_path, std::fs::Permissions::from_mode(0o600));
    }
    let mut child = Command::new("curl")
        .arg("-K")
        .arg(&cfg_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CallErr::Other(format!("curl: {e}")))?;
    child.stdin.take().map(|mut w| w.write_all(body.as_bytes()));
    let out = child
        .wait_with_output()
        .map_err(|e| CallErr::Other(e.to_string()))?;
    let _ = std::fs::remove_file(&cfg_path);
    let raw = String::from_utf8_lossy(&out.stdout).to_string();
    let (payload, code) = raw.rsplit_once('\n').unwrap_or(("", raw.as_str()));
    let code: u16 = code.trim().parse().unwrap_or(0);
    let v: Value = serde_json::from_str(payload).unwrap_or(Value::Null);
    let used = v
        .pointer("/budget/used")
        .and_then(|x| x.as_f64())
        .unwrap_or(0.0);
    let cap = v
        .pointer("/budget/cap")
        .and_then(|x| x.as_f64())
        .unwrap_or(0.0);
    if code == 429 {
        return Err(CallErr::Exhausted { used, cap });
    }
    if code != 200 {
        let msg = v
            .get("error")
            .and_then(|e| e.as_str())
            .map(String::from)
            .unwrap_or_else(|| String::from_utf8_lossy(&out.stderr).trim().to_string());
        return Err(CallErr::Other(format!("gateway HTTP {code}: {msg}")));
    }
    Ok(Reply {
        text: v
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        input_tokens: v
            .pointer("/usage/prompt_tokens")
            .and_then(|x| x.as_i64())
            .unwrap_or(0),
        output_tokens: v
            .pointer("/usage/completion_tokens")
            .and_then(|x| x.as_i64())
            .unwrap_or(0),
        neurons: v.get("neurons").and_then(|x| x.as_f64()).unwrap_or(0.0),
        used,
        cap,
    })
}

async fn call_async(
    gw: &Gateway,
    task: &str,
    agent: &str,
    system: String,
    user: String,
    max_tokens: u32,
) -> Result<Reply, CallErr> {
    let gw = gw.clone();
    let (task, agent) = (task.to_string(), agent.to_string());
    tokio::task::spawn_blocking(move || call(&gw, &task, &agent, &system, &user, max_tokens))
        .await
        .unwrap_or_else(|e| Err(CallErr::Other(e.to_string())))
}

/// Keep one plain line the pixel font can render; reject anything that adds facts.
pub fn clean(raw: &str, max_words: usize, source: &str) -> Result<String, String> {
    let line = raw
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut t: String = line
        .replace(['\u{2018}', '\u{2019}'], "'")
        .replace(['\u{201C}', '\u{201D}'], "\"")
        .replace(
            ['\u{2012}', '\u{2013}', '\u{2014}', '\u{2015}', '\u{2212}'],
            " - ",
        )
        .replace(['\u{00A0}', '\u{202F}'], " ")
        .replace('\u{2026}', "...");
    t = t
        .trim()
        .trim_matches(|c| c == '"' || c == '\'' || c == '*')
        .trim()
        .to_string();
    if let Some((head, rest)) = t.split_once(':') {
        if head.split_whitespace().count() <= 2 && !rest.trim().is_empty() {
            t = rest.trim().trim_matches('"').to_string();
        }
    }
    if t.len() < 3 {
        return Err("empty output".into());
    }
    if t.chars().any(|c| !(c.is_ascii_graphic() || c == ' ')) {
        return Err("characters outside the game font".into());
    }
    let words = t.split_whitespace().count();
    if words > max_words {
        return Err(format!("{words} words (limit {max_words})"));
    }
    let lower = t.to_lowercase();
    for bad in [
        "as an ai",
        "language model",
        "i cannot",
        "i can't help",
        "sorry, but",
        "assistant",
        "http",
        "@",
    ] {
        if lower.contains(bad) {
            return Err(format!("disallowed phrase '{bad}'"));
        }
    }
    let src_nums = numbers(source);
    if let Some(n) = numbers(&t).into_iter().find(|n| !src_nums.contains(n)) {
        return Err(format!("introduced a number not in the context ({n})"));
    }
    Ok(t)
}

fn numbers(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_ascii_digit())
        .filter(|x| !x.is_empty())
        .map(|x| x.trim_start_matches('0').to_string())
        .collect()
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(n).collect::<String>())
    }
}

#[allow(clippy::too_many_arguments)]
async fn record(
    sim: &Arc<Mutex<Sim>>,
    agent: &str,
    task: &str,
    minute: i64,
    ctx: &str,
    prompt: &str,
    reply: Option<&Reply>,
    status: &str,
    output: &str,
) -> i64 {
    let mut sim = sim.lock().await;
    let id = sim.next_id("lv_llm_calls");
    let (it, ot, ne) = reply
        .map(|r| (r.input_tokens, r.output_tokens, r.neurons))
        .unwrap_or((0, 0, 0.0));
    let sql = format!(
        "INSERT INTO lv_llm_calls VALUES ({id}, {minute}, {}, {}, {}, {}, {}, {it}, {ot}, {ne:.3}, {}, {})",
        q(agent),
        q(task),
        q(MODEL),
        q(ctx),
        q(prompt),
        q(status),
        q(output)
    );
    sim.sql_pub(agent, &format!("record model call ({task})"), &sql)
        .await;
    id
}

async fn phrase(
    gw: &Gateway,
    sim: &Arc<Mutex<Sim>>,
    store: &Arc<Store>,
    agent: &str,
    text: &str,
    minute: i64,
) -> Outcome {
    let world = { sim.lock().await.w.clone() };
    let situation = crate::context::Situation::from_world(&world, agent, minute);
    let packet = recall(
        store,
        &world,
        agent,
        minute,
        RecallIntent {
            task: "phrase".into(),
            purpose: "bounded tone context for one already-chosen line".into(),
            predicate: "1 = 1".into(),
            max_items: 3,
        },
    )
    .await
    .unwrap_or_else(|_| {
        crate::context::ContextPacket::empty(
            situation,
            "phrase".into(),
            "bounded tone context".into(),
            3,
        )
    });
    let name = packet.situation.name.clone();
    let title = packet.situation.title.clone();
    let system = format!(
        "You voice {name} ({title}), a fictional character in a research simulation. Rewrite the line so it sounds like {name}. Keep its meaning. The memories are background for tone only: do not mention anything that is not already in the line. Add no new facts, names, objects or numbers. At most {PHRASE_WORDS} words. Reply with the line only."
    );
    let user = format!(
        "Situation: {} at {} ({})\nLine: \"{text}\"\nWhat {name} can recall:\n{}",
        packet.situation.place,
        packet.situation.activity,
        packet.situation.time_zone,
        packet.empty_items_text()
    );
    let prompt = format!("{system}\n{user}");
    let ctx = packet.ids();
    match call_async(gw, "phrase", agent, system, user.clone(), 40).await {
        Ok(r) => match clean(&r.text, PHRASE_WORDS, &user) {
            Ok(line) => {
                let cid = record(
                    sim,
                    agent,
                    "phrase",
                    minute,
                    &ctx,
                    &prompt,
                    Some(&r),
                    "accepted",
                    &line,
                )
                .await;
                let mut sim = sim.lock().await;
                sim.voice(agent, text, &line, cid);
                Outcome::Ok {
                    used: r.used,
                    cap: r.cap,
                }
            }
            Err(why) => {
                record(
                    sim,
                    agent,
                    "phrase",
                    minute,
                    &ctx,
                    &prompt,
                    Some(&r),
                    &format!("rejected: {why}"),
                    &r.text,
                )
                .await;
                Outcome::Rejected {
                    used: r.used,
                    cap: r.cap,
                }
            }
        },
        Err(CallErr::Exhausted { used, cap }) => {
            record(
                sim,
                agent,
                "phrase",
                minute,
                &ctx,
                &prompt,
                None,
                "skipped: daily neuron budget reached",
                "",
            )
            .await;
            Outcome::Exhausted { used, cap }
        }
        Err(CallErr::Other(e)) => {
            record(
                sim,
                agent,
                "phrase",
                minute,
                &ctx,
                &prompt,
                None,
                &format!("failed: {}", truncate(&e, 160)),
                "",
            )
            .await;
            Outcome::Failed(e)
        }
    }
}

async fn reflect(
    gw: &Gateway,
    sim: &Arc<Mutex<Sim>>,
    store: &Arc<Store>,
    agent: &str,
    day: i64,
    minute: i64,
) -> Outcome {
    let start = (day - 1) * DAY;
    let world = { sim.lock().await.w.clone() };
    let situation = crate::context::Situation::from_world(&world, agent, minute);
    let packet = recall(
        store,
        &world,
        agent,
        minute,
        RecallIntent {
            task: "reflect".into(),
            purpose: format!("own memories from Day {day} (max 6)"),
            predicate: format!(
                "owner = {} AND sim_minute >= {start} AND sim_minute < {} AND tag <> 'reflection'",
                q(agent),
                start + DAY
            ),
            max_items: 6,
        },
    )
    .await
    .unwrap_or_else(|_| {
        crate::context::ContextPacket::empty(
            situation,
            "reflect".into(),
            format!("own memories from Day {day}"),
            6,
        )
    });
    if packet.items.is_empty() {
        return Outcome::Skipped;
    }
    let name = packet.situation.name.clone();
    let title = packet.situation.title.clone();
    let system = format!(
        "You write {name}'s private nightly journal. {name} is {title}, a fictional character in a research simulation. From today's memories write ONE lesson {name} will keep for future days. First person. Use only facts in the memories. At most {REFLECT_WORDS} words. Reply with the sentence only."
    );
    let user = format!(
        "Situation: {} at {}\nDay {day} memories:\n{}",
        packet.situation.place,
        packet.situation.activity,
        packet.bullets()
    );
    let prompt = format!("{system}\n{user}");
    let ctx = packet.ids();
    let source_refs: Vec<String> = packet
        .items
        .iter()
        .map(|item| format!("mem:{}", item.id))
        .collect();
    match call_async(gw, "reflect", agent, system, user.clone(), 50).await {
        Ok(r) => match clean(&r.text, REFLECT_WORDS, &user) {
            Ok(line) => {
                let cid = record(
                    sim,
                    agent,
                    "reflect",
                    minute,
                    &ctx,
                    &prompt,
                    Some(&r),
                    "accepted",
                    &line,
                )
                .await;
                let mut sim = sim.lock().await;
                sim.memory(
                    agent,
                    "permanent",
                    "private",
                    "",
                    "reflection",
                    &format!("day:{day}"),
                    -1,
                    ("llm", cid),
                    &line,
                )
                .await;
                sim.lesson(
                    &format!("lesson:{agent}:reflection:day:{day}"),
                    agent,
                    "reflection",
                    "a bounded nightly reflection was accepted as text",
                    &line,
                    "candidate",
                    55,
                    &source_refs,
                    "reflection.v1",
                )
                .await;
                Outcome::Ok {
                    used: r.used,
                    cap: r.cap,
                }
            }
            Err(why) => {
                record(
                    sim,
                    agent,
                    "reflect",
                    minute,
                    &ctx,
                    &prompt,
                    Some(&r),
                    &format!("rejected: {why}"),
                    &r.text,
                )
                .await;
                Outcome::Rejected {
                    used: r.used,
                    cap: r.cap,
                }
            }
        },
        Err(CallErr::Exhausted { used, cap }) => {
            record(
                sim,
                agent,
                "reflect",
                minute,
                &ctx,
                &prompt,
                None,
                "skipped: daily neuron budget reached",
                "",
            )
            .await;
            Outcome::Exhausted { used, cap }
        }
        Err(CallErr::Other(e)) => {
            record(
                sim,
                agent,
                "reflect",
                minute,
                &ctx,
                &prompt,
                None,
                &format!("failed: {}", truncate(&e, 160)),
                "",
            )
            .await;
            Outcome::Failed(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::clean;

    #[test]
    fn keeps_short_plain_lines() {
        assert_eq!(
            clean("\"Table 2 is yours, Sam!\"", 18, "Table 2 is held").unwrap(),
            "Table 2 is yours, Sam!"
        );
        assert_eq!(
            clean("Maya: See you soon!", 18, "").unwrap(),
            "See you soon!"
        );
    }

    #[test]
    fn rejects_new_numbers_long_text_and_assistant_talk() {
        assert!(clean("Your table is ready at 13:30", 18, "Table 2 at 12:45").is_err());
        assert!(clean(&"word ".repeat(30), 18, "").is_err());
        assert!(clean("As an AI I cannot do that", 18, "").is_err());
        assert!(clean("Hi 😀", 18, "").is_err());
    }
}

#[cfg(test)]
mod clean_tests {
    use super::clean;
    #[test]
    fn clean_normalises_dashes() {
        let out = clean(
            "Morning Lin, your usual\u{2019}s ready \u{2013} oat latte.",
            30,
            "Morning Lin, your usual is ready",
        )
        .unwrap();
        assert_eq!(out, "Morning Lin, your usual's ready  -  oat latte.");
    }
}
