//! Runs an automation step by step on a worker thread, exactly like the
//! desktop app: one persistent shell session, a failed foreground step stops
//! the run, Stop kills the session immediately.

use crate::*;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StepStatus {
    Pending,
    Running,
    Success,
    Error,
    Skipped,
}

#[derive(Debug, Clone)]
pub(crate) struct RunStep {
    pub(crate) label: String,
    pub(crate) status: StepStatus,
    pub(crate) output: String,
}

pub(crate) enum RunEvent {
    StepStarted(usize),
    StepFinished(usize, StepStatus, String, Option<String>),
    SessionFailed(String),
    Done,
}

pub(crate) struct RunState {
    pub(crate) automation_id: String,
    pub(crate) name: String,
    pub(crate) steps: Vec<RunStep>,
    pub(crate) running: bool,
    pub(crate) cancel_requested: bool,
    pub(crate) error: String,
    pub(crate) current: usize,
    pub(crate) started: Instant,
    pub(crate) finished: Option<Instant>,
    pub(crate) selected: usize,
    pub(crate) follow: bool,
    pub(crate) scroll: u16,
    cancel: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
    background_pids: Arc<Mutex<Vec<u32>>>,
    rx: mpsc::Receiver<RunEvent>,
}

pub(crate) fn step_label(step: &AutomationStep) -> String {
    if step.kind == "wait" {
        format!(
            "Wait {} {}",
            step.seconds,
            if step.seconds == 1 {
                "second"
            } else {
                "seconds"
            }
        )
    } else if step.behavior == "background" {
        format!("{}  (background)", step.command)
    } else {
        step.command.clone()
    }
}

impl RunState {
    pub(crate) fn start(automation: &Automation) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let pid = Arc::new(AtomicU32::new(0));
        let background_pids = Arc::new(Mutex::new(Vec::new()));
        let worker_automation = automation.clone();
        let (worker_cancel, worker_pid, worker_background) =
            (cancel.clone(), pid.clone(), background_pids.clone());
        thread::spawn(move || {
            run_worker(
                worker_automation,
                worker_cancel,
                worker_pid,
                worker_background,
                tx,
            )
        });

        Self {
            automation_id: automation.id.clone(),
            name: automation.name.clone(),
            steps: automation
                .steps
                .iter()
                .map(|step| RunStep {
                    label: step_label(step),
                    status: StepStatus::Pending,
                    output: String::new(),
                })
                .collect(),
            running: true,
            cancel_requested: false,
            error: String::new(),
            current: 0,
            started: Instant::now(),
            finished: None,
            selected: 0,
            follow: true,
            scroll: 0,
            cancel,
            pid,
            background_pids,
            rx,
        }
    }

    // Stop ends the run immediately: the command that is running in the
    // foreground is killed together with everything it started, then the
    // session's shell. Jobs a background step started with `&` are left
    // running on purpose, like in the desktop app.
    pub(crate) fn stop(&mut self) {
        if !self.running || self.cancel_requested {
            return;
        }
        self.cancel_requested = true;
        self.cancel.store(true, Ordering::SeqCst);
        let shell = self.pid.load(Ordering::SeqCst);
        if shell == 0 {
            return;
        }
        let keep: Vec<u32> = self
            .background_pids
            .lock()
            .map(|pids| pids.clone())
            .unwrap_or_default();
        for child in child_pids(shell) {
            if !keep.contains(&child) {
                kill_tree(child);
            }
        }
        kill(shell);
    }

    // Applies worker events; returns true when something changed.
    pub(crate) fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.rx.try_recv() {
            changed = true;
            match event {
                RunEvent::StepStarted(index) => {
                    self.current = index;
                    if let Some(step) = self.steps.get_mut(index) {
                        step.status = StepStatus::Running;
                    }
                    if self.follow {
                        self.selected = index;
                        self.scroll = 0;
                    }
                }
                RunEvent::StepFinished(index, status, output, error) => {
                    if let Some(step) = self.steps.get_mut(index) {
                        step.status = status;
                        step.output = output;
                    }
                    if let Some(error) = error {
                        self.error = error;
                    }
                }
                RunEvent::SessionFailed(error) => {
                    self.error = error;
                }
                RunEvent::Done => {
                    if self.cancel_requested {
                        self.error =
                            "Automation stopped. A background process that already started keeps running."
                                .to_string();
                    }
                    for step in &mut self.steps {
                        if step.status == StepStatus::Pending || step.status == StepStatus::Running
                        {
                            step.status = StepStatus::Skipped;
                        }
                    }
                    self.running = false;
                    self.finished = Some(Instant::now());
                }
            }
        }
        changed
    }

    pub(crate) fn succeeded(&self) -> bool {
        !self.running && self.error.is_empty()
    }
}

