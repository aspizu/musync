use std::{
    fs::{self, File},
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command},
};

use colored::Colorize;
use fxhash::{FxHashMap, FxHashSet};
use sha2::{Digest, Sha512};
use smol_str::SmolStr;
use walkdir::WalkDir;

const EXTENSIONS_TO_CONVERT: [&str; 7] =
    ["aiff", "flac", "flac", "ogg", "mod", "xm", "m4a"];
const STATE_FILE: &str = ".musync";
const MAX_JOBS: usize = 16;

/// Return an iterator to the Reader of the lines of the file.
fn read_lines<P>(path: P) -> io::Result<io::Lines<io::BufReader<File>>>
where P: AsRef<Path> {
    let file = File::open(path)?;
    Ok(io::BufReader::new(file).lines())
}

/// Read a 2-column table into a fxhash map. n is the length of the first column.
/// If the file does not exist, an empty map is returned.
fn read_table<P>(path: P, n: usize) -> io::Result<FxHashMap<SmolStr, SmolStr>>
where P: AsRef<Path> {
    let mut table: FxHashMap<SmolStr, SmolStr> = FxHashMap::default();
    if let Ok(lines) = read_lines(path) {
        for line in lines {
            let line = line?;
            if line.len() < n {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "line too short",
                ));
            }
            let hash = line[0..n].into();
            let path = line[n..].into();
            table.insert(hash, path);
        }
    }
    Ok(table)
}

/// Write a 2-column table into a file. Does not include any separator.
fn write_table<P>(path: P, table: &FxHashMap<SmolStr, SmolStr>) -> io::Result<()>
where P: AsRef<Path> {
    let mut file = File::create(path)?;
    for (key, value) in table {
        writeln!(file, "{}{}", key, value)?;
    }
    Ok(())
}

/// Flatten deep directory structure.
fn undepthify(path: impl AsRef<Path>) -> PathBuf {
    let mut components = path.as_ref().components();
    if let (Some(first), Some(last)) = (components.next(), components.last()) {
        PathBuf::from_iter([first, last])
    } else {
        path.as_ref().to_owned()
    }
}

/// Remove empty directories recursively.
fn remove_empty_directories<P>(path: P) -> io::Result<()>
where P: AsRef<Path> {
    Command::new("find")
        .arg(path.as_ref())
        .arg("-type")
        .arg("d")
        .arg("-empty")
        .arg("-delete")
        .spawn()?
        .wait()?;
    Ok(())
}

fn hash_file<P>(path: P, hasher: &mut Sha512) -> io::Result<SmolStr>
where P: AsRef<Path> {
    let mut file = File::open(path)?;
    // Only compute the hash of the first x bytes to make it faster.
    // If hash collisions are detected, tune this.
    let mut buffer = [0; 1048576];
    let n = file.read(&mut buffer)?;
    hasher.update(&buffer[..n]);
    Ok(format!("{:x}", hasher.finalize_reset()).into())
}

fn remove_non_existent_files<P>(dir: P, files: &FxHashSet<SmolStr>) -> io::Result<()>
where P: AsRef<Path> {
    for entry in WalkDir::new(&dir) {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let path = entry.path();
        let relative = path.strip_prefix(&dir).unwrap();
        if relative.to_str().unwrap() == STATE_FILE {
            continue;
        }
        if !files.contains(relative.to_str().unwrap()) {
            eprintln!("[{}] {}", "REMOVE".bold().red(), relative.display());
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn add_new_files<P>(
    src: P,
    dst: P,
    prev_state: FxHashMap<SmolStr, SmolStr>,
    new_state: &mut FxHashMap<SmolStr, SmolStr>,
    files: &mut FxHashSet<SmolStr>,
    to_convert: &mut Vec<(PathBuf, PathBuf)>,
) -> io::Result<()>
where
    P: AsRef<Path>,
{
    let mut hasher: Sha512 = Default::default();
    for entry in WalkDir::new(&src) {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let path = entry.path();
        let Some(ext) = path.extension() else { continue };
        let ext = ext.to_str().unwrap();
        let should_convert = EXTENSIONS_TO_CONVERT.contains(&ext);
        if !(should_convert || ext == "mp3") {
            continue;
        }
        let hash = hash_file(path, &mut hasher)?;
        let relative =
            undepthify(path.strip_prefix(&src).unwrap()).with_extension("mp3");
        let entry_dst = dst.as_ref().join(&relative);
        if let Some(prev_path) = prev_state.get(&hash) {
            if prev_path != relative.to_str().unwrap() {
                eprintln!("[{}] {}", "RENAME".bold().blue(), relative.display());
                fs::create_dir_all(entry_dst.parent().unwrap())?;
                fs::rename(dst.as_ref().join(prev_path.as_str()), entry_dst)?;
            }
        } else {
            fs::create_dir_all(entry_dst.parent().unwrap())?;
            if should_convert {
                to_convert.push((path.to_owned(), entry_dst));
            } else {
                eprintln!("[{}] {}", "COPY".bold().green(), relative.display());
                fs::copy(path, entry_dst)?;
            }
        }
        let new_path = SmolStr::from(relative.to_str().unwrap());
        if new_state.contains_key(&hash) {
            eprintln!("[{}] {}", "HASH COLLISION".bold().red(), relative.display());
        }
        new_state.insert(hash, new_path.clone());
        files.insert(new_path);
    }
    Ok(())
}

fn convert_files(to_convert: &[(PathBuf, PathBuf)]) -> io::Result<()> {
    let mut jobs: Vec<Child> = Vec::with_capacity(MAX_JOBS);
    for (src, dst) in to_convert {
        if jobs.len() >= MAX_JOBS {
            eprintln!(" --- Waiting for jobs --- ");
            for job in &mut jobs {
                job.wait()?;
            }
            jobs.clear();
        }
        eprintln!("[{}] {}", "CONVERT".bold().yellow(), src.display());
        jobs.push(
            Command::new("ffmpeg")
                .arg("-y")
                .arg("-i")
                .arg(src)
                .arg("-ab")
                .arg("256k")
                .arg("-hide_banner")
                .arg("-loglevel")
                .arg("error")
                .arg(dst)
                .spawn()?,
        );
    }
    eprintln!(" --- Waiting for jobs --- ");
    for job in &mut jobs {
        job.wait()?;
    }
    eprintln!(" --- Done --- ");
    Ok(())
}

pub fn musync<P>(src: P, dst: P) -> io::Result<()>
where P: AsRef<Path> {
    let mut new_state: FxHashMap<SmolStr, SmolStr> = Default::default();
    let mut files: FxHashSet<SmolStr> = Default::default();
    let mut to_convert: Vec<(PathBuf, PathBuf)> = Default::default();
    let state_file = dst.as_ref().join(STATE_FILE);
    let prev_state = read_table(&state_file, 128)?;
    add_new_files(&src, &dst, prev_state, &mut new_state, &mut files, &mut to_convert)?;
    convert_files(&to_convert)?;
    remove_non_existent_files(&dst, &files)?;
    write_table(state_file, &new_state)?;
    remove_empty_directories(dst)?;
    Ok(())
}
