//! Host-side ownership of two helper channels and the direct session sentinel.
use super::{
    protocol::{Channel, Frame, Kind},
    watch::ExitWatch,
};
use pty_runtime_domain::process::ProcessError;
use std::{
    fs::File,
    io::Read,
    os::unix::process::ExitStatusExt,
    process::{Child, ExitStatus},
    time::{Duration, Instant},
};

pub(super) struct Guardian {
    sentinel: Child,
    watch: Option<ExitWatch>,
    channels: [Option<Channel>; 2],
    generation: u64,
    hello: [Option<[i32; 4]>; 2],
    started: Option<[i32; 4]>,
    admitted: bool,
    execution_granted: bool,
    start_error: Option<ProcessError>,
    control: Option<Kind>,
    sent: [Option<Kind>; 2],
    retiring: [bool; 2],
    reaped: bool,
    observed_exit: Option<i32>,
    pending_exit: Option<ExitStatus>,
    fault: bool,
    pending_fault: bool,
    term_applied: bool,
    escalation_applied: bool,
    escalation_reported: bool,
    cleanup_host: Option<File>,
}
impl Guardian {
    pub fn new(
        sentinel: Child,
        channels: [Channel; 2],
        generation: u64,
        cleanup_host: File,
    ) -> Self {
        let watch = ExitWatch::new(sentinel.id()).ok();
        let [sentinel_channel, guardian_channel] = channels;
        Self {
            sentinel,
            watch,
            channels: [Some(sentinel_channel), Some(guardian_channel)],
            generation,
            hello: [None; 2],
            started: None,
            admitted: false,
            execution_granted: false,
            start_error: None,
            control: None,
            sent: [None; 2],
            retiring: [false; 2],
            reaped: false,
            observed_exit: None,
            pending_exit: None,
            fault: false,
            pending_fault: false,
            term_applied: false,
            escalation_applied: false,
            escalation_reported: false,
            cleanup_host: Some(cleanup_host),
        }
    }
    pub fn id(&self) -> u32 {
        self.started.map_or(0, |values| values[0] as u32)
    }
    pub fn admit(&mut self, host: &mut File) -> Result<(), ProcessError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let result = loop {
            self.service();
            if let Some(error) = self.start_error {
                break Err(error);
            }
            if self.fault {
                break Err(ProcessError::Internal);
            }
            if self.admitted {
                break Ok(());
            }
            if !self.execution_granted && self.hello.iter().all(Option::is_some) {
                for channel in self.channels.iter_mut().flatten() {
                    if channel
                        .enqueue(Frame::new(Kind::Execute, self.generation, [0; 4]))
                        .is_err()
                    {
                        self.start_error = Some(ProcessError::Io);
                    }
                }
                self.execution_granted = true;
                continue;
            }
            if Instant::now() >= deadline {
                break Err(ProcessError::Timeout);
            }
            self.pause(20);
        };
        if result.is_err() {
            self.cleanup(host);
        }
        result
    }
    pub fn request(&mut self, kind: Kind) {
        if self
            .control
            .is_none_or(|current| (kind as u16) > current as u16)
        {
            self.control = Some(kind);
        }
    }
    pub fn service(&mut self) {
        for index in 0..2 {
            if let Some(channel) = &mut self.channels[index] {
                if self.sent[index] != self.control {
                    if let Some(kind) = self.control {
                        if channel
                            .enqueue(Frame::new(kind, self.generation, [0; 4]))
                            .is_ok()
                        {
                            self.sent[index] = Some(kind);
                        }
                    }
                }
                // A final Retiring/status record may still be readable after a
                // racing control write fails. Classify loss only after reading it.
                let _ = channel.flush();
            }
            for _ in 0..8 {
                let Some(channel) = &mut self.channels[index] else {
                    break;
                };
                let message = channel.receive();
                let eof = channel.eof();
                match message {
                    Ok(Some(frame)) => self.receive(index, frame),
                    Ok(None) => {
                        if eof {
                            self.channel_ended(index);
                        }
                        break;
                    }
                    Err(error) => {
                        self.mark_fault();
                        if error.kind() != std::io::ErrorKind::InvalidData {
                            self.channel_ended(index);
                        }
                        break;
                    }
                }
            }
        }
        if !self.reaped {
            match self.sentinel.try_wait() {
                Ok(Some(_)) => {
                    self.reaped = true;
                    self.watch = None;
                }
                Ok(None) => {}
                Err(_) => {
                    // A competing host waiter violates the construction contract.
                    // Channel owners still perform cleanup; never signal a stale PID.
                    self.mark_fault();
                    self.watch = None;
                    if self.channels.iter().all(Option::is_none) {
                        self.reaped = true;
                    }
                }
            }
        }
    }
    fn channel_ended(&mut self, index: usize) {
        self.channels[index] = None;
        if !self.retiring[index] && self.start_error.is_none() {
            self.mark_fault();
        }
    }
    fn receive(&mut self, index: usize, frame: Frame) {
        let sid = self.sentinel.id() as i32;
        match frame.kind {
            Kind::Hello
                if frame.values[0] == sid
                    && frame.values[1] > 0
                    && frame.values[1] != sid
                    && frame.values[2..] == [0, 0] =>
            {
                if self.hello[index].is_some_and(|old| old != frame.values)
                    || self.hello[1 - index].is_some_and(|old| old != frame.values)
                {
                    self.mark_fault();
                } else {
                    self.hello[index] = Some(frame.values);
                }
            }
            Kind::Started | Kind::Admitted
                if frame.values[0] > 0
                    && frame.values[0] != sid
                    && frame.values[0] != frame.values[1]
                    && frame.values[2..] == [sid, sid]
                    && self.hello[index].is_some_and(|hello| hello[1] == frame.values[1]) =>
            {
                if (frame.kind == Kind::Started && index != 1)
                    || (frame.kind == Kind::Admitted && index != 0)
                    || self.started.is_some_and(|old| old != frame.values)
                {
                    self.mark_fault();
                } else {
                    self.started = Some(frame.values);
                    if frame.kind == Kind::Admitted {
                        self.admitted = true;
                    }
                }
            }
            Kind::WorkloadExit
                if frame.values[1] == self.id() as i32
                    && self.id() != 0
                    && (libc::WIFEXITED(frame.values[0]) || libc::WIFSIGNALED(frame.values[0])) =>
            {
                if self.observed_exit.is_some_and(|old| old != frame.values[0]) {
                    self.mark_fault();
                } else if self.observed_exit.is_none() {
                    self.observed_exit = Some(frame.values[0]);
                    self.pending_exit = Some(ExitStatus::from_raw(frame.values[0]));
                }
            }
            Kind::StartFailed if !self.admitted => {
                self.start_error = Some(super::error(std::io::Error::from_raw_os_error(
                    frame.values[0],
                )));
            }
            Kind::SignalApplied
                if index == 1
                    && self.id() != 0
                    && self.control.is_some()
                    && frame.values == [self.id() as i32, libc::SIGTERM, 7, 0] =>
            {
                self.term_applied = true;
            }
            Kind::Escalated
                if index == 1
                    && self.id() != 0
                    && frame.values == [self.id() as i32, libc::SIGKILL, 0, 0] =>
            {
                if !self.escalation_reported {
                    self.escalation_applied = true;
                    self.escalation_reported = true;
                }
            }
            Kind::Fault => self.mark_fault(),
            Kind::Retiring
                if frame.values[0] > 0
                    && frame.values[0] != self.id() as i32
                    && (index == 1 || frame.values[0] == sid) =>
            {
                self.retiring[index] = true
            }
            Kind::Successor
                if frame.values[0] > 0 && frame.values[1] > 0 && frame.values[2] == sid => {}
            _ => self.mark_fault(),
        }
    }
    fn mark_fault(&mut self) {
        if !self.fault {
            self.fault = true;
            self.pending_fault = true;
        }
        self.request(Kind::Abort);
    }
    pub fn take_exit(&mut self) -> Option<ExitStatus> {
        self.pending_exit.take()
    }
    pub fn take_escalation_applied(&mut self) -> bool {
        std::mem::take(&mut self.escalation_applied)
    }
    pub fn take_term_applied(&mut self) -> bool {
        std::mem::take(&mut self.term_applied)
    }
    pub fn take_fault(&mut self) -> bool {
        std::mem::take(&mut self.pending_fault)
    }
    pub fn cleanup_succeeded(&self) -> bool {
        self.complete() && !self.fault
    }
    pub fn complete(&self) -> bool {
        self.reaped && self.channels.iter().all(Option::is_none)
    }
    pub fn needs_poll(&self) -> bool {
        !self.reaped && self.watch.is_none()
    }
    pub fn pollfds(&self) -> [libc::pollfd; 3] {
        let mut fds = [libc::pollfd {
            fd: -1,
            events: libc::POLLIN,
            revents: 0,
        }; 3];
        fds[0].fd = self.watch.as_ref().map_or(-1, ExitWatch::fd);
        for (index, channel) in self.channels.iter().enumerate() {
            if let Some(channel) = channel {
                fds[index + 1].fd = channel.fd();
                if channel.wants_write() || self.sent[index] != self.control {
                    fds[index + 1].events |= libc::POLLOUT;
                }
            }
        }
        fds
    }
    fn pause(&self, millis: i32) {
        let mut fds = self.pollfds();
        // SAFETY: fixed initialized array; every descriptor remains owned by self.
        unsafe {
            libc::poll(fds.as_mut_ptr(), fds.len() as _, millis);
        }
    }
    pub fn cleanup(&mut self, host: &mut File) {
        self.request(Kind::Abort);
        while !self.complete() {
            self.service();
            discard(host);
            self.pause(10);
        }
    }
}
impl Drop for Guardian {
    fn drop(&mut self) {
        // A panic between native launch and registry admission must still close
        // the ownership protocol, drain discarded bytes, and reap S. The extra
        // owned host descriptor makes this RAII path independent of tuple/drop order.
        if let Some(mut host) = self.cleanup_host.take() {
            self.cleanup(&mut host);
        }
    }
}
pub(super) fn discard(host: &mut File) {
    let mut bytes = [0; 8192];
    for _ in 0..8 {
        match host.read(&mut bytes) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
}
