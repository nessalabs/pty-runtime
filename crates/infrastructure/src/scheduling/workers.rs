use std::{
    sync::{Condvar, Mutex},
    thread::{JoinHandle, ThreadId},
};

#[derive(Default)]
struct State {
    handles: Vec<JoinHandle<()>>,
    ids: Vec<ThreadId>,
    joining: bool,
}
/// Handle ownership stays outside worker state, so no worker/owner Arc cycle exists.
#[derive(Default)]
pub(super) struct Workers {
    state: Mutex<State>,
    changed: Condvar,
}
impl Workers {
    pub fn add(&self, handle: JoinHandle<()>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.ids.push(handle.thread().id());
        state.handles.push(handle);
    }
    pub fn join(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // Callbacks may request shutdown or release the last owner. Workers must
        // return to the run loop to finish; an external caller still joins them.
        if state.ids.contains(&std::thread::current().id()) {
            return;
        }
        while state.joining {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        state.joining = true;
        let handles = std::mem::take(&mut state.handles);
        drop(state);
        for handle in handles {
            let _ = handle.join();
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.joining = false;
        self.changed.notify_all();
    }
}
