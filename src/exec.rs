//! Запуск скриптов в фоновом потоке и оценка прогресса по истории.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command as ShellCommand, Stdio};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};

use crate::model::{Command, ExecutionLog, ParamType, Parameter, new_id, push_output_line};

pub const MASK: &str = "********";

enum RunEvent {
    /// Строка вывода; `true` — она пришла из stderr.
    Line(String, bool),
    Finished(Option<i32>),
}

/// Подставляет значения параметров вместо `${ИМЯ}`.
///
/// Если `mask_secrets`, значения паролей заменяются на [`MASK`] — такой вариант
/// скрипта уходит в историю, чтобы пароли не оседали на диске открытым текстом.
/// Неизвестные имена остаются в тексте как есть: `${HOME}` уедет в оболочку.
pub fn substitute(script: &str, params: &[Parameter], mask_secrets: bool) -> String {
    let value_of = |name: &str| -> Option<String> {
        params.iter().find(|p| p.name == name).map(|p| {
            if mask_secrets && p.param_type == ParamType::Password && !p.value.is_empty() {
                MASK.to_owned()
            } else {
                p.value.clone()
            }
        })
    };

    let chars: Vec<char> = script.chars().collect();
    let mut out = String::with_capacity(script.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$'
            && chars.get(i + 1) == Some(&'{')
            && let Some(close) = (i + 2..chars.len()).find(|&j| chars[j] == '}')
            && let Some(value) = value_of(&chars[i + 2..close].iter().collect::<String>())
        {
            out.push_str(&value);
            i = close + 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Медиана длительностей последних завершённых запусков — база для полосы прогресса.
pub fn estimate_secs(logs: &[ExecutionLog]) -> Option<f64> {
    const WINDOW: usize = 10;
    let mut durations: Vec<f64> = logs
        .iter()
        .rev()
        .filter_map(ExecutionLog::duration_secs)
        .filter(|d| *d > 0.0)
        .take(WINDOW)
        .collect();
    if durations.is_empty() {
        return None;
    }
    durations.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Some(durations[durations.len() / 2])
}

/// Текущий запуск: живое состояние, которое опрашивает UI.
pub struct ActiveRun {
    pub command_id: String,
    pub command_name: String,
    /// Скрипт с подставленными параметрами (пароли замаскированы).
    pub script: String,
    pub params: Vec<Parameter>,
    /// Объединённый вывод обоих потоков в порядке появления строк.
    pub output: String,
    pub exit_code: Option<i32>,
    pub start_time: DateTime<Local>,
    pub end_time: Option<DateTime<Local>>,
    pub finished: bool,
    pub cancelled: bool,
    pub estimate_secs: Option<f64>,
    start_instant: Instant,
    finish_instant: Option<Instant>,
    rx: Receiver<RunEvent>,
    child: Arc<Mutex<Option<Child>>>,
}

impl ActiveRun {
    /// Запускает скрипт команды с переданными значениями параметров.
    pub fn start(command: &Command, params: Vec<Parameter>, estimate_secs: Option<f64>) -> Self {
        let resolved = substitute(&command.script, &params, false);
        let masked = substitute(&command.script, &params, true);

        let (tx, rx) = channel();
        let child = Arc::new(Mutex::new(None));
        let env: Vec<(String, String)> = params
            .iter()
            .filter(|p| !p.name.is_empty())
            .map(|p| (p.name.clone(), p.value.clone()))
            .collect();

        {
            let child = Arc::clone(&child);
            thread::spawn(move || worker(resolved, env, tx, child));
        }

        Self {
            command_id: command.id.clone(),
            command_name: command.name.clone(),
            script: masked,
            params: params
                .into_iter()
                .map(|mut p| {
                    p.value = p.display_value();
                    p
                })
                .collect(),
            output: String::new(),
            exit_code: None,
            start_time: Local::now(),
            end_time: None,
            finished: false,
            cancelled: false,
            estimate_secs,
            start_instant: Instant::now(),
            finish_instant: None,
            rx,
            child,
        }
    }

    /// Забирает накопленный вывод. Возвращает `true`, если что-то изменилось.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.rx.try_recv() {
                Ok(RunEvent::Line(line, is_error)) => {
                    push_output_line(&mut self.output, &line, is_error);
                    changed = true;
                }
                Ok(RunEvent::Finished(code)) => {
                    self.exit_code = code;
                    self.finished = true;
                    self.end_time = Some(Local::now());
                    self.finish_instant = Some(Instant::now());
                    changed = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.finished {
                        self.finished = true;
                        self.end_time = Some(Local::now());
                        self.finish_instant = Some(Instant::now());
                        changed = true;
                    }
                    break;
                }
            }
        }
        changed
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.finish_instant
            .unwrap_or_else(Instant::now)
            .duration_since(self.start_instant)
            .as_secs_f64()
    }

    /// Доля выполнения по оценке из истории; `None` — оценки нет.
    pub fn progress(&self) -> Option<f32> {
        let estimate = self.estimate_secs?;
        if estimate <= 0.0 {
            return None;
        }
        // Пока скрипт не завершился, не показываем 100% — иначе полоса «врёт».
        Some(((self.elapsed_secs() / estimate) as f32).clamp(0.0, 0.99))
    }

    /// Останавливает процесс (кнопка «Остановить»).
    pub fn cancel(&mut self) {
        self.cancelled = true;
        if let Ok(mut guard) = self.child.lock()
            && let Some(child) = guard.as_mut()
        {
            let _ = child.kill();
        }
    }

    pub fn to_log(&self) -> ExecutionLog {
        ExecutionLog {
            id: new_id(),
            command_id: self.command_id.clone(),
            script: self.script.clone(),
            output: self.output.clone(),
            exit_code: self.exit_code,
            start_time: self.start_time,
            end_time: self.end_time,
            params: self.params.clone(),
        }
    }
}

