//! Memory on disk ([design](../../../../docs/design/bot-memory.md#memory-is-files)): the folders, the scan that
//! turns what changed in them into versions, and the host's own compare-and-swap writes. Which bot's turn a change
//! is credited to is `turns`. The rules (names, checks, who wrote each fact, what a bot may change, the index) are
//! `codync_mem::files`.

use crate::LockExt;
use crate::hub::Hub;
use crate::store::{BotConfig, Store};
use anyhow::{Context, Result, anyhow, bail};
use codync_mem::files::{self as mf, Author, Cause, Change, Version};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

/// A file changed more recently than this may still be mid-write: the next scan takes it.
pub(super) const SETTLE_MS: i64 = 2_000;
/// How long a file's earlier versions are kept, and with them what a bot forgot.
const KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// A layer's folder: the shared one (`None`), or a bot's own.
pub fn folder(bot: Option<&str>) -> PathBuf {
    let data = crate::service::data_dir();
    match bot {
        Some(id) => data.join("bots").join(id).join("bot-memory"),
        None => data.join("shared-memory"),
    }
}

/// The layers a bot sees: its own, and the shared one when it uses it.
pub fn layers(cfg: &BotConfig) -> Vec<Option<String>> {
    let mut layers = vec![Some(cfg.id.clone())];
    if super::shared(cfg) {
        layers.push(None);
    }
    layers
}

/// The folders a bot sees, created so an agent can be given them.
pub fn folders(cfg: &BotConfig) -> Vec<PathBuf> {
    layers(cfg)
        .iter()
        .map(|layer| {
            let dir = folder(layer.as_deref());
            if let Err(error) = std::fs::create_dir_all(&dir) {
                tracing::warn!(dir = %dir.display(), error = %error, "couldn't create a memory folder");
            }
            dir
        })
        .collect()
}

/// The index of the notes a bot sees, for its opening.
pub fn index(store: &Store, cfg: &BotConfig) -> String {
    match store.memory_read(|c| mf::index(c, &cfg.id, super::shared(cfg))) {
        Ok(index) => index.render(|bot| folder(bot).display().to_string()),
        Err(error) => {
            tracing::warn!(bot = %cfg.id, error = format!("{error:#}"), "couldn't read the memory files");
            String::new()
        }
    }
}

/// One scan or host write at a time, so two can't record the same change, nor two writes pass the same
/// compare-and-swap.
// ponytail: one lock for every folder; per-folder locks if scans ever queue behind each other.
static SCANNING: Mutex<()> = Mutex::new(());

/// Records what changed in the folders a bot sees since the last scan. Returns whether a file was too fresh to
/// take yet.
pub fn scan(hub: &Hub, cfg: &BotConfig) -> bool {
    let _one = SCANNING.locked();
    prune(&hub.store);
    let mut deferred = false;
    for layer in layers(cfg) {
        match scan_layer(hub, layer.as_deref()) {
            Ok(fresh) => deferred |= fresh,
            Err(error) => tracing::warn!(bot = %cfg.id, error = format!("{error:#}"), "couldn't scan the memory files"),
        }
    }
    deferred
}

/// Once a day: versions older than [`KEEP_MS`] go.
fn prune(store: &Store) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if store.kv_get("memory.pruned").as_deref() == Some(today.as_str()) {
        return;
    }
    match store.memory_write(|c, now, _| mf::prune(c, now - KEEP_MS)) {
        Ok(()) => {
            let _ = store.kv_set("memory.pruned", &today);
        }
        Err(error) => tracing::warn!(error = format!("{error:#}"), "couldn't prune old memory versions"),
    }
}

/// What is on disk for one file.
struct OnDisk {
    content: String,
    mtime: i64,
    size: u64,
}

