use std::{env, ffi::OsString, fs, io, path::Path};

use tokio::process::Command;

const ZSH_ENV: &str = include_str!("../../shell-integration/zsh/.zshenv");
const ZSH_MARKS: &str = include_str!("../../shell-integration/zsh/kodosi.zsh");
const BASH_MARKS: &str = include_str!("../../shell-integration/bash/kodosi.bash");

/// Starts a login zsh or bash with Kodosi's prompt marks (OSC 133), as Ghostty
/// starts its shells with its shell integration. The shell still reads the
/// user's startup files. A shell whose scripts cannot be written starts
/// without the marks.
pub(super) fn add(
    shell: &str,
    program: &Path,
    folder: &Path,
    environment: &[(&str, &str)],
    command: &mut Command,
) {
    match shell {
        "zsh" => add_to_zsh(&folder.join("zsh"), environment, command),
        "bash" if !is_macos_system_bash(program) => {
            add_to_bash(&folder.join("bash"), environment, command);
        }
        _ => {}
    }
}

fn add_to_zsh(folder: &Path, environment: &[(&str, &str)], command: &mut Command) {
    if write_scripts(folder, &[(".zshenv", ZSH_ENV), ("kodosi.zsh", ZSH_MARKS)]).is_err() {
        return;
    }
    if let Some(zdotdir) = variable(environment, "ZDOTDIR") {
        command.env("KODOSI_ZSH_ZDOTDIR", zdotdir);
    }
    command.env("ZDOTDIR", folder);
}

fn add_to_bash(folder: &Path, environment: &[(&str, &str)], command: &mut Command) {
    if write_scripts(folder, &[("kodosi.bash", BASH_MARKS)]).is_err() {
        return;
    }
    command.arg("--posix");
    if let Some(user_env) = variable(environment, "ENV") {
        command.env("KODOSI_BASH_ENV", user_env);
    }
    command.env("ENV", folder.join("kodosi.bash"));
    command.env("KODOSI_BASH_INJECT", "1");
    if variable(environment, "HISTFILE").is_none()
        && let Some(home) = variable(environment, "HOME")
    {
        command.env("HISTFILE", Path::new(&home).join(".bash_history"));
        command.env("KODOSI_BASH_UNEXPORT_HISTFILE", "1");
    }
}

fn variable(environment: &[(&str, &str)], name: &str) -> Option<OsString> {
    environment
        .iter()
        .rev()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| OsString::from(value))
        .or_else(|| env::var_os(name))
}

/// The bash 3.2 of macOS has no PS0, so it keeps starting as before.
fn is_macos_system_bash(program: &Path) -> bool {
    cfg!(target_os = "macos") && program == Path::new("/bin/bash")
}

