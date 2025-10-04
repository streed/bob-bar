use once_cell::sync::Lazy;
use std::collections::VecDeque;
use std::sync::Mutex;

const MAX_LOG_LINES: usize = 50;

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Info,
    Http,
    Debate,
    Refiner,
    Writer,
    DocumentCritic,
    Combiner,
    Worker,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub text: String,
    pub kind: Kind,
}

static VERBOSE_LOG: Lazy<Mutex<VecDeque<Entry>>> = Lazy::new(|| Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES)));

pub fn log<T: Into<String>>(line: T) {
    log_with(Kind::Info, line);
}

pub fn log_with<T: Into<String>>(kind: Kind, line: T) {
    if let Ok(mut buf) = VERBOSE_LOG.lock() {
        let s = line.into();
        if buf.len() >= MAX_LOG_LINES { buf.pop_front(); }
        buf.push_back(Entry { text: s, kind });
    }
}

pub fn recent(n: usize) -> Vec<Entry> {
    if let Ok(buf) = VERBOSE_LOG.lock() {
        let len = buf.len();
        let take = n.min(len);
        buf.iter().skip(len - take).cloned().collect()
    } else {
        Vec::new()
    }
}

pub fn clear() {
    if let Ok(mut buf) = VERBOSE_LOG.lock() {
        buf.clear();
    }
}
