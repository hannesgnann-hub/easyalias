//! Interactive automation runs through one persistent shell process.

use crate::*;

// Spawns the one persistent cmd.exe an automation run drives its steps
// through, with a background thread streaming its merged stdout/stderr into
// a channel so `execute_in_session` can read command output synchronously.
// `/Q` suppresses cmd.exe echoing each line it reads from stdin back out.
pub(crate) fn spawn_automation_session(working_directory: &Path) -> Result<AutomationSessionHandle, String> {
    let mut child = Command::new("cmd")
        .arg("/Q")
        .current_dir(working_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Shell session could not be started: {}", error))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Shell session has no input stream.".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Shell session has no output stream.".to_string())?;

    let (sender, receiver) = mpsc::channel::<String>();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(text) => {
                    if sender.send(text).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(AutomationSessionHandle {
        child,
        stdin,
        output_rx: receiver,
    })
}

// Runs one command inside an already-running automation session, so `cd`
// and `set` variables from earlier steps are still in effect. Foreground
// commands wait for a completion sentinel carrying %ERRORLEVEL%; background
// commands only wait for confirmation that the job was started (`start /B`
// has no simple single-line way to report a PID, so background steps always
// report `process_id: None`), so a long-running dev server does not block
// the next step. The command is wrapped in parentheses so `2>&1` redirects
// the whole command line, including any internal `&&`/`|`, into stdout.
pub(crate) fn execute_in_session(
    session: &mut AutomationSessionHandle,
    command: &str,
    background: bool,
) -> Result<AutomationCommandResult, String> {
    let send_error = |error: std::io::Error| format!("Command could not be sent: {}", error);
    let recv_error = || "Automation session ended unexpectedly.".to_string();

    if background {
        write!(
            session.stdin,
            "start \"\" /B cmd /C \"{}\" >nul 2>&1\r\n",
            command
        )
        .map_err(send_error)?;
        write!(session.stdin, "echo {}none\r\n", AUTOMATION_BG_MARKER).map_err(send_error)?;
        session.stdin.flush().map_err(send_error)?;

        loop {
            let line = session.output_rx.recv().map_err(|_| recv_error())?;
            if line.trim_end_matches('\r').starts_with(AUTOMATION_BG_MARKER) {
                return Ok(AutomationCommandResult {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    process_id: None,
                });
            }
        }
    }

    write!(session.stdin, "({}) 2>&1\r\n", command).map_err(send_error)?;
    write!(session.stdin, "echo {}%ERRORLEVEL%\r\n", AUTOMATION_DONE_MARKER).map_err(send_error)?;
    session.stdin.flush().map_err(send_error)?;

    let mut collected = String::new();
    loop {
        let line = session.output_rx.recv().map_err(|_| recv_error())?;
        let trimmed_line = line.trim_end_matches('\r');
        if let Some(code) = trimmed_line.strip_prefix(AUTOMATION_DONE_MARKER) {
            return Ok(AutomationCommandResult {
                exit_code: code.trim().parse::<i32>().ok(),
                stdout: limited_output(collected.trim_end().as_bytes()),
                stderr: String::new(),
                process_id: None,
            });
        }
        collected.push_str(trimmed_line);
        collected.push('\n');
    }
}