/// Scans one folder; true when a file was too fresh to take yet.
fn scan_layer(hub: &Hub, layer: Option<&str>) -> Result<bool> {
    let (mut items, unread) = listing(&folder(layer));
    let mut deferred = false;
    let recorded: HashMap<String, mf::File> =
        hub.store.memory_read(|c| mf::layer(c, layer))?.into_iter().map(|f| (f.name.clone(), f)).collect();
    let now = crate::store::now_ms();
    let mut present = HashSet::new();
    // MEMORY.md last: a fact moved from it into a new note is then found in the note, not taken as forgotten.
    items.sort_by_key(|(name, _)| name == mf::MAIN);
    for (name, path) in items {
        let Some((mtime, size)) = regular(&path, &name) else { continue };
        present.insert(name.clone());
        let known = recorded.get(&name);
        if known.is_some_and(|f| !f.deleted && f.mtime == mtime && f.size == size) {
            continue;
        }
        // A time in the future isn't waited for: it would outlast the turn that set it.
        if (0..SETTLE_MS).contains(&(now - mtime)) {
            deferred = true;
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                tracing::warn!(file = %name, error = %error, "couldn't read a memory file");
                continue;
            }
        };
        if known.is_some_and(|f| f.hash == mf::hash(&content)) {
            hub.store.memory_write(|c, _, _| mf::observed(c, layer, &name, mtime, size))?;
            continue;
        }
        found(hub, layer, &name, Some(&OnDisk { content, mtime, size }))?;
    }
    let gone = |f: &&mf::File| {
        !f.deleted && !present.contains(&f.name) && !unread.iter().any(|prefix| f.name.starts_with(prefix.as_str()))
    };
    for name in recorded.values().filter(gone).map(|f| &f.name) {
        found(hub, layer, name, None)?;
    }
    Ok(deferred)
}

/// Every entry of a layer's folder and its topic folders, named from the layer's folder (`health/allergies.md`),
/// and the folders that couldn't be read whole (`health/`, or `""` for the layer's own): a folder that went missing
/// or can't be read now is not every note in it deleted. Only folders a note may sit in are entered: never a link
/// (a `DirEntry`'s type doesn't follow one), a hidden one or one too deep.
fn listing(dir: &Path) -> (Vec<(String, PathBuf)>, Vec<String>) {
    let (mut found, mut unread) = (Vec::new(), Vec::new());
    let mut folders = vec![(String::new(), dir.to_path_buf())];
    while let Some((prefix, dir)) = folders.pop() {
        let read = std::fs::read_dir(&dir).and_then(|items| {
            for item in items {
                let item = item?;
                let Some(name) = item.file_name().to_str().map(|n| format!("{prefix}{n}")) else { continue };
                if item.file_type()?.is_dir() {
                    if mf::valid_folder(&name) {
                        folders.push((format!("{name}/"), item.path()));
                    }
                    continue;
                }
                found.push((name, item.path()));
            }
            Ok(())
        });
        if let Err(error) = read {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(dir = %dir.display(), error = %error, "couldn't read a memory folder");
            }
            unread.push(prefix);
        }
    }
    (found, unread)
}

/// A memory file's path, when each topic folder on the way is a real folder or not there yet: never a link, so
/// nothing can make the host read or write a file outside the memory folders.
fn path_of(layer: Option<&str>, name: &str) -> Result<PathBuf> {
    let mut path = folder(layer);
    let mut parts = name.split('/').peekable();
    while let Some(part) = parts.next() {
        path.push(part);
        if parts.peek().is_some() && std::fs::symlink_metadata(&path).is_ok_and(|meta| !meta.is_dir()) {
            bail!("a topic folder of {name} is a link");
        }
    }
    Ok(path)
}

/// A regular `.md` file with a valid name: its modification time and size. Links, hidden files and folders are
/// left alone, so nothing can make the host read, journal or restore another file.
fn regular(path: &Path, name: &str) -> Option<(i64, u64)> {
    if !mf::valid_name(name) {
        return None;
    }
    let meta = std::fs::symlink_metadata(path).ok().filter(std::fs::Metadata::is_file)?;
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis();
    Some((i64::try_from(mtime).ok()?, meta.len()))
}

