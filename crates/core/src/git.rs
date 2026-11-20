//! Rama actual de un proyecto, leída del `HEAD` de git sin lanzar `git`: el tablero la pide en
//! cada refresco y abrir un proceso por proyecto sería caro.

use std::path::{Path, PathBuf};

/// Rama en la que está la carpeta (o la del repositorio que la contiene). Con el `HEAD` separado
/// devuelve el commit abreviado; `None` si no es un repositorio de git.
pub fn current_branch(dir: impl AsRef<Path>) -> Option<String> {
    let git_dir = find_git_dir(dir.as_ref())?;
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    parse_head(&head)
}

/// Sube desde `dir` hasta encontrar `.git`: una carpeta en un repositorio normal o un archivo
/// `gitdir: <ruta>` en un worktree o submódulo.
fn find_git_dir(dir: &Path) -> Option<PathBuf> {
    for base in dir.ancestors() {
        let dot = base.join(".git");
        if dot.is_dir() {
            return Some(dot);
        }
        if dot.is_file() {
            let text = std::fs::read_to_string(&dot).ok()?;
            let target = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
            return Some(if target.is_relative() { base.join(target) } else { target });
        }
    }
    None
}

fn parse_head(head: &str) -> Option<String> {
    let head = head.trim();
    if let Some(r) = head.strip_prefix("ref:") {
        let r = r.trim();
        return Some(r.strip_prefix("refs/heads/").unwrap_or(r).to_string()).filter(|b| !b.is_empty());
    }
    (head.len() >= 7 && head.bytes().all(|b| b.is_ascii_hexdigit())).then(|| head[..7].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_ramas_y_head_separado() {
        assert_eq!(parse_head("ref: refs/heads/main\n").as_deref(), Some("main"));
        assert_eq!(parse_head("ref: refs/heads/claude/fix-login").as_deref(), Some("claude/fix-login"));
        assert_eq!(parse_head("b6888e927148940fe9db1a7afb0b576bb26bf5a4\n").as_deref(), Some("b6888e9"));
        assert_eq!(parse_head(""), None);
        assert_eq!(parse_head("basura"), None);
    }

    #[test]
    fn encuentra_el_repo_desde_una_subcarpeta_y_en_worktrees() {
        let root = std::env::temp_dir().join(format!("rm-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("app/src")).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/develop\n").unwrap();
        assert_eq!(current_branch(&repo).as_deref(), Some("develop"));
        assert_eq!(current_branch(repo.join("app/src")).as_deref(), Some("develop"));

        let wt_git = repo.join(".git/worktrees/feat");
        std::fs::create_dir_all(&wt_git).unwrap();
        std::fs::write(wt_git.join("HEAD"), "ref: refs/heads/feat/x\n").unwrap();
        let wt = root.join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
        assert_eq!(current_branch(&wt).as_deref(), Some("feat/x"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
