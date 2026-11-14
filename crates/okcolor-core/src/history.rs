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
pub struct History {
    current: Oklch,
    before_edit: Option<Oklch>,
    undo: VecDeque<Oklch>,
    redo: VecDeque<Oklch>,
    recent: VecDeque<Oklch>,
    history_limit: usize,
    recent_limit: usize,
}

impl History {
    pub fn new(initial: Oklch, history_limit: usize, recent_limit: usize) -> Self {
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

    pub fn current(&self) -> Oklch {
        self.current
    }
    /// Newest committed color first; undo/redo navigation does not add entries.
    pub fn recent(&self) -> &VecDeque<Oklch> {
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
        self.before_edit = Some(self.current);
        Ok(())
    }

    pub fn preview(&mut self, color: Oklch) -> Result<(), EditError> {
        self.before_edit.ok_or(EditError::NotEditing)?;
        self.current = color;
        Ok(())
    }

    pub fn commit(&mut self) -> Result<bool, EditError> {
        let before = self.before_edit.take().ok_or(EditError::NotEditing)?;
        if before == self.current {
            return Ok(false);
        }
        push_bounded(&mut self.undo, before, self.history_limit);
        self.redo.clear();
        self.recent.retain(|color| *color != self.current);
        self.recent.push_front(self.current);
        self.recent.truncate(self.recent_limit);
        Ok(true)
    }

    pub fn cancel(&mut self) -> Result<(), EditError> {
        self.current = self.before_edit.take().ok_or(EditError::NotEditing)?;
        Ok(())
    }

    /// A completed text entry, paste, preset or external burst is one transaction.
    pub fn set(&mut self, color: Oklch) -> Result<bool, EditError> {
        self.begin()?;
        self.preview(color)?;
        self.commit()
    }

    pub fn undo(&mut self) -> Result<bool, EditError> {
        self.ensure_idle()?;
        let Some(previous) = self.undo.pop_back() else {
            return Ok(false);
        };
        push_bounded(&mut self.redo, self.current, self.history_limit);
        self.current = previous;
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, EditError> {
        self.ensure_idle()?;
        let Some(next) = self.redo.pop_back() else {
            return Ok(false);
        };
        push_bounded(&mut self.undo, self.current, self.history_limit);
        self.current = next;
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

fn push_bounded(queue: &mut VecDeque<Oklch>, value: Oklch, limit: usize) {
    if limit == 0 {
        return;
    }
    if queue.len() == limit {
        queue.pop_front();
    }
    queue.push_back(value);
}