fn child_pids(pid: u32) -> Vec<u32> {
    Command::new("pgrep")
        .arg("-P")
        .arg(pid.to_string())
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .filter_map(|value| value.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn kill(pid: u32) {
    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
}

// Children first would let them be re-parented before we see them, so collect
// the whole subtree before killing anything.
fn kill_tree(pid: u32) {
    let mut all = vec![pid];
    let mut index = 0;
    while index < all.len() {
        let children = child_pids(all[index]);
        all.extend(children);
        index += 1;
    }
    for pid in all {
        kill(pid);
    }
}

fn run_worker(
    automation: Automation,
    cancel: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
    background_pids: Arc<Mutex<Vec<u32>>>,
    tx: mpsc::Sender<RunEvent>,
) {
    let session = automation_working_directory(&automation.path)
        .and_then(|dir| spawn_automation_session(&dir));
    let mut session = match session {
        Ok(session) => session,
        Err(error) => {
            let _ = tx.send(RunEvent::SessionFailed(format!(
                "Automation session could not be started: {}",
                error
            )));
            let _ = tx.send(RunEvent::Done);
            return;
        }
    };
    pid.store(session.child.id(), Ordering::SeqCst);

    for (index, step) in automation.steps.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let _ = tx.send(RunEvent::StepStarted(index));

        if step.kind == "wait" {
            let end = Instant::now() + Duration::from_secs(step.seconds);
            while Instant::now() < end && !cancel.load(Ordering::SeqCst) {
                thread::sleep(
                    Duration::from_millis(100).min(end.saturating_duration_since(Instant::now())),
                );
            }
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            let output = format!(
                "Waited {} {}.",
                step.seconds,
                if step.seconds == 1 {
                    "second"
                } else {
                    "seconds"
                }
            );
            let _ = tx.send(RunEvent::StepFinished(
                index,
                StepStatus::Success,
                output,
                None,
            ));
            continue;
        }

        if step.command.trim().is_empty() || step.command.len() > MAX_AUTOMATION_COMMAND_BYTES {
            let _ = tx.send(RunEvent::StepFinished(
                index,
                StepStatus::Error,
                format!(
                    "Command must contain at most {} bytes.",
                    MAX_AUTOMATION_COMMAND_BYTES
                ),
                Some(format!("Step {} could not be completed.", index + 1)),
            ));
            break;
        }

        let background = step.behavior == "background";
        match execute_in_session(&mut session, &step.command, background) {
            Ok(result) => {
                let output = [result.stdout.trim(), result.stderr.trim()]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                if background {
                    if let (Some(job), Ok(mut pids)) = (result.process_id, background_pids.lock()) {
                        pids.push(job);
                    }
                    let text = match result.process_id {
                        Some(pid) => format!("Started in background (PID {}).", pid),
                        None => "Started in background.".to_string(),
                    };
                    let _ = tx.send(RunEvent::StepFinished(
                        index,
                        StepStatus::Success,
                        text,
                        None,
                    ));
                } else if result.exit_code == Some(0) {
                    let text = if output.is_empty() {
                        "Command completed.".to_string()
                    } else {
                        output
                    };
                    let _ = tx.send(RunEvent::StepFinished(
                        index,
                        StepStatus::Success,
                        text,
                        None,
                    ));
                } else {
                    let text = if output.is_empty() {
                        format!(
                            "Command exited with code {}.",
                            result
                                .exit_code
                                .map(|code| code.to_string())
                                .unwrap_or_else(|| "unknown".to_string())
                        )
                    } else {
                        output
                    };
                    let _ = tx.send(RunEvent::StepFinished(
                        index,
                        StepStatus::Error,
                        text,
                        Some(format!(
                            "Step {} failed. Remaining steps were not started.",
                            index + 1
                        )),
                    ));
                    break;
                }
            }
            Err(error) => {
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                let _ = tx.send(RunEvent::StepFinished(
                    index,
                    StepStatus::Error,
                    error,
                    Some(format!("Step {} could not be completed.", index + 1)),
                ));
                break;
            }
        }
    }

    let _ = session.child.kill();
    let _ = session.child.wait();
    let _ = tx.send(RunEvent::Done);
}