/// A change the scan found: made in a bot's turn, checked, and put back when it may not stand; otherwise just
/// recorded as the computer's.
fn found(hub: &Hub, layer: Option<&str>, name: &str, after: Option<&OnDisk>) -> Result<()> {
    let before = hub.store.memory_read(|c| mf::current(c, layer, name))?;
    let (mtime, size) = after.map_or((0, 0), |a| (a.mtime, a.size));
    let content = after.map(|a| a.content.as_str());
    let change = Change {
        bot: layer,
        name,
        content,
        author: Author::Computer,
        author_bot: None,
        cause: if content.is_some() { Cause::Edit } else { Cause::Delete },
        message: None,
        mtime,
        size,
    };
    let Some((cfg, turn)) = super::turns::turn_of(hub, layer, name, after.map(|a| a.mtime))? else {
        record(hub, &change)?;
        return Ok(());
    };
    let asked = turn.message.is_some();
    if let Err(error) = admit(hub, layer, name, before.as_deref(), content, asked) {
        // Put back what was there, as it was recorded, and tell the bot why.
        let path = path_of(layer, name)?;
        put(&path, before.as_deref())?;
        let (mtime, size) = regular(&path, name).unwrap_or_default();
        if before.is_some() {
            hub.store.memory_write(|c, _, _| mf::observed(c, layer, name, mtime, size))?;
        }
        tracing::info!(bot = %cfg.id, file = name, reason = %error, "put back a bot's memory change");
        super::put_back(&hub.store, &cfg.id, &turn.session, name, &error.to_string());
        return Ok(());
    }
    let change = Change { author: Author::Bot, author_bot: Some(&cfg.id), message: turn.message.as_deref(), ..change };
    record_bot_change(hub, &cfg, Some(&turn.session), before.as_deref(), &change, asked)?;
    Ok(())
}

/// Whether a bot's change to a file may stand: what it brings in may be memory, and a bot may change it outside a
/// turn the user started in its own chat (`asked`).
fn admit(
    hub: &Hub,
    layer: Option<&str>,
    name: &str,
    before: Option<&str>,
    after: Option<&str>,
    asked: bool,
) -> Result<()> {
    if let Some(content) = after {
        mf::check_change(layer, name, before, content, asked)?;
    }
    hub.store.memory_read(|c| mf::admit(c, layer, before, after, asked))?;
    Ok(())
}

/// Records a bot's change, with what it had forgotten when the user asked for it, and tells the other sessions;
/// the one it was made in (`session`) already knows.
fn record_bot_change(
    hub: &Hub,
    cfg: &BotConfig,
    session: Option<&str>,
    before: Option<&str>,
    change: &Change<'_>,
    asked: bool,
) -> Result<Version> {
    let heard = super::current_revision(&hub.store, &cfg.id);
    let version = hub.store.memory_write(|c, now, rev| {
        mf::settle(c, change.bot, change.name, before, change.content, asked)?;
        mf::record(c, change, now, rev)
    })?;
    super::changed(hub, change.bot);
    if let Some(session) = session
        && let Err(error) = super::heard_own_write(&hub.store, &cfg.id, session, &heard)
    {
        tracing::warn!(bot = %cfg.id, error = format!("{error:#}"), "couldn't record the bot's own edit");
    }
    Ok(version)
}

/// The file's content on disk now, `None` when there is none. Refuses names the folders don't hold.
pub fn read(layer: Option<&str>, name: &str) -> Result<Option<String>> {
    if !mf::valid_name(name) {
        bail!(
            "Memory files are MEMORY.md, or notes named in lowercase letters, digits and dashes, ending in .md, in up to {} topic folders (like people.md or health/allergies.md).",
            mf::DEPTH_MAX
        );
    }
    let Some(path) = path_of(layer, name).ok().filter(|path| regular(path, name).is_some()) else { return Ok(None) };
    std::fs::read_to_string(&path).map(Some).with_context(|| format!("reading {name}"))
}

