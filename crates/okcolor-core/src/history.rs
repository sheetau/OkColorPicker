//! Bounded edit transactions. Pointer samples never become individual undo entries.
use crate::Oklch;
use std::{collections::VecDeque, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditError {
    AlreadyEditing,
    NotEditing,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::AlreadyEditing => "Commit or cancel the current edit first",
            Self::NotEditing => "Begin an edit before previewing or committing it",
        })
    }
}
impl std::error::Error for EditError {}

#[derive(Debug)]
pub struct History<T = Oklch> {
    current: T,
    before_edit: Option<T>,
    undo: VecDeque<T>,
    redo: VecDeque<T>,
    recent: VecDeque<T>,
    history_limit: usize,
    recent_limit: usize,
}

impl<T: Clone + PartialEq> History<T> {
    pub fn new(initial: T, history_limit: usize, recent_limit: usize) -> Self {
        Self {
            current: initial,
            before_edit: None,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            recent: VecDeque::new(),
            history_limit,
            recent_limit,
        }
    }

    pub fn current(&self) -> T {
        self.current.clone()
    }

    /// During a preview, this remains the last committed value.
    pub fn committed(&self) -> T {
        self.before_edit.as_ref().unwrap_or(&self.current).clone()
    }

    /// Restore recents without inventing undo steps. Input order is newest first.
    pub fn restored(
        initial: T,
        history_limit: usize,
        recent_limit: usize,
        recent: impl IntoIterator<Item = T>,
    ) -> Self {
        let mut history = Self::new(initial, history_limit, recent_limit);
        for color in recent {
            if history.recent.len() == recent_limit {
                break;
            }
            if !history.recent.contains(&color) {
                history.recent.push_back(color);
            }
        }
        history
    }
    /// Newest committed color first; undo/redo navigation does not add entries.
    pub fn recent(&self) -> &VecDeque<T> {
        &self.recent
    }
    pub fn is_editing(&self) -> bool {
        self.before_edit.is_some()
    }
    pub fn can_undo(&self) -> bool {
        !self.is_editing() && !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.is_editing() && !self.redo.is_empty()
    }

    pub fn begin(&mut self) -> Result<(), EditError> {
        self.ensure_idle()?;
        self.before_edit = Some(self.current.clone());
        Ok(())
    }

    pub fn preview(&mut self, color: T) -> Result<(), EditError> {
        self.before_edit.as_ref().ok_or(EditError::NotEditing)?;
        self.current = color;
        Ok(())
    }

    pub fn commit(&mut self) -> Result<bool, EditError> {
        self.commit_with_recent(true)
    }

    fn commit_with_recent(&mut self, add_recent: bool) -> Result<bool, EditError> {
        let before = self.before_edit.take().ok_or(EditError::NotEditing)?;
        if before == self.current {
            return Ok(false);
        }
        push_bounded(&mut self.undo, before, self.history_limit);
        self.redo.clear();
        if add_recent && self.recent_limit > 0 {
            self.recent.retain(|color| *color != self.current);
            self.recent.push_front(self.current.clone());
            self.recent.truncate(self.recent_limit);
        }
        Ok(true)
    }

    pub fn cancel(&mut self) -> Result<(), EditError> {
        self.current = self.before_edit.take().ok_or(EditError::NotEditing)?;
        Ok(())
    }

    /// A completed text entry, paste, preset or external burst is one transaction.
    pub fn set(&mut self, color: T) -> Result<bool, EditError> {
        self.begin()?;
        self.preview(color)?;
        self.commit()
    }

    /// A wheel adjustment remains undoable without adding intermediate recent colors.
    pub fn set_without_recent(&mut self, color: T) -> Result<bool, EditError> {
        self.begin()?;
        self.preview(color)?;
        self.commit_with_recent(false)
    }

    pub fn undo(&mut self) -> Result<bool, EditError> {
        self.ensure_idle()?;
        let Some(previous) = self.undo.pop_back() else {
            return Ok(false);
        };
        let current = std::mem::replace(&mut self.current, previous);
        push_bounded(&mut self.redo, current, self.history_limit);
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, EditError> {
        self.ensure_idle()?;
        let Some(next) = self.redo.pop_back() else {
            return Ok(false);
        };
        let current = std::mem::replace(&mut self.current, next);
        push_bounded(&mut self.undo, current, self.history_limit);
        Ok(true)
    }

    fn ensure_idle(&self) -> Result<(), EditError> {
        if self.is_editing() {
            Err(EditError::AlreadyEditing)
        } else {
            Ok(())
        }
    }
}

fn push_bounded<T>(queue: &mut VecDeque<T>, value: T, limit: usize) {
    if limit == 0 {
        return;
    }
    if queue.len() == limit {
        queue.pop_front();
    }
    queue.push_back(value);
}
