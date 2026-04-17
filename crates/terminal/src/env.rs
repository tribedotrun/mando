use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tracing::warn;

const SHELL_ENV_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
pub struct ShellEnvResolver {
    cached: OnceLock<HashMap<String, String>>,
}

impl ShellEnvResolver {
    pub fn new() -> Self {
        Self {
            cached: OnceLock::new(),
        }
    }

    pub fn resolve(
        &self,
        config_env: &HashMap<String, String>,
        terminal_env: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        let mut merged = self.base_env();
        merged.extend(config_env.clone());
        merged.extend(terminal_env.clone());
        merged
    }

    fn base_env(&self) -> HashMap<String, String> {
        self.cached
            .get_or_init(|| match snapshot_login_shell_env() {
                Ok(env) => env,
                Err(err) => {
                    warn!(error = %err, "failed to resolve shell-derived terminal env; falling back to current process env");
                    sanitized_env(std::env::vars())
                }
            })
            .clone()
    }
}

fn snapshot_login_shell_env() -> anyhow::Result<HashMap<String, String>> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let shell_name = Path::new(&shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let flag = if matches!(shell_name, "bash" | "zsh") {
        "-lic"
    } else {
        "-lc"
    };
    let marker = "__MANDO_ENV_BEGIN__";
    let mut cmd = Command::new(&shell);
    cmd.arg(flag)
        .arg(format!("printf '{marker}\\0'; env -0"))
        .env_clear()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for key in [
        "HOME", "USER", "LOGNAME", "PATH", "SHELL", "TMPDIR", "LANG", "LC_ALL",
    ] {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }

    let output = run_probe_command_with_timeout(cmd, SHELL_ENV_PROBE_TIMEOUT)?;
    if !output.status.success() {
        anyhow::bail!("shell env probe exited with status {}", output.status);
    }
    parse_env_output(&output.stdout, marker)
}

fn run_probe_command_with_timeout(
    mut cmd: Command,
    timeout: Duration,
) -> anyhow::Result<std::process::Output> {
    let mut child = cmd.spawn()?;

    // Read stdout/stderr on background threads to avoid pipe-buffer deadlock
    // when the shell env output exceeds the OS pipe buffer (~64 KB).
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stdout_pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = stderr_pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });

    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            let stdout = stdout_thread.join().unwrap_or_default();
            let stderr = stderr_thread.join().unwrap_or_default();
            return Ok(std::process::Output {
                status,
                stdout,
                stderr,
            });
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("shell env probe timed out after {:?}", timeout);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn parse_env_output(stdout: &[u8], marker: &str) -> anyhow::Result<HashMap<String, String>> {
    let marker = format!("{marker}\0").into_bytes();
    let start = find_subsequence(stdout, &marker)
        .ok_or_else(|| anyhow::anyhow!("shell env probe marker not found"))?
        + marker.len();
    let mut env = HashMap::new();
    for entry in stdout[start..].split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let Some(eq_pos) = entry.iter().position(|byte| *byte == b'=') else {
            continue;
        };
        let (key, value) = entry.split_at(eq_pos);
        let value = &value[1..];
        let key = String::from_utf8_lossy(key).to_string();
        let value = String::from_utf8_lossy(value).to_string();
        if !is_runtime_only_key(&key) {
            env.insert(key, value);
        }
    }
    Ok(env)
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn sanitized_env<I>(vars: I) -> HashMap<String, String>
where
    I: IntoIterator<Item = (String, String)>,
{
    vars.into_iter()
        .filter(|(key, _)| !is_runtime_only_key(key))
        .collect()
}

fn is_runtime_only_key(key: &str) -> bool {
    key.starts_with("MANDO_") || key.starts_with("ELECTRON_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_env_output_ignores_noise_before_marker() {
        let stdout = b"noise\n__MANDO_ENV_BEGIN__\0PATH=/bin\0FOO=bar\0";
        let env = parse_env_output(stdout, "__MANDO_ENV_BEGIN__").unwrap();
        assert_eq!(env.get("PATH"), Some(&"/bin".to_string()));
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn sanitized_env_strips_runtime_only_vars() {
        let env = sanitized_env(vec![
            ("PATH".into(), "/bin".into()),
            ("MANDO_PORT".into(), "18791".into()),
            ("ELECTRON_RUN_AS_NODE".into(), "1".into()),
        ]);
        assert_eq!(env.get("PATH"), Some(&"/bin".to_string()));
        assert!(!env.contains_key("MANDO_PORT"));
        assert!(!env.contains_key("ELECTRON_RUN_AS_NODE"));
    }

    #[test]
    fn explicit_terminal_env_wins_over_config_env() {
        let resolver = ShellEnvResolver {
            cached: OnceLock::new(),
        };
        resolver
            .cached
            .set(HashMap::from([
                ("PATH".into(), "/shell".into()),
                ("FOO".into(), "shell".into()),
            ]))
            .unwrap();
        let env = resolver.resolve(
            &HashMap::from([
                ("FOO".into(), "config".into()),
                ("BAR".into(), "config".into()),
            ]),
            &HashMap::from([
                ("FOO".into(), "terminal".into()),
                ("BAZ".into(), "terminal".into()),
            ]),
        );

        assert_eq!(env.get("PATH"), Some(&"/shell".to_string()));
        assert_eq!(env.get("FOO"), Some(&"terminal".to_string()));
        assert_eq!(env.get("BAR"), Some(&"config".to_string()));
        assert_eq!(env.get("BAZ"), Some(&"terminal".to_string()));
    }

    #[test]
    fn shell_env_probe_times_out() {
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-lc").arg("sleep 1");
        let err = run_probe_command_with_timeout(cmd, Duration::from_millis(50)).unwrap_err();
        assert!(err.to_string().contains("timed out"));
    }
}
