use anyhow::{Context, Result};
use rat_widget::event::{HandleEvent, Regular};
use rat_widget::focus::{Focus, FocusBuilder, HasFocus};
use rat_widget::paragraph::Paragraph;
use rat_widget::text_input::TextInput;
use rat_widget::{
    event::ct_event, focus::impl_has_focus, paragraph::ParagraphState, text_input::TextInputState,
};
use ratatui::Frame;
use ratatui::crossterm::event;
use ratatui::style::{Color, Style};
use ratatui::widgets::StatefulWidget;
use ratatui_macros::vertical;
use std::io::{self, Read, Write};
use std::process::{Child, ChildStdin};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use crate::file_buffer::MemBuf;

#[derive(Default)]
pub struct App {
    input_state: TextInputState,
    child: Option<Child>,
    child_stdin: Option<ChildStdin>,
    stdout_rx: Option<Receiver<Vec<u8>>>,
    output: String,
    paragraph: ParagraphState,
    last_idx: Option<usize>,
    buf: MemBuf,
    should_quit: bool,
    focus: Option<Focus>,
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn run(&mut self) -> Result<()> {
        ratatui::run(|terminal| {
            while !self.should_quit {
                self.poll_stdout();
                self.update_child_state()?;

                terminal.try_draw(|f| {
                    self.draw(f);
                    let timeout = Duration::from_secs_f64(1.0 / 50.0);
                    if !event::poll(timeout)? {
                        return io::Result::Ok(());
                    }
                    let event = event::read()?;
                    self.handle_event(&event);
                    io::Result::Ok(())
                })?;
            }

            if let Some(child) = &mut self.child {
                match child.try_wait() {
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    Err(_) => {}
                }
            }

            anyhow::Ok(())
        })
    }

    fn handle_event(&mut self, event: &ratatui::crossterm::event::Event) {
        let f = build_focus(self).handle(event, Regular);
        if matches!(f, rat_widget::event::Outcome::Changed) {
            return;
        }

        match event {
            ct_event!(key press CONTROL-'c') => self.should_quit = true,
            ct_event!(keycode press Enter) => self.submit_input(),
            ct_event!(keycode press Up) if self.input_state.is_focused() => {
                if let Some(last_idx) = self.last_idx {
                    let new_last = last_idx.saturating_sub(1);
                    if let Some(nth) = self.buf.nth(new_last) {
                        self.input_state.set_text(nth);
                        self.last_idx = Some(new_last);
                    }
                } else {
                    let idx = self.buf.len().saturating_sub(1);
                    if let Some(nth) = self.buf.nth(idx) {
                        self.input_state.set_text(nth);
                        self.last_idx = Some(idx);
                    }
                }
            }
            _ => {
                self.input_state.handle(event, Regular);
                self.paragraph.handle(event, Regular);
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let buf = frame.buffer_mut();
        let [paragraph, input_area] = vertical![*=1, ==3].areas(area);

        let input = TextInput::default().style(Style::default().bg(Color::Rgb(59, 59, 59)));
        input.render(input_area, buf, &mut self.input_state);

        let para = Paragraph::new(self.output.as_str());
        para.render(paragraph, buf, &mut self.paragraph);
    }

    fn poll_stdout(&mut self) {
        let Some(rx) = &self.stdout_rx else {
            return;
        };

        while let Ok(chunk) = rx.try_recv() {
            self.output.push_str(&String::from_utf8_lossy(&chunk));
        }
    }

    fn submit_input(&mut self) {
        let input = self.input_state.text().to_string();
        if input.is_empty() {
            return;
        }

        self.buf.write(input.clone());
        self.last_idx = None;

        if let Some(stdin) = &mut self.child_stdin
            && writeln!(stdin, "{input}")
                .and_then(|_| stdin.flush())
                .is_err()
        {
            self.output.push_str("\nError writing to child stdin\n");
        }

        self.input_state.clear();
    }

    fn update_child_state(&mut self) -> Result<()> {
        let Some(child) = &mut self.child else {
            return Ok(());
        };

        if let Some(status) = child.try_wait().context("failed to poll child process")? {
            self.child_stdin = None;
            self.output
                .push_str(&format!("\n[process exited with {status}]\n"));
            self.should_quit = true;
        }

        Ok(())
    }

    pub fn set_handle(&mut self, mut child: Child) -> Result<()> {
        let stdout = child.stdout.take().context("child stdout was not piped")?;
        let stderr = child.stderr.take().context("child stderr was not piped")?;
        let stdin = child.stdin.take().context("child stdin was not piped")?;
        let (tx, rx) = mpsc::channel();

        spawn_pipe_reader(stdout, tx.clone(), "stdout");
        spawn_pipe_reader(stderr, tx, "stderr");

        self.child_stdin = Some(stdin);
        self.stdout_rx = Some(rx);
        self.child = Some(child);

        Ok(())
    }
}

fn spawn_pipe_reader<R>(mut reader: R, tx: mpsc::Sender<Vec<u8>>, label: &'static str)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buf = [0_u8; 4096];

        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => {
                    let _ = tx.send(format!("\nError reading {label}: {err}\n").into_bytes());
                    break;
                }
            }
        }
    });
}

impl_has_focus!(input_state, paragraph for App);

fn build_focus(state: &mut App) -> &mut Focus {
    let mut fb = FocusBuilder::new(state.focus.take());
    state.build(&mut fb);
    state.focus = Some(fb.build());
    state.focus.as_mut().expect("focus")
}
