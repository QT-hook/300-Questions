use rand::seq::SliceRandom;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Selection {
    Fixed { start: usize, end: usize },
    Random { count: usize },
}

impl Selection {
    pub fn indices(self, size: usize) -> Result<Vec<usize>, String> {
        match self {
            Self::Fixed { start, end } if start > 0 && start <= end && end <= size => {
                Ok((start - 1..end).collect())
            }
            Self::Random { count } if count > 0 && count <= size => {
                let mut indices: Vec<_> = (0..size).collect();
                indices.shuffle(&mut rand::rng());
                indices.truncate(count);
                Ok(indices)
            }
            _ => Err(format!(
                "题号和数量须在 1～{size} 内，且起始题号不能大于结束题号"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Answering,
    Feedback,
    Leaving,
    Entering,
    Finished,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    pub generation: u64,
    pub indices: Vec<usize>,
    pub position: usize,
    pub selected: Option<usize>,
    pub correct: usize,
    pub phase: Phase,
}

impl Session {
    pub fn new(selection: Selection, size: usize, generation: u64) -> Result<Self, String> {
        Ok(Self {
            generation,
            indices: selection.indices(size)?,
            position: 0,
            selected: None,
            correct: 0,
            phase: Phase::Answering,
        })
    }

    pub fn answer(&mut self, selected: usize, correct: usize) -> bool {
        if self.phase != Phase::Answering || selected >= 4 || correct >= 4 {
            return false;
        }
        self.selected = Some(selected);
        self.correct += usize::from(selected == correct);
        self.phase = Phase::Feedback;
        true
    }

    // Each timer carries both a session identity and its expected phase.
    pub fn advance(&mut self, generation: u64, position: usize, expected: Phase) -> bool {
        if self.generation != generation || self.position != position || self.phase != expected {
            return false;
        }
        match expected {
            Phase::Feedback => self.phase = Phase::Leaving,
            Phase::Leaving => {
                if self.position + 1 == self.indices.len() {
                    self.phase = Phase::Finished;
                } else {
                    self.position += 1;
                    self.selected = None;
                    self.phase = Phase::Entering;
                }
            }
            Phase::Entering => self.phase = Phase::Answering,
            _ => return false,
        }
        true
    }

    pub fn accuracy(&self) -> String {
        format!(
            "{:.1}",
            self.correct as f64 / self.indices.len() as f64 * 100.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn fixed_selection_is_inclusive_and_ordered() {
        assert_eq!(
            Selection::Fixed { start: 1, end: 300 }
                .indices(300)
                .unwrap(),
            (0..300).collect::<Vec<_>>()
        );
        assert_eq!(
            Selection::Fixed {
                start: 300,
                end: 300
            }
            .indices(300)
            .unwrap(),
            vec![299]
        );
    }

    #[test]
    fn selections_reject_zero_reverse_and_out_of_bounds() {
        for selection in [
            Selection::Fixed { start: 0, end: 1 },
            Selection::Fixed { start: 2, end: 1 },
            Selection::Fixed { start: 1, end: 301 },
            Selection::Random { count: 0 },
            Selection::Random { count: 301 },
        ] {
            assert!(selection.indices(300).is_err());
        }
    }

    #[test]
    fn random_selection_has_requested_size_and_no_duplicates() {
        for count in [1, 3, 300] {
            let indices = Selection::Random { count }.indices(300).unwrap();
            assert_eq!(indices.len(), count);
            assert_eq!(indices.iter().collect::<HashSet<_>>().len(), count);
            assert!(indices.iter().all(|&index| index < 300));
        }
    }

    #[test]
    fn first_click_locks_scoring_and_stale_timers_do_not_advance() {
        let mut session = Session::new(Selection::Random { count: 1 }, 300, 7).unwrap();
        assert!(session.answer(1, 1));
        assert!(!session.answer(1, 1));
        assert_eq!(session.correct, 1);
        assert!(!session.advance(6, 0, Phase::Feedback));
        assert!(!session.advance(7, 1, Phase::Feedback));
        assert!(!session.advance(7, 0, Phase::Leaving));
        assert_eq!(session.phase, Phase::Feedback);
        assert!(session.advance(7, 0, Phase::Feedback));
        assert!(!session.advance(7, 0, Phase::Feedback));
        assert!(session.advance(7, 0, Phase::Leaving));
        assert_eq!(session.phase, Phase::Finished);
        assert_eq!(session.accuracy(), "100.0");
        assert!(!session.answer(0, 0));
    }

    #[test]
    fn results_include_last_answer_and_format_one_decimal() {
        for (answers, expected) in [
            ([true, true, true], "100.0"),
            ([false, false, false], "0.0"),
            ([true, false, false], "33.3"),
        ] {
            let mut session = Session::new(Selection::Fixed { start: 1, end: 3 }, 300, 1).unwrap();
            for (position, correct) in answers.into_iter().enumerate() {
                assert!(session.answer(usize::from(!correct), 0));
                assert!(session.advance(1, position, Phase::Feedback));
                assert!(session.advance(1, position, Phase::Leaving));
                if position < 2 {
                    assert!(!session.answer(0, 0));
                    assert!(session.advance(1, position + 1, Phase::Entering));
                }
            }
            assert_eq!(session.phase, Phase::Finished);
            assert_eq!(session.accuracy(), expected);
        }
    }
}