/// Who a host write is for.
#[derive(Clone, Copy)]
pub struct Writer<'a> {
    pub author: Author,
    pub bot: Option<&'a BotConfig>,
    pub cause: Cause,
    /// A bot's write: the user's message that started the turn, when it was one the user started in the bot's own
    /// chat. Only then may it change or take out what memory says.
    pub message: Option<&'a str>,
}

/// Writes a file the way every host writer does: only over the content whose hash is `expected` (`None`: no file
/// yet), through a temporary file in the same folder renamed over it, or by deleting it (`content` `None`). A file
/// that changed meanwhile isn't overwritten: its change is recorded and the write fails. A bot's write passes the
/// checks and the rules on what it may change first.
pub fn write(
    hub: &Hub,
    layer: Option<&str>,
    name: &str,
    content: Option<&str>,
    expected: Option<&str>,
    writer: Writer<'_>,
) -> Result<Version> {
    // Held until the change is recorded, so another writer can't pass the same compare-and-swap meanwhile.
    let _one = SCANNING.locked();
    let now = read(layer, name)?;
    if now.as_deref().map(mf::hash).as_deref() != expected {
        // The other change is recorded, so it isn't lost.
        scan_layer(hub, layer)?;
        bail!("{name} changed meanwhile. Read it again and redo the change on top of it.");
    }
    let asked = writer.message.is_some();
    if writer.author == Author::Bot {
        admit(hub, layer, name, now.as_deref(), content, asked)?;
    }
    if now.as_deref() == content {
        return hub
            .store
            .memory_read(|c| {
                let file = mf::get(c, layer, name)?.ok_or(codync_mem::store::Error::NotFound)?;
                mf::version(c, file.version)?.ok_or(codync_mem::store::Error::NotFound)
            })
            .context("the file has no recorded version");
    }
    let path = path_of(layer, name)?;
    put(&path, content)?;
    let (mtime, size) = regular(&path, name).unwrap_or_default();
    let change = Change {
        bot: layer,
        name,
        content,
        author: writer.author,
        author_bot: writer.bot.map(|cfg| cfg.id.as_str()),
        cause: writer.cause,
        message: writer.message,
        mtime,
        size,
    };
    match writer.bot {
        Some(cfg) if writer.author == Author::Bot => {
            let session = hub.runtime(&cfg.id).session;
            record_bot_change(hub, cfg, session.as_deref(), now.as_deref(), &change, asked)
        }
        _ => record(hub, &change),
    }
}

/// Records a change no rule on a bot's changes applies to, and tells the apps and sessions.
fn record(hub: &Hub, change: &Change<'_>) -> Result<Version> {
    let version = hub.store.memory_write(|c, now, rev| mf::record(c, change, now, rev))?;
    super::changed(hub, change.bot);
    Ok(version)
}

/// Replaces or deletes a file on disk at a [`path_of`] path, its missing folders made first: a temporary sibling
/// renamed over it, so a reader never sees half of it.
fn put(path: &Path, content: Option<&str>) -> Result<()> {
    let Some(content) = content else {
        return match std::fs::remove_file(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        };
    };
    let dir = path.parent().ok_or_else(|| anyhow!("a memory file has a folder"))?;
    std::fs::create_dir_all(dir).context("creating the memory folder")?;
    let temp = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&temp, content).context("writing the memory file")?;
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(error).context("replacing the memory file");
    }
    Ok(())
}

/// A deleted bot's folder.
pub fn remove_bot(id: &str) {
    if let Err(error) = std::fs::remove_dir_all(folder(Some(id)))
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(error = %error, "couldn't remove a deleted bot's memory files");
    }
}