fn worker(
    script: String,
    env: Vec<(String, String)>,
    tx: Sender<RunEvent>,
    child_slot: Arc<Mutex<Option<Child>>>,
) {
    let mut builder = shell_command();
    builder
        .arg(&script)
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dirs) = directories::UserDirs::new() {
        builder.current_dir(dirs.home_dir());
    }

    let mut child = match builder.spawn() {
        Ok(child) => child,
        Err(err) => {
            let _ = tx.send(RunEvent::Line(
                format!("не удалось запустить оболочку: {err}"),
                true,
            ));
            let _ = tx.send(RunEvent::Finished(None));
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    if let Ok(mut slot) = child_slot.lock() {
        *slot = Some(child);
    }

    let readers = [
        stdout.map(|pipe| spawn_reader(pipe, tx.clone(), false)),
        stderr.map(|pipe| spawn_reader(pipe, tx.clone(), true)),
    ];
    for reader in readers.into_iter().flatten() {
        let _ = reader.join();
    }

    // Каналы закрыты, но процесс мог ещё не завершиться — ждём коротким опросом,
    // не удерживая мьютекс, чтобы «Остановить» могло сработать в любой момент.
    let code = loop {
        let status = child_slot
            .lock()
            .ok()
            .and_then(|mut slot| slot.as_mut().map(Child::try_wait));
        match status {
            Some(Ok(Some(status))) => break status.code(),
            Some(Ok(None)) => thread::sleep(Duration::from_millis(30)),
            _ => break None,
        }
    };

    if let Ok(mut slot) = child_slot.lock() {
        *slot = None;
    }
    let _ = tx.send(RunEvent::Finished(code));
}

/// Оба потока пишут в один канал: порядок строк в объединённом выводе —
/// это порядок, в котором они пришли от процесса.
fn spawn_reader<R: Read + Send + 'static>(
    pipe: R,
    tx: Sender<RunEvent>,
    is_error: bool,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(pipe).lines() {
            let (line, is_error) = match line {
                Ok(line) => (line, is_error),
                Err(err) => (format!("<ошибка чтения вывода: {err}>"), true),
            };
            if tx.send(RunEvent::Line(line, is_error)).is_err() {
                break;
            }
        }
    })
}

#[cfg(unix)]
fn shell_command() -> ShellCommand {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
    let mut cmd = ShellCommand::new(shell);
    cmd.arg("-c");
    cmd
}

#[cfg(windows)]
fn shell_command() -> ShellCommand {
    let mut cmd = ShellCommand::new("cmd");
    cmd.arg("/C");
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(name: &str, value: &str, param_type: ParamType) -> Parameter {
        Parameter {
            name: name.to_owned(),
            value: value.to_owned(),
            param_type,
        }
    }

    #[test]
    fn substitutes_braced_names_only() {
        let params = vec![param("ENV", "prod", ParamType::String)];
        assert_eq!(
            substitute("deploy ${ENV} to $ENV", &params, false),
            "deploy prod to $ENV"
        );
    }

    #[test]
    fn leaves_unknown_names_to_the_shell() {
        let params = vec![param("FOO", "1", ParamType::String)];
        assert_eq!(
            substitute("${HOME} ${FOOBAR} ${FOO} ${", &params, false),
            "${HOME} ${FOOBAR} 1 ${"
        );
    }

    #[test]
    fn masks_passwords_only_when_asked() {
        let params = vec![param("PWD", "s3cret", ParamType::Password)];
        assert_eq!(substitute("login ${PWD}", &params, false), "login s3cret");
        assert_eq!(substitute("login ${PWD}", &params, true), format!("login {MASK}"));
    }

    #[test]
    fn estimate_is_median_of_completed_runs() {
        let make = |secs: i64| ExecutionLog {
            id: new_id(),
            command_id: "c".to_owned(),
            script: String::new(),
            output: String::new(),
            exit_code: Some(0),
            start_time: Local::now(),
            end_time: Some(Local::now() + chrono::Duration::seconds(secs)),
            params: Vec::new(),
        };
        let logs = vec![make(2), make(10), make(6)];
        assert_eq!(estimate_secs(&logs), Some(6.0));
        assert_eq!(estimate_secs(&[]), None);
    }
}
