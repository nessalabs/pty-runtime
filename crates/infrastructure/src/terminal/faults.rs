//! Test-only native response faults. Terminal calls never fork; guards stay on
//! their arming thread and cannot leak a fault to concurrent terminal tests.
use super::ffi;
use std::{cell::RefCell, marker::PhantomData, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Point {
    Info,
    Rows,
    Cell,
    CellRetry,
    Feed,
    Resize,
    Checkpoint,
    History,
    Compress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ColorField {
    Foreground,
    Background,
    Underline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    Return(Point, i32),
    InvalidDimensions,
    InvalidCodepoint,
    InvalidColor(ColorField),
    InvalidUnderline,
}

struct Armed {
    fault: Fault,
    consumed: bool,
}
thread_local! {
    static ACTIVE: RefCell<Option<Armed>> = const { RefCell::new(None) };
}

pub(super) struct Guard(PhantomData<Rc<()>>);
impl Guard {
    pub fn arm(fault: Fault) -> Self {
        ACTIVE.with_borrow_mut(|slot| {
            assert!(slot.is_none(), "nested terminal faults are not supported");
            *slot = Some(Armed {
                fault,
                consumed: false,
            });
        });
        Self(PhantomData)
    }
    pub fn assert_consumed(&self) {
        ACTIVE.with_borrow(|slot| {
            assert!(
                slot.as_ref().is_some_and(|armed| armed.consumed),
                "native fault was not consumed"
            );
        });
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        ACTIVE.with_borrow_mut(|slot| *slot = None);
    }
}

fn take(point: Point) -> Option<Fault> {
    ACTIVE.with_borrow_mut(|slot| {
        let armed = slot.as_mut()?;
        let matches = match armed.fault {
            Fault::Return(target, _) => point == target,
            Fault::InvalidDimensions => point == Point::Info,
            Fault::InvalidCodepoint | Fault::InvalidColor(_) | Fault::InvalidUnderline => {
                point == Point::Cell
            }
        };
        if armed.consumed || !matches {
            return None;
        }
        armed.consumed = true;
        Some(armed.fault)
    })
}

pub(super) fn result(point: Point, actual: i32) -> i32 {
    match take(point) {
        Some(Fault::Return(_, replacement)) => {
            assert!(
                actual >= 0,
                "native operation failed before fault injection"
            );
            replacement
        }
        None => actual,
        unexpected => panic!("wrong terminal response hook: {unexpected:?}"),
    }
}

pub(super) fn info(actual: i32, info: &mut ffi::Info) -> i32 {
    match take(Point::Info) {
        Some(Fault::Return(_, replacement)) => {
            assert_eq!(actual, 0);
            replacement
        }
        Some(Fault::InvalidDimensions) => {
            assert_eq!(actual, 0);
            info.cols = 0;
            actual
        }
        None => actual,
        unexpected => panic!("wrong terminal info hook: {unexpected:?}"),
    }
}

pub(super) fn cell(actual: i32, text: &mut [u32], len: usize, style: &mut ffi::Style) -> i32 {
    let Some(fault) = take(Point::Cell) else {
        return actual;
    };
    assert_eq!(actual, 0, "cell fault requires a successful native read");
    assert!(
        len <= text.len(),
        "native cell length exceeds initialized storage"
    );
    match fault {
        Fault::Return(_, replacement) => return replacement,
        Fault::InvalidCodepoint => {
            assert!(len > 0, "codepoint fault requires a nonempty cell");
            text[0] = 0xd800;
        }
        Fault::InvalidColor(field) => match field {
            ColorField::Foreground => style.foreground.tag = u8::MAX,
            ColorField::Background => style.background.tag = u8::MAX,
            ColorField::Underline => style.underline_color.tag = u8::MAX,
        },
        Fault::InvalidUnderline => style.underline = u8::MAX,
        unexpected => panic!("wrong terminal cell hook: {unexpected:?}"),
    }
    actual
}