fn write_scripts(folder: &Path, scripts: &[(&str, &str)]) -> io::Result<()> {
    fs::create_dir_all(folder)?;
    for (name, contents) in scripts {
        let path = folder.join(name);
        if fs::read(&path).is_ok_and(|current| current == contents.as_bytes()) {
            continue;
        }
        let staged = folder.join(format!("{name}.{}.new", std::process::id()));
        fs::write(&staged, contents)?;
        fs::rename(&staged, &path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Duration};

    use super::*;
    use crate::{KodosiPty, ShutdownStage};

    fn shell(path: &str) -> Option<PathBuf> {
        let path = PathBuf::from(path);
        path.exists().then_some(path)
    }

    async fn run(
        program: &Path,
        environment: &[(&str, &str)],
        folder: &Path,
        lines: &[&[u8]],
        done: &str,
    ) -> String {
        let (mut pty, mut reader) =
            KodosiPty::spawn_program(program, &[], None, 24, 80, environment, Some(folder))
                .unwrap();
        let mut output = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let mut pending = lines.iter();
        let mut prompts = 0;
        while !String::from_utf8_lossy(&output).contains(done) {
            let seen = String::from_utf8_lossy(&output)
                .matches("\x1b]133;B\x07")
                .count();
            if seen > prompts {
                prompts = seen;
                if let Some(line) = pending.next() {
                    pty.write(line).unwrap();
                }
            }
            let mut bytes = [0; 4096];
            let count = tokio::time::timeout_at(deadline, reader.read(&mut bytes))
                .await
                .unwrap_or_else(|_| panic!("no answer: {}", String::from_utf8_lossy(&output)))
                .unwrap();
            assert!(
                count > 0,
                "shell exited: {}",
                String::from_utf8_lossy(&output)
            );
            output.extend_from_slice(&bytes[..count]);
        }
        pty.request_shutdown(ShutdownStage::Force).unwrap();
        pty.wait_within(Duration::from_secs(5)).await.unwrap();
        String::from_utf8_lossy(&output).into_owned()
    }

    fn block_on<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn a_login_zsh_reads_the_users_files_and_marks_each_prompt_and_command() {
        let Some(zsh) = shell("/bin/zsh") else {
            return;
        };
        let home = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        fs::write(home.path().join(".zshenv"), "export FROM_ZSHENV=yes\n").unwrap();
        fs::write(home.path().join(".zshrc"), "PS1='T> '\n").unwrap();
        let home_path = home.path().to_str().unwrap();

        let output = block_on(run(
            &zsh,
            &[("HOME", home_path), ("ZDOTDIR", home_path)],
            folder.path(),
            &[
                b"print -r -- \"probe=$FROM_ZSHENV:$ZDOTDIR:${KODOSI_ZSH_ZDOTDIR-none}\"\n",
                b"false\n",
                b"print -r -- finished\n",
            ],
            "\x1b]133;C\x07finished",
        ));

        assert!(
            output.contains(&format!("probe=yes:{home_path}:none")),
            "{output}"
        );
        assert!(
            output.contains("\x1b]133;A;cl=line\x07T> \x1b]133;B\x07"),
            "{output}"
        );
        assert!(output.contains("\x1b]133;C\x07probe="), "{output}");
        assert!(output.contains("\x1b]133;D;0\x07"), "{output}");
        assert!(output.contains("\x1b]133;D;1\x07"), "{output}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_login_bash_reads_the_users_profile_and_marks_each_prompt_and_command() {
        let Some(bash) = shell("/bin/bash") else {
            return;
        };
        let home = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        fs::write(
            home.path().join(".bash_profile"),
            "export FROM_PROFILE=yes\nPS1='B> '\nPROMPT_COMMAND='history -a;'\n",
        )
        .unwrap();
        let home_path = home.path().to_str().unwrap();

        let output = block_on(run(
            &bash,
            &[("HOME", home_path)],
            folder.path(),
            &[
                b"echo \"probe=$FROM_PROFILE:${ENV-unset}:$(shopt -qo posix && echo on || echo off):$HISTFILE\"\n",
                b"false\n",
                b"echo \"status=$?\"\n",
            ],
            "status=1",
        ));

        assert!(
            output.contains(&format!("probe=yes:unset:off:{home_path}/.bash_history")),
            "{output}"
        );
        assert!(
            output.contains("\x1b]133;A;redraw=last;cl=line\x07"),
            "{output}"
        );
        assert!(output.contains("B> \x1b]133;B\x07"), "{output}");
        assert!(output.contains("\x1b]133;D;1\x07"), "{output}");
    }

    #[test]
    fn other_programs_and_the_macos_bash_start_as_before() {
        let folder = tempfile::tempdir().unwrap();
        for (shell, program) in [("sh", "/bin/sh"), ("fish", "/usr/bin/fish")] {
            let mut command = Command::new(program);
            add(shell, Path::new(program), folder.path(), &[], &mut command);
            assert_eq!(command.as_std().get_envs().count(), 0, "{shell}");
            assert_eq!(command.as_std().get_args().count(), 0, "{shell}");
        }

        let mut bash = Command::new("/bin/bash");
        add(
            "bash",
            Path::new("/bin/bash"),
            folder.path(),
            &[],
            &mut bash,
        );
        assert_eq!(
            bash.as_std().get_args().count(),
            usize::from(!cfg!(target_os = "macos"))
        );
    }
}
