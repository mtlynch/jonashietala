use crate::{markup::MarkupType, paths::FilePath};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::{DateTime, NaiveDateTime};
use eyre::Result;
use git2::{Oid, Repository};
use lazy_static::lazy_static;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

lazy_static! {
    pub static ref IGNORED_COMMITS: HashSet<&'static str> = [
        // Enclose titles in quotes
        "011cebb16131566a186b1fe7c8ba884890a688d6",
        // Rework tags
        "babfc47475c92b8566fe9cba2eaa38af2376fd09",
        // Minor corrections
        "48a1238cba939b031d68ce9069b1f31a731017e4",
        // Recommended posts
        "67bc27e0804bd0219599727ae50d5976a760d821",
        // Content update after rust rewrite
        "9208bc3505c056335695112f127ca70d0c20b79e",
        // Better image handling
        "80ca420bad3085c5d803df4c78e683f36f22c5c4",
        // Reorganize tags
        "cffe82c533d220840743301b6e2db6917646ebfb",
        // Formatting and other small fixes
        "fd2f5d679d52132aacb603c3aa7752feeaeb1339",
        // Recommended posts
        "de126c069c9356019c9f743998f4022ee9e89a6c",
        // Recommended posts
        "099e3ffca0246c873a37f332dda0d27e0fe9c39a",
        // Favorite system
        "ffcc7fb0a323bd1b4c181aaf8bc2236d0bf34529",
        // Add to favorite
        "dbbd685327e956deae2b3ed7d8431d74c137a6c9",
        // Migrate from ad-hoc inline code language spec to Djot attributes
        "52025a7683b2f882ff800eeb6d4e874cab2f8aa2",
        // Rework tags a little
        "9074ed5f7aa581bd1110555379a3e9cbdd799517",
        // Use `{=rust}` instead of `{lang=rust} for inline highlighting
        "b5310570e82f3b89792a5987214344627b4f8f3b"
    ]
    .into();
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestCommitInfo {
    pub dt: NaiveDateTime,
    pub id: Oid,
    // Revision is true if there's more than one commit for the file.
    pub is_revision: bool,
}

#[derive(Debug, Default)]
pub struct LatestCommits {
    commits: HashMap<Utf8PathBuf, LatestCommitInfo>,
}

impl LatestCommits {
    pub fn new(repo: &Repository, dir_offset: Option<&Utf8Path>) -> Result<Self> {
        let mut commits: HashMap<Utf8PathBuf, LatestCommitInfo> = HashMap::new();

        // There's no native way to get the latest commit of a file...
        // https://github.com/rust-lang/git2-rs/issues/588
        let mut revwalk = repo.revwalk()?;
        revwalk.set_sorting(git2::Sort::TIME)?;
        revwalk.push_head()?;
        for commit_id in revwalk {
            let commit_id = commit_id?;
            let commit = repo.find_commit(commit_id)?;
            // Ignore merge commits (2+ parents) because that's what 'git whatchanged' does.
            // Ignore commit with 0 parents (initial commit) because there's nothing to diff against
            if commit.parent_count() == 1 {
                let prev_commit = commit.parent(0)?;
                let tree = commit.tree()?;
                let prev_tree = prev_commit.tree()?;
                let diff = repo.diff_tree_to_tree(Some(&prev_tree), Some(&tree), None)?;
                for delta in diff.deltas() {
                    let id = commit.id();
                    if IGNORED_COMMITS.contains(id.to_string().as_str()) {
                        continue;
                    }

                    let file_path = Utf8Path::from_path(delta.new_file().path().unwrap())
                        .expect("Non-utf8 path");
                    // Only care about markup files.
                    if let Some(ext) = file_path.extension() {
                        if MarkupType::from_extension(ext).is_none() {
                            continue;
                        }
                    }
                    // If `dir_offset` then transform paths and ignore non-matches
                    let path = match dir_offset {
                        Some(offset) => match file_path.strip_prefix(offset) {
                            Ok(path) => path.to_owned(),
                            Err(_) => continue,
                        },
                        None => file_path.to_owned(),
                    };

                    let file_mod_time = commit.time();
                    let unix_time = file_mod_time.seconds();
                    let dt = DateTime::from_timestamp(unix_time, 0)
                        .unwrap()
                        .naive_local();
                    commits
                        .entry(path)
                        .and_modify(|c| {
                            if dt > c.dt {
                                *c = LatestCommitInfo {
                                    id,
                                    dt,
                                    is_revision: true,
                                };
                            } else {
                                c.is_revision = true
                            }
                        })
                        .or_insert(LatestCommitInfo {
                            id,
                            dt,
                            is_revision: false,
                        });
                }
            }
        }

        Ok(Self { commits })
    }

    pub fn get(&self, path: &FilePath) -> Option<&LatestCommitInfo> {
        self.get_path(&path.rel_path.0)
    }

    pub fn get_path<P: AsRef<Utf8Path>>(&self, path: P) -> Option<&LatestCommitInfo> {
        self.commits.get(path.as_ref())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CommitContext {
    id: String,
    short_id: String,
    dt: String,
    is_revision: bool,
}

impl From<&LatestCommitInfo> for CommitContext {
    fn from(info: &LatestCommitInfo) -> Self {
        let id = info.id.to_string();
        Self {
            short_id: id.chars().take(7).collect(),
            id,
            dt: info.dt.format("%FT%T%.fZ").to_string(),
            is_revision: info.is_revision,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveTime};

    use super::*;

    #[test]
    fn test_git_info() -> Result<()> {
        let repo = Repository::open(".")?;
        let x = LatestCommits::new(&repo, None)?;
        assert!(
            x.get_path("posts/2014-04-27-ores.markdown")
                == Some(&LatestCommitInfo {
                    dt: NaiveDateTime::new(
                        NaiveDate::from_ymd_opt(2014, 4, 27).unwrap(),
                        NaiveTime::from_hms_opt(17, 55, 40).unwrap()
                    ),
                    id: Oid::from_str("b6f61cd4154b23b05b65e37cf9097fa3d3d9113f")?,
                    is_revision: false
                })
        );

        Ok(())
    }
}
